//! Thread and run endpoints

use std::convert::Infallible;
use std::time::Duration;

use axum::Json;
use axum::extract::{Path, State};
use axum::response::Sse;
use axum::response::sse::{Event, KeepAlive};
use futures::{StreamExt, stream};
use infraware_engine::{AgentEvent, ResumeResponse, RunInput};
use infraware_shared::{EngineStatus, Interrupt, Message, MessageRole, ThreadId};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

// === Input Validation Constants ===

/// Maximum number of messages per request.
/// Set to 100 to balance conversation context with request size.
/// Typical LLM context windows support 100+ messages.
const MAX_MESSAGES: usize = 100;

/// Maximum length of a single message content in bytes (100KB).
/// This prevents memory exhaustion from oversized payloads while
/// allowing reasonably large code blocks or documents.
const MAX_MESSAGE_LENGTH: usize = 100_000;

/// Valid message roles for the API.
const VALID_ROLES: &[&str] = &["user", "assistant", "system"];

/// SSE keep-alive interval in seconds.
/// Sends periodic keep-alive messages to detect disconnected clients
/// and prevent reverse proxies from timing out idle connections.
const SSE_KEEP_ALIVE_SECS: u64 = 15;

// === SSE Status Event Helpers ===

/// Create an SSE event for EngineStatus changes.
///
/// Status events are emitted at key points in the stream lifecycle:
/// - `Thinking` at stream start
/// - `Interrupted(...)` when HITL interrupt is received
/// - `Ready` at stream end
fn create_status_event(status: EngineStatus) -> Event {
    tracing::debug!(status = ?status, "Emitting EngineStatus SSE event");
    Event::default()
        .event("status")
        .data(serde_json::to_string(&status).unwrap_or_else(|_| "{}".to_string()))
}

/// Request to create a thread
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateThreadRequest {
    /// Optional metadata to associate with the thread
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Response after creating a thread
#[derive(Debug, Serialize, ToSchema)]
pub struct CreateThreadResponse {
    /// The unique identifier for the created thread
    pub thread_id: String,
}

/// Create a new conversation thread
#[utoipa::path(
    post,
    path = "/threads",
    tag = "threads",
    request_body = CreateThreadRequest,
    responses(
        (status = 200, description = "Thread created successfully", body = CreateThreadResponse),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn create_thread(
    State(state): State<AppState>,
    Json(req): Json<CreateThreadRequest>,
) -> Result<Json<CreateThreadResponse>, ApiError> {
    let thread_id = state.engine.create_thread(req.metadata).await?;

    Ok(Json(CreateThreadResponse {
        thread_id: thread_id.to_string(),
    }))
}

/// Request to start/resume a run
#[derive(Debug, Deserialize, ToSchema)]
pub struct StreamRunRequest {
    /// Assistant ID to use for the run
    #[allow(dead_code)]
    pub assistant_id: String,
    /// Stream modes to enable (e.g., "values", "updates", "messages")
    #[serde(default)]
    #[allow(dead_code)]
    pub stream_mode: Vec<String>,
    /// Input messages for the run
    #[serde(default)]
    pub input: Option<InputContainer>,
    /// Command for resuming a run (e.g., after HITL interrupt)
    #[serde(default)]
    pub command: Option<CommandContainer>,
}

/// Container for input messages
#[derive(Debug, Deserialize, ToSchema)]
pub struct InputContainer {
    /// List of messages to send
    pub messages: Vec<MessageInput>,
}

/// A single message input
#[derive(Debug, Deserialize, ToSchema)]
pub struct MessageInput {
    /// Message role: "user", "assistant", or "system"
    pub role: String,
    /// Message content text
    pub content: String,
}

/// Command container for resuming runs
#[derive(Debug, Deserialize, ToSchema)]
pub struct CommandContainer {
    /// Resume action: "approved" or "rejected"
    pub resume: String,
}

// === Validation Functions ===

/// Validate thread_id using shared validation logic
fn validate_thread_id(thread_id: &str) -> Result<(), ApiError> {
    ThreadId::validate_str(thread_id).map_err(|e| ApiError::bad_request(e.to_string()))
}

/// Validate message role
fn validate_role(role: &str) -> Result<MessageRole, ApiError> {
    match role {
        "user" => Ok(MessageRole::User),
        "assistant" => Ok(MessageRole::Assistant),
        "system" => Ok(MessageRole::System),
        _ => Err(ApiError::bad_request(format!(
            "invalid role '{}' (allowed: {})",
            role,
            VALID_ROLES.join(", ")
        ))),
    }
}

/// Validate input messages
fn validate_messages(
    input: &Option<InputContainer>,
    allow_empty_command_output: bool,
) -> Result<(), ApiError> {
    if let Some(container) = input {
        if container.messages.len() > MAX_MESSAGES {
            return Err(ApiError::bad_request(format!(
                "too many messages (max {})",
                MAX_MESSAGES
            )));
        }

        for (i, msg) in container.messages.iter().enumerate() {
            if msg.content.len() > MAX_MESSAGE_LENGTH {
                return Err(ApiError::bad_request(format!(
                    "message {} content too long (max {} bytes)",
                    i, MAX_MESSAGE_LENGTH
                )));
            }

            if msg.content.is_empty() {
                // For command_output resume, allow an empty second message (captured command output can be empty).
                let is_allowed_empty_command_output =
                    allow_empty_command_output && i == 1 && container.messages.len() >= 2;
                if is_allowed_empty_command_output {
                    continue;
                }
                return Err(ApiError::bad_request(format!(
                    "message {} content cannot be empty",
                    i
                )));
            }

            // Validate role
            validate_role(&msg.role)?;
        }
    }
    Ok(())
}

/// Start or resume a streaming run
///
/// Starts a new run or resumes an interrupted run (e.g., after HITL approval).
/// Returns Server-Sent Events (SSE) with agent responses.
#[utoipa::path(
    post,
    path = "/threads/{thread_id}/runs/stream",
    tag = "threads",
    request_body = StreamRunRequest,
    params(
        ("thread_id" = String, Path, description = "Thread identifier"),
    ),
    responses(
        (status = 200, description = "SSE stream of agent events"),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Unauthorized"),
        (status = 404, description = "Thread not found"),
    ),
    security(
        ("api_key" = [])
    )
)]
pub async fn stream_run(
    State(state): State<AppState>,
    Path(thread_id): Path<String>,
    Json(req): Json<StreamRunRequest>,
) -> Result<Sse<impl futures::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    // Validate inputs
    validate_thread_id(&thread_id)?;
    let allow_empty_command_output = req
        .command
        .as_ref()
        .map(|c| c.resume.as_str() == "command_output")
        .unwrap_or(false);
    validate_messages(&req.input, allow_empty_command_output)?;

    let thread_id = ThreadId::new(thread_id);

    // Determine if this is a resume or new run
    let event_stream = if let Some(command) = req.command {
        // Resume run
        let response = match command.resume.as_str() {
            "approved" => {
                // Check if there's user input (answer to question)
                if let Some(msg) = req.input.as_ref().and_then(|i| i.messages.first()) {
                    ResumeResponse::answer(&msg.content)
                } else {
                    ResumeResponse::approved()
                }
            }
            "rejected" => ResumeResponse::rejected(),
            "command_output" => {
                // Command was executed in terminal PTY, output captured
                // Expected format: two messages - first is command, second is output
                if let Some(input) = &req.input {
                    let messages: Vec<_> = input.messages.iter().collect();
                    if messages.len() >= 2 {
                        ResumeResponse::command_output(&messages[0].content, &messages[1].content)
                    } else if let Some(msg) = messages.first() {
                        // Fallback: single message contains output, command unknown
                        ResumeResponse::command_output("unknown", &msg.content)
                    } else {
                        ResumeResponse::command_output("unknown", "")
                    }
                } else {
                    ResumeResponse::command_output("unknown", "")
                }
            }
            _ => ResumeResponse::approved(),
        };

        state.engine.resume_run(&thread_id, response).await?
    } else {
        // New run
        let messages = req
            .input
            .map(|input| {
                input
                    .messages
                    .into_iter()
                    .map(|m| {
                        let role = match m.role.as_str() {
                            "user" => MessageRole::User,
                            "assistant" => MessageRole::Assistant,
                            "system" => MessageRole::System,
                            _ => MessageRole::User,
                        };
                        Message::new(role, m.content)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let input = RunInput::new(messages);
        state.engine.stream_run(&thread_id, input).await?
    };

    // Convert engine events to SSE events with explicit EngineStatus events
    //
    // Status event flow:
    // 1. Emit status: Thinking at stream start
    // 2. Emit status: Interrupted(...) when HITL interrupt is received
    // 3. Emit status: Ready at stream end or on error

    // Start stream with Thinking status
    tracing::debug!(thread_id = %thread_id, "Starting SSE stream");
    let thinking_event = stream::once(async {
        tracing::debug!("Emitting initial Thinking status");
        Ok::<Event, Infallible>(create_status_event(EngineStatus::thinking()))
    });

    // Main event stream with status events injected at appropriate points
    let main_stream = event_stream.flat_map(|result| {
        let events: Vec<Result<Event, Infallible>> = match result {
            Ok(agent_event) => {
                tracing::debug!(event = ?agent_event, "Processing AgentEvent");
                let mut events = vec![];

                // Convert agent event to SSE event
                let (event_type, data) = match &agent_event {
                    AgentEvent::Metadata { run_id } => {
                        ("metadata", serde_json::json!({ "run_id": run_id }))
                    }
                    AgentEvent::Message(msg) => (
                        "messages",
                        serde_json::json!([{
                            "role": msg.role,
                            "content": msg.content
                        }]),
                    ),
                    AgentEvent::Values { messages } => (
                        "values",
                        serde_json::json!({
                            "messages": messages.iter().map(|m| {
                                serde_json::json!({
                                    "type": match m.role {
                                        MessageRole::Assistant => "ai",
                                        _ => "human"
                                    },
                                    "content": m.content
                                })
                            }).collect::<Vec<_>>()
                        }),
                    ),
                    AgentEvent::Updates { interrupts } => {
                        let interrupt_data = interrupts.as_ref().map(|ints| {
                            ints.iter()
                                .map(|int| {
                                    serde_json::json!({
                                        "value": match int {
                                            Interrupt::CommandApproval { command, message, needs_continuation } => {
                                                serde_json::json!({
                                                    "command": command,
                                                    "message": message,
                                                    "needs_continuation": needs_continuation
                                                })
                                            }
                                            Interrupt::Question { question, options } => {
                                                serde_json::json!({
                                                    "question": question,
                                                    "options": options
                                                })
                                            }
                                        }
                                    })
                                })
                                .collect::<Vec<_>>()
                        });

                        (
                            "updates",
                            serde_json::json!({
                                "__interrupt__": interrupt_data
                            }),
                        )
                    }
                    AgentEvent::Error { message } => {
                        ("error", serde_json::json!({ "message": message }))
                    }
                    AgentEvent::End => ("end", serde_json::json!({})),
                    AgentEvent::Phase { phase } => {
                        ("phase", serde_json::json!({ "phase": phase }))
                    }
                };

                tracing::debug!(event_type = event_type, "Emitting SSE event");
                events.push(Ok(Event::default()
                    .event(event_type)
                    .data(data.to_string())));

                // Inject status events for interrupts and end
                match agent_event {
                    AgentEvent::Updates {
                        interrupts: Some(ref ints),
                    } if !ints.is_empty() => {
                        // Emit Interrupted status with the first interrupt
                        // Guard ensures ints is non-empty, so first() always succeeds
                        let int = &ints[0];
                        events.push(Ok(create_status_event(EngineStatus::interrupted(
                            int.clone(),
                        ))));
                    }
                    AgentEvent::End => {
                        // Emit Ready status when stream ends normally
                        events.push(Ok(create_status_event(EngineStatus::ready())));
                    }
                    AgentEvent::Error { .. } => {
                        // Emit Ready status on error (workflow terminates)
                        events.push(Ok(create_status_event(EngineStatus::ready())));
                    }
                    _ => {}
                }

                events
            }
            Err(e) => {
                vec![
                    Ok(Event::default()
                        .event("error")
                        .data(serde_json::json!({ "message": e.to_string() }).to_string())),
                    Ok(create_status_event(EngineStatus::ready())),
                ]
            }
        };

        stream::iter(events)
    });

    // Combine: Thinking status first, then main event stream
    let sse_stream = thinking_event.chain(main_stream);

    // Configure SSE with keep-alive to detect disconnected clients
    // and prevent reverse proxies from timing out idle connections
    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(SSE_KEEP_ALIVE_SECS))
            .text("keep-alive"),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input_with_messages(messages: Vec<(&str, &str)>) -> Option<InputContainer> {
        Some(InputContainer {
            messages: messages
                .into_iter()
                .map(|(role, content)| MessageInput {
                    role: role.to_string(),
                    content: content.to_string(),
                })
                .collect(),
        })
    }

    #[test]
    fn validate_messages_rejects_empty_content_by_default() {
        let input = input_with_messages(vec![("user", "docker logs"), ("user", "")]);
        assert!(
            validate_messages(&input, false).is_err(),
            "empty content must be rejected"
        );
    }

    #[test]
    fn validate_messages_allows_empty_second_message_for_command_output_resume() {
        let input = input_with_messages(vec![("user", "docker logs --tail 100 api"), ("user", "")]);
        validate_messages(&input, true).expect("empty command output should be accepted");
    }

    #[test]
    fn validate_messages_still_rejects_empty_first_message_for_command_output_resume() {
        let input = input_with_messages(vec![("user", ""), ("user", "some output")]);
        assert!(
            validate_messages(&input, true).is_err(),
            "first message cannot be empty"
        );
    }
}
