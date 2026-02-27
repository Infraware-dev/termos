//! Orchestrator for converting rig-core agent output to EventStream
//!
//! This module handles the execution of rig-rs agents with native tool support.
//! Tools are integrated using rig-rs's function calling system via `PromptHook`.

use std::sync::Arc;

use async_stream::stream;
use futures::StreamExt;
use rig::agent::{Agent, HookAction, PromptHook, ToolCallHookAction};
use rig::client::CompletionClient;
use rig::completion::{CompletionModel, CompletionResponse, Prompt};
use rig::providers::anthropic;
use rig::tool::Tool;
use tokio::sync::{RwLock, mpsc};

use super::config::RigEngineConfig;
use super::incident;
use super::state::{PendingInterrupt, ResumeContext, StateStore};
use super::tools::{
    AskUserArgs, AskUserTool, DiagnosticCommandTool, HitlMarker, ShellCommandArgs,
    ShellCommandTool, StartIncidentArgs, StartIncidentInvestigationTool,
};
use crate::engine::adapters::rig::memory::session::{MemoryStore, SaveMemoryTool};
use crate::engine::error::EngineError;
use crate::engine::shared::{AgentEvent, Message, MessageEvent, MessageRole, RunInput};
use crate::engine::traits::EventStream;
use crate::engine::types::ResumeResponse;

/// Type alias for the base rig-core agent
pub type RigAgent = Agent<anthropic::completion::CompletionModel>;

/// Intercepted tool call from the LLM
#[derive(Debug, Clone)]
pub(super) struct InterceptedToolCall {
    pub(super) tool_name: String,
    pub(super) tool_call_id: Option<String>,
    pub(super) args: String,
}

/// Hook for intercepting tool calls and enabling HITL (Human-in-the-Loop)
///
/// This hook is called by rig-rs when the LLM wants to execute a tool.
/// For tools that require user approval (shell commands, questions),
/// we intercept the call and cancel automatic execution.
#[derive(Clone)]
pub(super) struct HitlHook {
    /// Channel for sending intercepted tool calls back to the orchestrator
    pub(super) tool_call_tx: mpsc::UnboundedSender<InterceptedToolCall>,
}

impl std::fmt::Debug for HitlHook {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HitlHook")
            .field("tool_call_tx", &"<UnboundedSender>")
            .finish()
    }
}

impl PromptHook<anthropic::completion::CompletionModel> for HitlHook {
    fn on_tool_call(
        &self,
        tool_name: &str,
        tool_call_id: Option<String>,
        _internal_call_id: &str,
        args: &str,
    ) -> impl Future<Output = ToolCallHookAction> + Send {
        // Capture values for the async block
        let tool_name = tool_name.to_string();
        let args = args.to_string();
        let tx = self.tool_call_tx.clone();

        async move {
            // Intercept tools that require HITL approval/confirmation.
            if tool_name == <ShellCommandTool as Tool>::NAME
                || tool_name == <AskUserTool as Tool>::NAME
                || tool_name == <StartIncidentInvestigationTool as Tool>::NAME
                || tool_name == <DiagnosticCommandTool as Tool>::NAME
            {
                tracing::debug!(
                    tool_name = %tool_name,
                    tool_call_id = ?tool_call_id,
                    "Intercepted tool call for HITL"
                );

                let _ = tx.send(InterceptedToolCall {
                    tool_name,
                    tool_call_id,
                    args,
                });

                // Stop automatic tool execution - we'll handle it after user approval
                return ToolCallHookAction::Skip {
                    reason: "Intercepted for HITL approval".into(),
                };
            }

            ToolCallHookAction::cont()
        }
    }

    #[expect(clippy::manual_async_fn, reason = "Trait signature requires this form")]
    fn on_completion_response(
        &self,
        _prompt: &rig::completion::message::Message,
        _response: &CompletionResponse<
            <anthropic::completion::CompletionModel as CompletionModel>::Response,
        >,
    ) -> impl Future<Output = HookAction> + Send {
        async { HookAction::cont() }
    }
}

/// Create a rig-core agent with native tools registered
///
/// Tools are registered using `.tool()` which enables rig-rs's function calling.
/// The LLM will see the tool schemas and can call them directly.
pub fn create_agent(
    client: &anthropic::Client,
    config: &RigEngineConfig,
    memory_store: &Arc<RwLock<MemoryStore>>,
    memory: &MemoryStore,
) -> RigAgent {
    let memory_preamble = memory.build_preamble();

    client
        .agent(&config.model)
        .preamble(&config.system_prompt)
        .append_preamble(&memory_preamble)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(ShellCommandTool::new())
        .tool(AskUserTool::new())
        .tool(SaveMemoryTool::new(Arc::clone(memory_store)))
        .tool(StartIncidentInvestigationTool)
        .build()
}

/// Rig message type alias for convenience
type RigMessage = rig::completion::message::Message;

/// Convert conversation history to rig-core chat history format
fn to_chat_history(messages: &[Message]) -> Vec<RigMessage> {
    messages
        .iter()
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| match m.role {
            MessageRole::User => RigMessage::user(&m.content),
            MessageRole::Assistant => RigMessage::assistant(&m.content),
            MessageRole::System => RigMessage::user(&m.content),
        })
        .collect()
}

/// Process a potentially intercepted tool call and produce the HITL interrupt and SSE event.
///
/// Returns `Some((interrupt, event))` when the tool was recognized and should pause for HITL.
/// Returns `None` when the tool is unknown or the args cannot be parsed (execution continues).
fn handle_tool_intercept(
    intercepted: InterceptedToolCall,
) -> Option<(PendingInterrupt, AgentEvent)> {
    match intercepted.tool_name.as_str() {
        "execute_shell_command" => {
            let args = serde_json::from_str::<ShellCommandArgs>(&intercepted.args).ok()?;
            let pending = PendingInterrupt::command_approval_with_tool(
                args.command.clone(),
                args.explanation.clone(),
                args.needs_continuation,
                intercepted.tool_call_id,
                serde_json::from_str(&intercepted.args).ok(),
            );
            let event = AgentEvent::updates_with_interrupt(
                HitlMarker::CommandApproval {
                    command: args.command,
                    message: args.explanation,
                    needs_continuation: args.needs_continuation,
                }
                .into(),
            );
            Some((pending, event))
        }
        "ask_user" => {
            let args = serde_json::from_str::<AskUserArgs>(&intercepted.args).ok()?;
            let pending = PendingInterrupt::question_with_tool(
                args.question.clone(),
                args.options.clone(),
                intercepted.tool_call_id,
                serde_json::from_str(&intercepted.args).ok(),
            );
            let event = AgentEvent::updates_with_interrupt(
                HitlMarker::Question {
                    question: args.question,
                    options: args.options,
                }
                .into(),
            );
            Some((pending, event))
        }
        "start_incident_investigation" => {
            let args = serde_json::from_str::<StartIncidentArgs>(&intercepted.args).ok()?;
            let pending =
                PendingInterrupt::incident_confirmation(args.incident_description.clone());
            let question = format!(
                "Start multi-agent incident investigation?\n\nIncident: {}",
                args.incident_description
            );
            let event = AgentEvent::updates_with_interrupt(
                HitlMarker::Question {
                    question,
                    options: Some(vec!["Yes, start investigation".into(), "No, cancel".into()]),
                }
                .into(),
            );
            Some((pending, event))
        }
        _ => None,
    }
}

/// Result of a single agent continuation turn.
enum AgentTurnOutcome {
    ToolIntercepted {
        interrupt: Box<PendingInterrupt>,
        event: AgentEvent,
    },
    Response(String),
    Error(EngineError),
}

/// Run a single agent turn and return the outcome without streaming.
async fn run_agent_turn(
    client: &anthropic::Client,
    config: &RigEngineConfig,
    memory_store: &Arc<RwLock<MemoryStore>>,
    history: &[Message],
    continuation: &str,
) -> AgentTurnOutcome {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let hook = HitlHook { tool_call_tx: tx };

    let agent = {
        let memory = memory_store.read().await;
        create_agent(client, config, memory_store, &memory)
    };
    let mut chat_history = to_chat_history(history);

    let result = agent
        .prompt(continuation)
        .with_history(&mut chat_history)
        .max_turns(1)
        .with_hook(hook)
        .await;

    if let Ok(intercepted) = rx.try_recv()
        && let Some((interrupt, event)) = handle_tool_intercept(intercepted)
    {
        return AgentTurnOutcome::ToolIntercepted {
            interrupt: Box::new(interrupt),
            event,
        };
    }

    match result {
        Ok(response) => AgentTurnOutcome::Response(response),
        Err(e) => {
            AgentTurnOutcome::Error(EngineError::Other(anyhow::anyhow!("Agent error: {}", e)))
        }
    }
}

/// Emit events for a single agent continuation turn as a stream.
///
/// Stores any new tool interrupt, or emits the assistant response (skipping empty) and ends.
#[expect(
    clippy::too_many_arguments,
    reason = "All fields required to drive agent + state side-effects"
)]
fn handle_agent_continuation(
    client: Arc<anthropic::Client>,
    config: Arc<RigEngineConfig>,
    memory_store: Arc<RwLock<MemoryStore>>,
    state: Arc<StateStore>,
    thread_id: crate::engine::shared::ThreadId,
    run_id: String,
    history: Vec<Message>,
    continuation: String,
) -> EventStream {
    Box::pin(stream! {
        match run_agent_turn(&client, &config, &memory_store, &history, &continuation).await {
            AgentTurnOutcome::ToolIntercepted { interrupt, event } => {
                let _ = state.store_interrupt(&thread_id, *interrupt).await;
                yield Ok(event);
            }
            AgentTurnOutcome::Response(response) => {
                if !response.trim().is_empty() {
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&response)));
                    let _ = state.add_messages(&thread_id, vec![Message::assistant(&response)]).await;
                    yield Ok(AgentEvent::Values { messages: vec![Message::assistant(&response)] });
                }
                yield Ok(AgentEvent::end());
            }
            AgentTurnOutcome::Error(e) => {
                tracing::error!(run_id = %run_id, "Agent continuation failed");
                yield Err(e);
            }
        }
    })
}

/// Create an event stream from a new run
///
/// This function executes the agent with native tools and a `PromptHook`
/// to intercept tool calls for HITL (Human-in-the-Loop) approval.
pub fn create_run_stream(
    config: Arc<RigEngineConfig>,
    client: Arc<anthropic::Client>,
    memory_store: Arc<RwLock<MemoryStore>>,
    state: Arc<StateStore>,
    thread_id: crate::engine::shared::ThreadId,
    input: RunInput,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        // Emit metadata event
        yield Ok(AgentEvent::metadata(&run_id));

        // Clear any pending interrupt from previous query (user started new conversation)
        let _ = state.take_interrupt(&thread_id).await;

        // Get conversation history
        let history = state.get_messages(&thread_id).await.unwrap_or_default();

        // Add new input messages to history
        let _ = state.add_messages(&thread_id, input.messages.clone()).await;

        // Extract the user's prompt
        let prompt = input
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        if prompt.trim().is_empty() {
            yield Err(EngineError::Other(anyhow::anyhow!("No user message provided")));
            return;
        }

        // Setup channel for receiving intercepted tool calls
        let (tx, mut rx) = mpsc::unbounded_channel();
        let hook = HitlHook { tool_call_tx: tx };

        // Create the agent with native tools
        let agent = {
            let memory = memory_store.read().await;
            create_agent(&client, &config, &memory_store, &memory)
        };
        let chat_history = to_chat_history(&history);

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            prompt = %prompt,
            history_len = chat_history.len(),
            "Executing rig agent with native tools"
        );

        // Execute agent with hook to intercept tool calls
        // max_turns(1) allows one round of tool execution before stopping
        //
        // NOTE: We intentionally DON'T use chat_history for new queries.
        // Each query is independent - history was causing LLM confusion
        // where it would respond to old messages instead of the new prompt.
        // History is still stored for resume operations.
        let result = agent
            .prompt(&prompt)
            .max_turns(1)
            .with_hook(hook)
            .await;
        tracing::debug!("Agent returned result={:?}", result.as_ref().map(|_| "ok").unwrap_or("err"));

        // Check if a tool call was intercepted for HITL
        if let Ok(intercepted) = rx.try_recv() {
            tracing::debug!(
                run_id = %run_id,
                tool = %intercepted.tool_name,
                "Tool call intercepted for HITL"
            );

            if let Some((pending, event)) = handle_tool_intercept(intercepted) {
                let _ = state.store_interrupt(&thread_id, pending).await;
                yield Ok(event);
                return;
            }
            tracing::warn!("Unknown or invalid intercepted tool payload");
        }

        // No tool call intercepted - handle normal response
        match result {
            Ok(response) => {
                yield Ok(AgentEvent::Message(MessageEvent::assistant(&response)));
                let _ = state.add_messages(&thread_id, vec![Message::assistant(&response)]).await;

                yield Ok(AgentEvent::Values {
                    messages: vec![Message::assistant(&response)],
                });
                yield Ok(AgentEvent::end());
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "Agent execution failed");
                yield Err(EngineError::Other(anyhow::anyhow!("Agent error: {}", e)));
            }
        }
    })
}

/// Create an event stream from a resumed run
pub fn create_resume_stream(
    config: Arc<RigEngineConfig>,
    client: Arc<anthropic::Client>,
    memory_store: Arc<RwLock<MemoryStore>>,
    state: Arc<StateStore>,
    thread_id: crate::engine::shared::ThreadId,
    response: ResumeResponse,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        yield Ok(AgentEvent::metadata(&run_id));

        let pending = state.take_interrupt(&thread_id).await;
        if pending.is_none() {
            yield Err(EngineError::run_not_resumable(thread_id.as_str()));
            return;
        }

        let pending = pending.unwrap();

        match (&response, &pending.resume_context) {
            // Command rejected
            (ResumeResponse::Rejected, ResumeContext::CommandApproval { command, .. }) => {
                let response_text = format!(
                    "Command `{}` was rejected. Let me know if you'd like to try something else.",
                    command
                );

                yield Ok(AgentEvent::Message(MessageEvent::assistant(&response_text)));
                let _ = state.add_messages(&thread_id, vec![Message::assistant(&response_text)]).await;

                yield Ok(AgentEvent::Values {
                    messages: vec![Message::assistant(&response_text)],
                });
                yield Ok(AgentEvent::end());
            }

            // Command output from terminal PTY execution
            (ResumeResponse::CommandOutput { command, output }, ResumeContext::CommandApproval { needs_continuation, .. }) => {
                tracing::info!(command = %command, output_len = output.len(), needs_continuation = %needs_continuation, "Received command output from terminal");

                // Store in history for context
                let _ = state.add_messages(&thread_id, vec![
                    Message::user(format!("Executed command: {}\nOutput:\n{}", command, output.trim())),
                ]).await;

                // If needs_continuation is false, the command output directly answers the user's question.
                // Don't call the agent - just end the stream.
                if !needs_continuation {
                    tracing::debug!(run_id = %run_id, "Command output is final answer (needs_continuation=false), ending");
                    yield Ok(AgentEvent::end());
                    return;
                }

                // needs_continuation is true - agent needs to process the output to continue the task
                tracing::debug!(run_id = %run_id, "Command output needs processing (needs_continuation=true), calling agent");

                let continuation = format!(
                    "The command `{}` was executed. Output:\n\n{}\n\nPlease continue with your original task based on this information. \
    If you need to run more commands, use execute_shell_command.",
                    command, output.trim()
                );

                let history = state.get_messages(&thread_id).await.unwrap_or_default();
                let _ = state.add_messages(&thread_id, vec![Message::user(&continuation)]).await;

                for await event in handle_agent_continuation(
                    Arc::clone(&client), Arc::clone(&config), Arc::clone(&memory_store),
                    Arc::clone(&state), thread_id.clone(), run_id.clone(),
                    history, continuation,
                ) {
                    yield event;
                }
            }

            // Question answered - continue with agent using hook
            (ResumeResponse::Answer { text }, ResumeContext::Question { question }) => {
                let continuation = format!(
                    "The user answered the question \"{}\" with: {}\n\nPlease continue based on this response.",
                    question, text
                );

                let history = state.get_messages(&thread_id).await.unwrap_or_default();
                let _ = state.add_messages(&thread_id, vec![Message::user(&continuation)]).await;

                tracing::debug!(thread_id = %thread_id, run_id = %run_id, "Resuming rig agent with answer");

                for await event in handle_agent_continuation(
                    Arc::clone(&client), Arc::clone(&config), Arc::clone(&memory_store),
                    Arc::clone(&state), thread_id.clone(), run_id.clone(),
                    history, continuation,
                ) {
                    yield event;
                }
            }

            // Operator confirmed incident investigation — start the pipeline
            (ResumeResponse::Answer { text }, ResumeContext::IncidentConfirmation { incident_description }) => {
                let confirmed = text.trim().to_lowercase();
                let is_affirmative =
                    confirmed == "y" || confirmed == "yes" || confirmed.starts_with("yes");
                if !is_affirmative {
                    let msg = "Incident investigation cancelled.".to_string();
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&msg)));
                    yield Ok(AgentEvent::end());
                    return;
                }

                // Start investigation
                let mut investigation_stream = incident::start_investigation(
                    Arc::clone(&client),
                    Arc::clone(&config),
                    Arc::clone(&state),
                    thread_id.clone(),
                    incident_description.clone(),
                    run_id.clone(),
                );

                while let Some(event) = investigation_stream.next().await {
                    yield event;
                }
            }

            // Diagnostic command executed via terminal PTY
            (ResumeResponse::CommandOutput { output, .. }, ResumeContext::IncidentCommand { command, motivation, needs_continuation, risk_level, expected_diagnostic_value, context, .. }) => {
                let mut stream = incident::resume_investigation_with_output(
                    Arc::clone(&client),
                    Arc::clone(&config),
                    Arc::clone(&state),
                    thread_id.clone(),
                    command.clone(),
                    motivation.clone(),
                    *needs_continuation,
                    *risk_level,
                    expected_diagnostic_value.clone(),
                    context.clone(),
                    run_id.clone(),
                    output.clone(),
                );

                while let Some(event) = stream.next().await {
                    yield event;
                }
            }

            (ResumeResponse::Rejected, ResumeContext::IncidentCommand { command, .. }) => {
                let msg = format!("Diagnostic command `{}` rejected. Investigation stopped.", command);
                yield Ok(AgentEvent::Message(MessageEvent::assistant(&msg)));
                yield Ok(AgentEvent::end());
            }

            _ => {
                yield Err(EngineError::run_not_resumable("Invalid resume response for interrupt type"));
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_chat_history() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant("Hi there!"),
            Message::user("How are you?"),
        ];

        let history = to_chat_history(&messages);
        assert_eq!(history.len(), 3);
    }

    #[test]
    fn test_to_chat_history_filters_empty() {
        let messages = vec![
            Message::user("Hello"),
            Message::assistant(""),
            Message::user("How are you?"),
        ];

        let history = to_chat_history(&messages);
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_hitl_hook_intercepts_tools() {
        // Test that the tool names match what we expect
        assert_eq!(<ShellCommandTool as Tool>::NAME, "execute_shell_command");
        assert_eq!(<AskUserTool as Tool>::NAME, "ask_user");
    }
}
