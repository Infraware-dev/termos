//! LLM query management via the `Agent`.
//!
//! Provides `LlmController` which drives the agent directly and converts
//! its event stream into `AppBackgroundEvent` values for the terminal UI.

use std::sync::{Arc, RwLock, mpsc};

use futures::StreamExt as _;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

use super::AppBackgroundEvent;
use crate::agent::{Agent, AgentEvent, Interrupt, MockAgent, ResumeResponse, RunInput, ThreadId};
use crate::markdown::IncrementalRenderer;

/// Drives the `Agent` and converts its event stream into
/// `AppBackgroundEvent` values that the terminal UI can consume.
#[derive(Debug)]
pub struct LlmController {
    agent: Arc<dyn Agent>,
    thread_id: Arc<RwLock<Option<ThreadId>>>,
    /// Incremental renderer for streaming responses (markdown -> ANSI)
    pub incremental_renderer: IncrementalRenderer,
    /// Channel for background events (sender)
    bg_event_tx: mpsc::Sender<AppBackgroundEvent>,
    /// Channel for background events (receiver)
    bg_event_rx: mpsc::Receiver<AppBackgroundEvent>,
    /// Cancellation token for active LLM queries
    cancel_token: Option<CancellationToken>,
}

impl LlmController {
    /// Creates a new controller, selecting the agent from environment.
    pub fn new() -> Self {
        let (bg_event_tx, bg_event_rx) = mpsc::channel();
        let agent = Self::create_agent();

        Self {
            agent,
            thread_id: Arc::new(RwLock::new(None)),
            incremental_renderer: IncrementalRenderer::new(),
            bg_event_tx,
            bg_event_rx,
            cancel_token: None,
        }
    }

    /// Selects and initialises the agent based on `AGENT_TYPE` env var.
    fn create_agent() -> Arc<dyn Agent> {
        let agent_type = std::env::var("AGENT_TYPE").unwrap_or_else(|_| "rig".to_string());

        match agent_type.as_str() {
            #[cfg(feature = "rig")]
            "rig" => match crate::agent::RigAgent::from_env() {
                Ok(agent) => {
                    tracing::info!("Initialised RigAgent (Anthropic Claude)");
                    Arc::new(agent)
                }
                Err(e) => {
                    tracing::warn!("RigAgent init failed ({e}), falling back to MockAgent");
                    Arc::new(MockAgent::new(None))
                }
            },
            _ => {
                tracing::info!("Using MockAgent");
                let workflow = std::env::var("MOCK_WORKFLOW_FILE").ok().and_then(|path| {
                    let data = std::fs::read_to_string(&path).ok()?;
                    serde_json::from_str(&data).ok()
                });
                Arc::new(MockAgent::new(workflow))
            }
        }
    }

    /// Starts a new LLM query, spawning a background task that streams
    /// agent events and forwards them as `AppBackgroundEvent`.
    pub fn start_query(&mut self, runtime: &Runtime, text: String) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.incremental_renderer.reset();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let agent = Arc::clone(&self.agent);
        let tx = self.bg_event_tx.clone();
        let thread_id_lock = Arc::clone(&self.thread_id);

        runtime.spawn(async move {
            // Get or create a thread
            let thread_id = {
                let existing = thread_id_lock
                    .read()
                    .unwrap_or_else(|e| {
                        tracing::warn!("thread_id read lock was poisoned, recovering");
                        e.into_inner()
                    })
                    .clone();
                match existing {
                    Some(id) => id,
                    None => match agent.create_thread(None).await {
                        Ok(id) => {
                            *thread_id_lock.write().unwrap_or_else(|e| {
                                tracing::warn!("thread_id write lock was poisoned, recovering");
                                e.into_inner()
                            }) = Some(id.clone());
                            id
                        }
                        Err(e) => {
                            let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                                "Failed to create thread: {e}"
                            )));
                            return;
                        }
                    },
                }
            };

            let input = RunInput::single_user_message(text);
            let stream = match agent.stream_run(&thread_id, input).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                        "Failed to start run: {e}"
                    )));
                    return;
                }
            };

            Self::consume_event_stream(stream, &tx, &cancel_token).await;
        });
    }

    /// Resumes LLM run with command output from PTY execution.
    pub fn resume_with_command_output(
        &mut self,
        runtime: &Runtime,
        command: String,
        output: String,
    ) {
        self.spawn_resume(runtime, ResumeResponse::command_output(command, output));
    }

    /// Resumes LLM run with a text answer.
    pub fn resume_with_answer(&mut self, runtime: &Runtime, answer: String) {
        self.spawn_resume(runtime, ResumeResponse::answer(answer));
    }

    /// Resumes LLM run with rejection (user rejected the command).
    pub fn resume_rejected(&mut self, runtime: &Runtime) {
        self.spawn_resume(runtime, ResumeResponse::Rejected);
    }

    /// Cancels the active LLM query if one exists.
    pub fn cancel(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            tracing::info!("Cancelling active LLM stream");
            token.cancel();
        }
    }

    /// Polls and returns pending background events (non-blocking).
    #[must_use]
    pub fn poll_events(&mut self) -> Vec<AppBackgroundEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.bg_event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Spawns a background task that resumes an interrupted run.
    fn spawn_resume(&mut self, runtime: &Runtime, response: ResumeResponse) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.incremental_renderer.reset();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let agent = Arc::clone(&self.agent);
        let tx = self.bg_event_tx.clone();
        let thread_id_lock = Arc::clone(&self.thread_id);

        runtime.spawn(async move {
            let thread_id = thread_id_lock
                .read()
                .unwrap_or_else(|e| {
                    tracing::warn!("thread_id read lock was poisoned, recovering");
                    e.into_inner()
                })
                .clone();
            let Some(thread_id) = thread_id else {
                let _ = tx.send(AppBackgroundEvent::LlmError(
                    "No active thread for resume".to_string(),
                ));
                return;
            };

            let stream = match agent.resume_run(&thread_id, response).await {
                Ok(s) => s,
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!(
                        "Failed to resume run: {e}"
                    )));
                    return;
                }
            };

            Self::consume_event_stream(stream, &tx, &cancel_token).await;
        });
    }

    /// Consumes an agent event stream and forwards events to the UI channel.
    async fn consume_event_stream(
        mut stream: crate::agent::EventStream,
        tx: &mpsc::Sender<AppBackgroundEvent>,
        cancel_token: &CancellationToken,
    ) {
        let mut hitl_interrupted = false;

        while let Some(result) = stream.next().await {
            if cancel_token.is_cancelled() {
                return;
            }

            match result {
                Ok(AgentEvent::Message(msg)) => {
                    let _ = tx.send(AppBackgroundEvent::LlmChunk(msg.content));
                }
                Ok(AgentEvent::Updates { interrupts }) => {
                    if let Some(interrupts) = interrupts {
                        for interrupt in interrupts {
                            let event = match interrupt {
                                Interrupt::CommandApproval {
                                    command,
                                    message,
                                    needs_continuation,
                                } => AppBackgroundEvent::LlmCommandApproval {
                                    command,
                                    message,
                                    needs_continuation,
                                },
                                Interrupt::Question { question, options } => {
                                    AppBackgroundEvent::LlmQuestion { question, options }
                                }
                            };
                            hitl_interrupted = true;
                            let _ = tx.send(event);
                        }
                    }
                }
                Ok(AgentEvent::Phase { phase }) => {
                    let _ = tx.send(AppBackgroundEvent::LlmPhase(phase));
                }
                Ok(AgentEvent::Error { message }) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(message));
                }
                Ok(AgentEvent::End) => {
                    let _ = tx.send(AppBackgroundEvent::LlmStreamComplete);
                    return;
                }
                Ok(AgentEvent::Metadata { .. } | AgentEvent::Values { .. }) => {
                    // Agent-internal events; not displayed in terminal UI
                }
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!("Stream error: {e}")));
                    return;
                }
            }
        }

        // Stream ended without explicit End event.
        // If the stream was paused for HITL (interrupt emitted), don't signal
        // completion — the interaction will resume after user input.
        if !hitl_interrupted {
            let _ = tx.send(AppBackgroundEvent::LlmStreamComplete);
        }
    }
}
