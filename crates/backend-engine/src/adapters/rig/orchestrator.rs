//! Orchestrator for converting rig-core agent output to EventStream

use std::sync::Arc;

use async_stream::stream;
use rig::agent::Agent;
use rig::client::CompletionClient;
use rig::completion::{Chat, Prompt};
use rig::providers::anthropic;

use crate::error::EngineError;
use crate::traits::EventStream;
use crate::types::ResumeResponse;

use super::config::RigEngineConfig;
use super::state::{PendingInterrupt, StateStore};
use super::tools::{HitlMarker, parse_hitl_marker};

use infraware_shared::{AgentEvent, Message, MessageEvent, MessageRole, RunInput};

/// Type alias for a configured rig-core agent
pub type RigAgent = Agent<anthropic::completion::CompletionModel>;

/// Create a rig-core agent (no tools - we parse the response for commands)
pub fn create_agent(config: &RigEngineConfig) -> Result<RigAgent, EngineError> {
    // Create client with API key
    let client = anthropic::Client::new(&config.api_key).map_err(|e| {
        EngineError::Other(anyhow::anyhow!("Failed to create Anthropic client: {}", e))
    })?;

    // Create agent WITHOUT tools - we'll parse the response for [EXECUTE: ...] patterns
    let agent = client
        .agent(&config.model)
        .preamble(&config.system_prompt)
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .build();

    Ok(agent)
}

/// Parse response for [EXECUTE: command] pattern
fn parse_execute_pattern(response: &str) -> Option<(String, String)> {
    // Look for [EXECUTE: command]
    let execute_start = response.find("[EXECUTE:")?;
    let command_start = execute_start + "[EXECUTE:".len();
    let command_end = response[command_start..].find(']')?;
    let command = response[command_start..command_start + command_end]
        .trim()
        .to_string();

    // Get the explanation (everything after the command pattern)
    let explanation = response[command_start + command_end + 1..]
        .trim()
        .to_string();

    Some((command, explanation))
}

/// Parse response for [QUESTION: ...] pattern
fn parse_question_pattern(response: &str) -> Option<(String, Option<Vec<String>>)> {
    let question_start = response.find("[QUESTION:")?;
    let q_start = question_start + "[QUESTION:".len();
    let q_end = response[q_start..].find(']')?;
    let question = response[q_start..q_start + q_end].trim().to_string();

    // Check for options
    let options = if let Some(opt_start) = response.find("[OPTIONS:") {
        let o_start = opt_start + "[OPTIONS:".len();
        if let Some(o_end) = response[o_start..].find(']') {
            let opts_str = response[o_start..o_start + o_end].trim();
            Some(opts_str.split(',').map(|s| s.trim().to_string()).collect())
        } else {
            None
        }
    } else {
        None
    };

    Some((question, options))
}

/// Rig message type alias for convenience
type RigMessage = rig::completion::message::Message;

/// Convert conversation history to rig-core chat history format
fn to_chat_history(messages: &[Message]) -> Vec<RigMessage> {
    messages
        .iter()
        // Filter out messages with empty content (Anthropic API rejects these)
        .filter(|m| !m.content.trim().is_empty())
        .map(|m| match m.role {
            MessageRole::User => RigMessage::user(&m.content),
            MessageRole::Assistant => RigMessage::assistant(&m.content),
            MessageRole::System => RigMessage::user(&m.content), // System as user for history
        })
        .collect()
}

/// Create an event stream from a new run
pub fn create_run_stream(
    config: Arc<RigEngineConfig>,
    state: Arc<StateStore>,
    thread_id: infraware_shared::ThreadId,
    input: RunInput,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        // Emit metadata event
        yield Ok(AgentEvent::metadata(&run_id));

        // Get conversation history
        let history = state.get_messages(&thread_id).await.unwrap_or_default();

        // Add new input messages to history
        let _ = state.add_messages(&thread_id, input.messages.clone()).await;

        // Extract the user's prompt from input
        let prompt = input
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("\n");

        // Validate prompt is not empty
        if prompt.trim().is_empty() {
            yield Err(EngineError::Other(anyhow::anyhow!("No user message provided")));
            return;
        }

        // Create the agent
        let agent = match create_agent(&config) {
            Ok(a) => a,
            Err(e) => {
                yield Err(e);
                return;
            }
        };

        // Build chat history for the agent
        let chat_history = to_chat_history(&history);

        // Execute the agent
        tracing::debug!(thread_id = %thread_id, run_id = %run_id, history_len = chat_history.len(), "Executing rig agent");

        // Use multi_turn(1) to allow a single tool call
        // The tool will return a HITL marker which should come through in the response
        let result = if chat_history.is_empty() {
            agent.prompt(&prompt).multi_turn(1).await
        } else {
            agent.chat(&prompt, chat_history).await
        };

        match result {
            Ok(response) => {
                // Check for [EXECUTE: command] pattern
                if let Some((command, explanation)) = parse_execute_pattern(&response) {
                    tracing::debug!(run_id = %run_id, command = %command, "Command detected in response");

                    let pending = PendingInterrupt::command_approval(command.clone(), explanation.clone());
                    let _ = state.store_interrupt(&thread_id, pending).await;

                    let marker = HitlMarker::command_approval(&command, &explanation);
                    yield Ok(AgentEvent::updates_with_interrupt(marker.into()));
                }
                // Check for [QUESTION: ...] pattern
                else if let Some((question, options)) = parse_question_pattern(&response) {
                    tracing::debug!(run_id = %run_id, question = %question, "Question detected in response");

                    let pending = PendingInterrupt::question(question.clone(), options.clone());
                    let _ = state.store_interrupt(&thread_id, pending).await;

                    let marker = HitlMarker::question(&question, options);
                    yield Ok(AgentEvent::updates_with_interrupt(marker.into()));
                }
                // Regular completion
                else {
                    yield Ok(AgentEvent::Message(MessageEvent::assistant(&response)));
                    let _ = state.add_messages(&thread_id, vec![Message::assistant(&response)]).await;

                    yield Ok(AgentEvent::Values {
                        messages: vec![Message::assistant(&response)],
                    });
                    yield Ok(AgentEvent::end());
                }
            }
            Err(e) => {
                tracing::error!(run_id = %run_id, error = ?e, "Agent execution failed");
                yield Err(EngineError::Other(anyhow::anyhow!("Agent error: {}", e)));
            }
        }
    })
}

/// Execute a shell command and return the output
fn execute_command(command: &str) -> String {
    use std::process::Command;

    let output = if cfg!(target_os = "windows") {
        Command::new("cmd").args(["/C", command]).output()
    } else {
        Command::new("sh").arg("-c").arg(command).output()
    };

    match output {
        Ok(output) => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);

            if output.status.success() {
                if stdout.trim().is_empty() && stderr.trim().is_empty() {
                    "(Command executed successfully, no output)".to_string()
                } else {
                    format!("{}{}", stdout, stderr)
                }
            } else {
                format!(
                    "Exit code: {}\n{}{}",
                    output.status.code().unwrap_or(-1),
                    stdout,
                    stderr
                )
            }
        }
        Err(e) => format!("Failed to execute command: {}", e),
    }
}

/// Create an event stream from a resumed run
pub fn create_resume_stream(
    config: Arc<RigEngineConfig>,
    state: Arc<StateStore>,
    thread_id: infraware_shared::ThreadId,
    response: ResumeResponse,
    run_id: String,
) -> EventStream {
    Box::pin(stream! {
        // Emit metadata event
        yield Ok(AgentEvent::metadata(&run_id));

        // Get the pending interrupt
        let pending = state.take_interrupt(&thread_id).await;
        if pending.is_none() {
            yield Err(EngineError::run_not_resumable(thread_id.as_str()));
            return;
        }

        let pending = pending.unwrap();

        match (&response, &pending.resume_context) {
            // Command approved - execute it directly
            (ResumeResponse::Approved, super::state::ResumeContext::CommandApproval { command }) => {
                tracing::info!(command = %command, "Executing approved command");

                // Execute the command
                let output = execute_command(command);

                // Format response
                let response_text = format!("```\n$ {}\n{}\n```", command, output.trim());

                // Emit the result
                yield Ok(AgentEvent::Message(MessageEvent::assistant(&response_text)));

                // Store in history
                let _ = state.add_messages(&thread_id, vec![
                    Message::user(format!("Execute: {}", command)),
                    Message::assistant(&response_text),
                ]).await;

                yield Ok(AgentEvent::Values {
                    messages: vec![Message::assistant(&response_text)],
                });
                yield Ok(AgentEvent::end());
            }

            // Command rejected - inform the user
            (ResumeResponse::Rejected, super::state::ResumeContext::CommandApproval { command }) => {
                let response_text = format!("Command `{}` was rejected. Let me know if you'd like to try something else.", command);

                yield Ok(AgentEvent::Message(MessageEvent::assistant(&response_text)));
                let _ = state.add_messages(&thread_id, vec![Message::assistant(&response_text)]).await;

                yield Ok(AgentEvent::Values {
                    messages: vec![Message::assistant(&response_text)],
                });
                yield Ok(AgentEvent::end());
            }

            // Question answered - continue with agent
            (ResumeResponse::Answer { text }, super::state::ResumeContext::Question { question }) => {
                let continuation = format!(
                    "The user answered the question \"{}\" with: {}\n\nPlease continue based on this response.",
                    question, text
                );

                // Get conversation history
                let history = state.get_messages(&thread_id).await.unwrap_or_default();
                let _ = state.add_messages(&thread_id, vec![Message::user(&continuation)]).await;

                // Create the agent
                let agent = match create_agent(&config) {
                    Ok(a) => a,
                    Err(e) => {
                        yield Err(e);
                        return;
                    }
                };

                let chat_history = to_chat_history(&history);
                tracing::debug!(thread_id = %thread_id, run_id = %run_id, "Resuming rig agent with answer");

                let result = if chat_history.is_empty() {
                    agent.prompt(&continuation).await
                } else {
                    agent.chat(&continuation, chat_history).await
                };

                match result {
                    Ok(response) => {
                        if let Some(marker) = parse_hitl_marker(&response) {
                            let pending = match &marker {
                                HitlMarker::CommandApproval { command, message } => {
                                    PendingInterrupt::command_approval(command.clone(), message.clone())
                                }
                                HitlMarker::Question { question, options } => {
                                    PendingInterrupt::question(question.clone(), options.clone())
                                }
                            };
                            let _ = state.store_interrupt(&thread_id, pending).await;
                            yield Ok(AgentEvent::updates_with_interrupt(marker.into()));
                        } else {
                            yield Ok(AgentEvent::Message(MessageEvent::assistant(&response)));
                            let _ = state.add_messages(&thread_id, vec![Message::assistant(&response)]).await;
                            yield Ok(AgentEvent::Values {
                                messages: vec![Message::assistant(&response)],
                            });
                            yield Ok(AgentEvent::end());
                        }
                    }
                    Err(e) => {
                        tracing::error!(run_id = %run_id, error = ?e, "Agent resume failed");
                        yield Err(EngineError::Other(anyhow::anyhow!("Agent error: {}", e)));
                    }
                }
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
            Message::assistant(""), // Should be filtered out
            Message::user("How are you?"),
        ];

        let history = to_chat_history(&messages);
        assert_eq!(history.len(), 2);
    }
}
