//! HTTP Engine - Proxy to LangGraph server
//!
//! This engine forwards requests to a LangGraph server running on a configurable URL.
//! It translates between our AgenticEngine trait and the LangGraph HTTP/SSE protocol.

use async_trait::async_trait;
use futures::{Stream, StreamExt};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::error::EngineError;
use crate::traits::{AgenticEngine, EventStream};
use crate::types::{HealthStatus, ResumeResponse};
use crate::{AgentEvent, Interrupt, Message, MessageRole, RunInput, ThreadId};

/// Configuration for the HTTP engine
#[derive(Debug, Clone)]
pub struct HttpEngineConfig {
    /// Base URL of the LangGraph server (e.g., "http://localhost:2024")
    pub base_url: String,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Assistant ID for the agent
    pub assistant_id: String,
}

impl Default for HttpEngineConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:2024".to_string(),
            timeout_secs: 300,
            assistant_id: "supervisor".to_string(),
        }
    }
}

impl HttpEngineConfig {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            ..Default::default()
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_assistant_id(mut self, assistant_id: impl Into<String>) -> Self {
        self.assistant_id = assistant_id.into();
        self
    }
}

/// HTTP Engine that proxies to a LangGraph server
#[derive(Debug)]
pub struct HttpEngine {
    config: HttpEngineConfig,
    client: Client,
}

impl HttpEngine {
    pub fn new(config: HttpEngineConfig) -> Result<Self, EngineError> {
        let client = Client::builder()
            .timeout(std::time::Duration::from_secs(config.timeout_secs))
            .build()
            .map_err(|e| EngineError::Connection(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self { config, client })
    }

    pub fn with_default_config() -> Result<Self, EngineError> {
        Self::new(HttpEngineConfig::default())
    }

    /// Build URL for a given path
    fn url(&self, path: &str) -> String {
        format!("{}{}", self.config.base_url, path)
    }
}

// === LangGraph Protocol Types ===

#[derive(Debug, Serialize)]
struct CreateThreadRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct CreateThreadResponse {
    thread_id: String,
}

#[derive(Debug, Serialize)]
struct StreamRunRequest {
    assistant_id: String,
    stream_mode: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<InputContainer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<CommandContainer>,
}

#[derive(Debug, Serialize)]
struct InputContainer {
    messages: Vec<LangGraphMessage>,
}

#[derive(Debug, Serialize)]
struct LangGraphMessage {
    role: String,
    content: String,
}

#[derive(Debug, Serialize)]
struct CommandContainer {
    resume: String,
}

// === SSE Parsing ===

/// Parse SSE events from a byte stream into AgentEvents
///
/// Uses efficient buffer management to avoid O(n²) allocations:
/// - Drains processed lines instead of creating new strings
/// - Only allocates when necessary for event data
fn parse_sse_stream(
    byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
) -> EventStream {
    let stream = async_stream::stream! {
        let mut buffer = String::new();
        let mut current_event: Option<String> = None;

        tokio::pin!(byte_stream);

        while let Some(chunk_result) = byte_stream.next().await {
            match chunk_result {
                Ok(chunk) => {
                    // Note: from_utf8_lossy may replace invalid UTF-8 with replacement chars
                    let text = String::from_utf8_lossy(&chunk);
                    buffer.push_str(&text);

                    // Process complete lines efficiently using drain
                    while let Some(newline_pos) = buffer.find('\n') {
                        // Extract line without trailing newline/whitespace
                        let line: String = buffer.drain(..=newline_pos).collect();
                        let line = line.trim_end();

                        if line.is_empty() {
                            continue;
                        }

                        // Parse SSE line
                        if let Some(event_type) = line.strip_prefix("event: ") {
                            current_event = Some(event_type.trim().to_string());
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            if let Some(ref event) = current_event {
                                if let Some(agent_event) = parse_sse_event(event, data) {
                                    yield Ok(agent_event);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("SSE stream error: {}", e);
                    yield Err(EngineError::Connection(format!("Stream error: {}", e)));
                    break;
                }
            }
        }
    };

    Box::pin(stream)
}

/// Parse a single SSE event into an AgentEvent
fn parse_sse_event(event_type: &str, data: &str) -> Option<AgentEvent> {
    match event_type {
        "metadata" => {
            if let Ok(meta) = serde_json::from_str::<serde_json::Value>(data) {
                let run_id = meta
                    .get("run_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                Some(AgentEvent::metadata(run_id))
            } else {
                None
            }
        }
        "values" => {
            if let Ok(values) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(msgs) = values.get("messages").and_then(|v| v.as_array()) {
                    let messages: Vec<Message> = msgs
                        .iter()
                        .filter_map(|m| {
                            let role = match m.get("type").and_then(|v| v.as_str()) {
                                Some("ai") => MessageRole::Assistant,
                                Some("human") => MessageRole::User,
                                _ => match m.get("role").and_then(|v| v.as_str()) {
                                    Some("assistant") => MessageRole::Assistant,
                                    Some("user") => MessageRole::User,
                                    Some("system") => MessageRole::System,
                                    _ => return None,
                                },
                            };

                            let content = extract_content(m)?;
                            Some(Message::new(role, content))
                        })
                        .collect();

                    if !messages.is_empty() {
                        return Some(AgentEvent::Values { messages });
                    }
                }
            }
            None
        }
        "updates" => {
            if let Ok(updates) = serde_json::from_str::<serde_json::Value>(data) {
                if let Some(interrupts) = updates.get("__interrupt__").and_then(|v| v.as_array()) {
                    let parsed_interrupts: Vec<Interrupt> = interrupts
                        .iter()
                        .filter_map(|int| {
                            let value = int.get("value")?;
                            Some(parse_interrupt(value))
                        })
                        .collect();

                    if !parsed_interrupts.is_empty() {
                        return Some(AgentEvent::Updates {
                            interrupts: Some(parsed_interrupts),
                        });
                    }
                }
            }
            None
        }
        "error" => {
            if let Ok(error) = serde_json::from_str::<serde_json::Value>(data) {
                let message = error
                    .get("message")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown error")
                    .to_string();
                Some(AgentEvent::error(message))
            } else {
                None
            }
        }
        "end" => Some(AgentEvent::end()),
        _ => {
            tracing::trace!("Unknown SSE event type: {}", event_type);
            None
        }
    }
}

/// Extract content from a LangGraph message (handles both string and array formats)
fn extract_content(msg: &serde_json::Value) -> Option<String> {
    // Try string content first
    if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
        if !content.is_empty() {
            return Some(content.to_string());
        }
    }

    // Try array of content blocks
    if let Some(content_array) = msg.get("content").and_then(|v| v.as_array()) {
        let text_parts: Vec<String> = content_array
            .iter()
            .filter_map(|block| {
                if block.get("type").and_then(|v| v.as_str()) == Some("text") {
                    block
                        .get("text")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect();

        if !text_parts.is_empty() {
            return Some(text_parts.join("\n"));
        }
    }

    None
}

/// Parse an interrupt value to determine type
fn parse_interrupt(value: &serde_json::Value) -> Interrupt {
    if value.get("command").is_some() {
        let command = value
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let message = value
            .get("message")
            .and_then(|v| v.as_str())
            .unwrap_or("Command requires approval")
            .to_string();
        Interrupt::command_approval(command, message)
    } else {
        let question = value
            .get("question")
            .or_else(|| value.get("message"))
            .and_then(|v| v.as_str())
            .unwrap_or("Agent is asking for input")
            .to_string();
        let options = value.get("options").and_then(|v| v.as_array()).map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        });
        Interrupt::question(question, options)
    }
}

#[async_trait]
impl AgenticEngine for HttpEngine {
    async fn create_thread(
        &self,
        metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError> {
        let url = self.url("/threads");
        tracing::info!(url = %url, "Creating thread");

        let request = CreateThreadRequest { metadata };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    EngineError::Connection(format!(
                        "Cannot connect to LangGraph server at {}: {}",
                        self.config.base_url, e
                    ))
                } else {
                    EngineError::Other(e.into())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(EngineError::Other(anyhow::anyhow!(
                "Failed to create thread ({}): {}",
                status,
                error_text
            )));
        }

        let thread_response: CreateThreadResponse = response.json().await.map_err(|e| {
            EngineError::Other(anyhow::anyhow!("Failed to parse thread response: {}", e))
        })?;

        let thread_id = thread_response.thread_id;
        tracing::info!(thread_id = %thread_id, "Thread created");
        Ok(ThreadId::new(thread_id))
    }

    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError> {
        let url = self.url(&format!("/threads/{}/runs/stream", thread_id));
        tracing::info!(url = %url, "Starting stream run");

        let messages: Vec<LangGraphMessage> = input
            .messages
            .iter()
            .map(|m| LangGraphMessage {
                role: match m.role {
                    MessageRole::User => "user".to_string(),
                    MessageRole::Assistant => "assistant".to_string(),
                    MessageRole::System => "system".to_string(),
                },
                content: m.content.clone(),
            })
            .collect();

        let request = StreamRunRequest {
            assistant_id: self.config.assistant_id.clone(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: Some(InputContainer { messages }),
            command: None,
        };

        let response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| {
                if e.is_connect() {
                    EngineError::Connection(format!("Cannot connect to LangGraph server: {}", e))
                } else {
                    EngineError::Other(e.into())
                }
            })?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(EngineError::Other(anyhow::anyhow!(
                "Stream run failed ({}): {}",
                status,
                error_text
            )));
        }

        let byte_stream = response.bytes_stream();
        Ok(parse_sse_stream(byte_stream))
    }

    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError> {
        let url = self.url(&format!("/threads/{}/runs/stream", thread_id));
        tracing::info!(url = %url, ?response, "Resuming run");

        let mut request = StreamRunRequest {
            assistant_id: self.config.assistant_id.clone(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: None,
            command: Some(CommandContainer {
                resume: "approved".to_string(),
            }),
        };

        // If it's an answer, include the user's response
        if let ResumeResponse::Answer { text } = response {
            request.input = Some(InputContainer {
                messages: vec![LangGraphMessage {
                    role: "user".to_string(),
                    content: text,
                }],
            });
        }

        let http_response = self
            .client
            .post(&url)
            .json(&request)
            .send()
            .await
            .map_err(|e| EngineError::Connection(format!("Resume request failed: {}", e)))?;

        if !http_response.status().is_success() {
            let status = http_response.status();
            let error_text = http_response.text().await.unwrap_or_default();
            return Err(EngineError::Other(anyhow::anyhow!(
                "Resume run failed ({}): {}",
                status,
                error_text
            )));
        }

        let byte_stream = http_response.bytes_stream();
        Ok(parse_sse_stream(byte_stream))
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        // Try to hit the LangGraph server
        let url = self.url("/ok");

        match self.client.get(&url).send().await {
            Ok(response) if response.status().is_success() => Ok(HealthStatus::healthy()
                .with_details(serde_json::json!({
                    "engine": "http",
                    "langgraph_url": self.config.base_url,
                    "status": "connected"
                }))),
            Ok(response) => Ok(HealthStatus::unhealthy(format!(
                "LangGraph returned status {}",
                response.status()
            ))),
            Err(e) => Ok(HealthStatus::unhealthy(format!(
                "Cannot connect to LangGraph at {}: {}",
                self.config.base_url, e
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = HttpEngineConfig::default();
        assert_eq!(config.base_url, "http://localhost:2024");
        assert_eq!(config.timeout_secs, 300);
    }

    #[test]
    fn test_config_builder() {
        let config = HttpEngineConfig::new("http://example.com:8080").with_timeout(60);
        assert_eq!(config.base_url, "http://example.com:8080");
        assert_eq!(config.timeout_secs, 60);
    }

    #[test]
    fn test_parse_interrupt_command() {
        let value = serde_json::json!({
            "command": "rm -rf temp/",
            "message": "Clean up temp files"
        });
        let interrupt = parse_interrupt(&value);
        match interrupt {
            Interrupt::CommandApproval { command, message } => {
                assert_eq!(command, "rm -rf temp/");
                assert_eq!(message, "Clean up temp files");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_parse_interrupt_question() {
        let value = serde_json::json!({
            "question": "Which environment?",
            "options": ["dev", "prod"]
        });
        let interrupt = parse_interrupt(&value);
        match interrupt {
            Interrupt::Question { question, options } => {
                assert_eq!(question, "Which environment?");
                assert_eq!(options, Some(vec!["dev".to_string(), "prod".to_string()]));
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_extract_content_string() {
        let msg = serde_json::json!({
            "content": "Hello world"
        });
        assert_eq!(extract_content(&msg), Some("Hello world".to_string()));
    }

    #[test]
    fn test_extract_content_array() {
        let msg = serde_json::json!({
            "content": [
                {"type": "text", "text": "Part 1"},
                {"type": "text", "text": "Part 2"}
            ]
        });
        assert_eq!(extract_content(&msg), Some("Part 1\nPart 2".to_string()));
    }

    #[test]
    fn test_extract_content_empty() {
        let msg = serde_json::json!({
            "content": ""
        });
        assert_eq!(extract_content(&msg), None);
    }
}
