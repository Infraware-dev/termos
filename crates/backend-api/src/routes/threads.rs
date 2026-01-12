//! Thread and run endpoints

use axum::{
    Json,
    extract::{Path, State},
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use std::{convert::Infallible, time::Duration};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;
use infraware_engine::{AgentEvent, ResumeResponse, RunInput};
use infraware_shared::{Message, MessageRole, ThreadId};

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
fn validate_messages(input: &Option<InputContainer>) -> Result<(), ApiError> {
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
    validate_messages(&req.input)?;

    let thread_id = ThreadId::new(thread_id);

    // Determine if this is a resume or new run
    let event_stream = if let Some(command) = req.command {
        // Resume run
        let response = match command.resume.as_str() {
            "approved" => {
                // Check if there's user input (answer to question)
                if let Some(input) = &req.input {
                    if let Some(msg) = input.messages.first() {
                        ResumeResponse::answer(&msg.content)
                    } else {
                        ResumeResponse::approved()
                    }
                } else {
                    ResumeResponse::approved()
                }
            }
            "rejected" => ResumeResponse::rejected(),
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

    // Convert engine events to SSE events
    let sse_stream = event_stream.map(|result| {
        let event = match result {
            Ok(agent_event) => {
                let (event_type, data) = match agent_event {
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
                        let interrupt_data = interrupts.map(|ints| {
                            ints.into_iter()
                                .map(|int| {
                                    serde_json::json!({
                                        "value": match int {
                                            infraware_engine::Interrupt::CommandApproval { command, message } => {
                                                serde_json::json!({
                                                    "command": command,
                                                    "message": message
                                                })
                                            }
                                            infraware_engine::Interrupt::Question { question, options } => {
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
                };

                Event::default()
                    .event(event_type)
                    .data(data.to_string())
            }
            Err(e) => Event::default()
                .event("error")
                .data(serde_json::json!({ "message": e.to_string() }).to_string()),
        };

        Ok(event)
    });

    // Configure SSE with keep-alive to detect disconnected clients
    // and prevent reverse proxies from timing out idle connections
    Ok(Sse::new(sse_stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(SSE_KEEP_ALIVE_SECS))
            .text("keep-alive"),
    ))
}
