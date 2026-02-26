//! LLM query management via the `AgenticEngine`.
//!
//! Provides `LlmController` which drives the engine directly and converts
//! its event stream into `AppBackgroundEvent` values for the terminal UI.

use std::sync::{Arc, RwLock, mpsc};

use futures::StreamExt as _;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

use super::AppBackgroundEvent;
use crate::engine::{
    AgentEvent, AgenticEngine, Interrupt, MockEngine, ResumeResponse, RunInput, ThreadId,
};
use crate::llm::IncrementalRenderer;

/// Drives the `AgenticEngine` and converts its event stream into
/// `AppBackgroundEvent` values that the terminal UI can consume.
#[derive(Debug)]
pub struct LlmController {
    engine: Arc<dyn AgenticEngine>,
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
    /// Creates a new controller, selecting the engine from environment.
    pub fn new() -> Self {
        let (bg_event_tx, bg_event_rx) = mpsc::channel();
        let engine = Self::create_engine();

        Self {
            engine,
            thread_id: Arc::new(RwLock::new(None)),
            incremental_renderer: IncrementalRenderer::new(),
            bg_event_tx,
            bg_event_rx,
            cancel_token: None,
        }
    }

    /// Selects and initialises the engine based on `ENGINE_TYPE` env var.
    fn create_engine() -> Arc<dyn AgenticEngine> {
        let engine_type = std::env::var("ENGINE_TYPE").unwrap_or_else(|_| "rig".to_string());

        match engine_type.as_str() {
            #[cfg(feature = "rig")]
            "rig" => match crate::engine::RigEngine::from_env() {
                Ok(engine) => {
                    tracing::info!("Initialised RigEngine (Anthropic Claude)");
                    Arc::new(engine)
                }
                Err(e) => {
                    tracing::warn!("RigEngine init failed ({e}), falling back to MockEngine");
                    Arc::new(MockEngine::new(None))
                }
            },
            _ => {
                tracing::info!("Using MockEngine");
                let workflow = std::env::var("MOCK_WORKFLOW_FILE").ok().and_then(|path| {
                    let data = std::fs::read_to_string(&path).ok()?;
                    serde_json::from_str(&data).ok()
                });
                Arc::new(MockEngine::new(workflow))
            }
        }
    }

    /// Starts a new LLM query, spawning a background task that streams
    /// engine events and forwards them as `AppBackgroundEvent`.
    pub fn start_query(&mut self, runtime: &Runtime, text: String) {
        if let Some(token) = self.cancel_token.take() {
            token.cancel();
        }

        self.incremental_renderer.reset();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        let engine = Arc::clone(&self.engine);
        let tx = self.bg_event_tx.clone();
        let thread_id_lock = Arc::clone(&self.thread_id);

        runtime.spawn(async move {
            // Get or create a thread
            let thread_id = {
                let existing = thread_id_lock
                    .read()
                    .expect("thread_id lock poisoned")
                    .clone();
                match existing {
                    Some(id) => id,
                    None => match engine.create_thread(None).await {
                        Ok(id) => {
                            *thread_id_lock.write().expect("thread_id lock poisoned") =
                                Some(id.clone());
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
            let stream = match engine.stream_run(&thread_id, input).await {
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

    /// Resumes LLM run with plain command approval (engine executes command).
    pub fn resume_approved(&mut self, runtime: &Runtime) {
        self.spawn_resume(runtime, ResumeResponse::Approved);
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

        let engine = Arc::clone(&self.engine);
        let tx = self.bg_event_tx.clone();
        let thread_id_lock = Arc::clone(&self.thread_id);

        runtime.spawn(async move {
            let thread_id = thread_id_lock
                .read()
                .expect("thread_id lock poisoned")
                .clone();
            let Some(thread_id) = thread_id else {
                let _ = tx.send(AppBackgroundEvent::LlmError(
                    "No active thread for resume".to_string(),
                ));
                return;
            };

            let stream = match engine.resume_run(&thread_id, response).await {
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

    /// Consumes an engine event stream and forwards events to the UI channel.
    async fn consume_event_stream(
        mut stream: crate::engine::EventStream,
        tx: &mpsc::Sender<AppBackgroundEvent>,
        cancel_token: &CancellationToken,
    ) {
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
                    // Backend-only concerns; skip
                }
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(format!("Stream error: {e}")));
                    return;
                }
            }
        }

        // Stream ended without explicit End event
        let _ = tx.send(AppBackgroundEvent::LlmStreamComplete);
    }
}
