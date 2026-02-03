/// LLM client for natural language queries
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{Duration, timeout};
use tokio_util::sync::CancellationToken;

/// Idle timeout for SSE streams - if no data received for this duration, the stream is considered dead.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(120);

/// Safely truncate a UTF-8 string to at most `max_bytes` bytes,
/// ensuring the result ends at a valid char boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    // Find the last valid char boundary at or before max_bytes
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Request to create a new LLM thread
#[derive(Debug, Serialize)]
struct CreateThreadRequest {
    metadata: serde_json::Value,
}

/// Response from creating a thread
#[derive(Debug, Deserialize)]
struct CreateThreadResponse {
    thread_id: String,
}

/// Request for streaming run via POST /threads/{id}/runs/stream
#[derive(Debug, Serialize)]
struct StreamRunRequest {
    assistant_id: String,
    stream_mode: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    input: Option<StreamInput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<StreamCommand>,
}

/// Input container for streaming request
#[derive(Debug, Serialize)]
struct StreamInput {
    messages: Vec<ChatMessage>,
}

/// Chat message for LLM conversation
#[derive(Debug, Serialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Command for resuming interrupted runs
#[derive(Debug, Serialize)]
struct StreamCommand {
    resume: String,
}

/// Result of an LLM query - complete, command approval, or question
#[derive(Debug, Clone)]
pub enum LLMQueryResult {
    /// Query completed with a final response
    Complete(String),
    /// Query interrupted - LLM wants to execute a command and needs approval (y/n)
    CommandApproval {
        /// The command the LLM wants to execute
        command: String,
        /// Description/reason from the LLM
        message: String,
    },
    /// Query interrupted - LLM is asking a question (free-form text answer)
    Question {
        /// The question being asked
        question: String,
        /// Optional predefined choices
        options: Option<Vec<String>>,
    },
}

impl LLMQueryResult {
    /// Returns the response text if Complete, or None otherwise
    #[cfg(test)]
    pub fn as_complete(&self) -> Option<&str> {
        match self {
            LLMQueryResult::Complete(s) => Some(s),
            LLMQueryResult::CommandApproval { .. } | LLMQueryResult::Question { .. } => None,
        }
    }

    /// Unwraps a Complete result, panics if not Complete
    #[cfg(test)]
    pub fn unwrap_complete(self) -> String {
        match self {
            LLMQueryResult::Complete(s) => s,
            LLMQueryResult::CommandApproval { command, .. } => {
                panic!(
                    "Expected Complete, got CommandApproval for command: {}",
                    command
                )
            }
            LLMQueryResult::Question { question, .. } => {
                panic!("Expected Complete, got Question: {}", question)
            }
        }
    }
}

/// Internal result from SSE stream parsing
#[derive(Debug)]
enum StreamResult {
    /// Stream completed with response text
    Complete(String),
    /// Stream interrupted with command approval request (y/n)
    CommandApproval { command: String, message: String },
    /// Stream interrupted with question (free-form text answer)
    Question {
        question: String,
        options: Option<Vec<String>>,
    },
}

/// Internal interrupt data parsed from SSE events
#[derive(Debug)]
enum InterruptData {
    /// Command approval interrupt (y/n response)
    CommandApproval { command: String, message: String },
    /// Question interrupt (free-form text response)
    Question {
        question: String,
        options: Option<Vec<String>>,
    },
}

/// Trait for LLM client implementations
///
/// This trait allows different LLM backends (mock, HTTP, OpenAI, etc.)
/// to be used interchangeably via dependency injection
#[async_trait]
pub trait LLMClientTrait: Send + Sync + std::fmt::Debug {
    /// Query the LLM with natural language input.
    /// Returns LLMQueryResult which can be Complete or Interrupted (for HITL).
    async fn query(&self, text: &str) -> Result<LLMQueryResult>;

    /// Resume an interrupted run with a text answer (for questions)
    async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult>;

    /// Resume with command output after PTY execution.
    /// Called when a command was executed in the terminal and output was captured.
    async fn resume_with_command_output(
        &self,
        command: &str,
        output: &str,
    ) -> Result<LLMQueryResult>;

    /// Resume an interrupted run with rejection (user rejected the command)
    async fn resume_rejected(&self) -> Result<LLMQueryResult>;

    /// Query with cancellation support (default: no cancellation)
    async fn query_cancellable(
        &self,
        text: &str,
        _cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        // Default: ignore cancellation token
        self.query(text).await
    }
}

/// HTTP-based LLM client for production use
pub struct HttpLLMClient {
    base_url: String,
    client: reqwest::Client,
    /// API key for authentication
    api_key: String,
    /// Cached thread ID for conversation continuity
    thread_id: RwLock<Option<String>>,
}

impl std::fmt::Debug for HttpLLMClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpLLMClient")
            .field("base_url", &self.base_url)
            .field("client", &"<reqwest::Client>")
            .field("api_key", &"<redacted>")
            .field("thread_id", &"<RwLock<Option<String>>>")
            .finish()
    }
}

impl HttpLLMClient {
    /// Create a new HTTP LLM client with API key
    pub fn new(base_url: String, api_key: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder()
                .connect_timeout(std::time::Duration::from_secs(10)) // Connection timeout
                .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))) // Force IPv4
                .pool_max_idle_per_host(0) // Disable connection pooling for SSE
                .build()
                .unwrap_or_default(),
            api_key,
            thread_id: RwLock::new(None),
        }
    }

    /// Create a new LLM thread via POST /threads
    async fn create_thread(&self) -> Result<String> {
        let url = format!("{}/threads", self.base_url);
        log::info!("[HTTP-OUT] POST {}", url);

        let request = CreateThreadRequest {
            metadata: serde_json::json!({}),
        };

        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let elapsed = request_start.elapsed();
        log::info!(
            "[HTTP-IN] POST /threads | status={} | elapsed={}ms",
            response.status(),
            elapsed.as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Failed to create thread ({}): {}", status, error_text);
            anyhow::bail!("Failed to create thread ({}): {}", status, error_text);
        }

        let thread_response: CreateThreadResponse = response.json().await?;
        log::info!("Created LLM thread: {}", thread_response.thread_id);

        // Cache the thread ID
        *self.thread_id.write().await = Some(thread_response.thread_id.clone());

        Ok(thread_response.thread_id)
    }

    /// Get existing thread ID or create a new one
    async fn ensure_thread(&self) -> Result<String> {
        // Check if we already have a thread
        if let Some(id) = self.thread_id.read().await.clone() {
            return Ok(id);
        }

        // Create a new thread
        self.create_thread().await
    }

    /// Stream a run via POST /threads/{thread_id}/runs/stream
    ///
    /// # Arguments
    /// * `thread_id` - The thread ID to run on
    /// * `input` - Optional user input text (None for resume)
    /// * `resume` - Whether this is resuming an interrupted run
    ///
    /// # Returns
    /// `StreamResult::Complete` with response text, or
    /// `StreamResult::Interrupted` with command approval details
    async fn stream_run(
        &self,
        thread_id: &str,
        input: Option<&str>,
        resume: bool,
        cancel_token: CancellationToken,
    ) -> Result<StreamResult> {
        let url = format!("{}/threads/{}/runs/stream", self.base_url, thread_id);
        log::info!(
            "[HTTP-OUT] POST {} | input={} | resume={}",
            url,
            input.is_some(),
            resume
        );

        let mut request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: None,
            command: None,
        };

        if let Some(text) = input {
            request.input = Some(StreamInput {
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: text.into(),
                }],
            });
        }

        if resume {
            request.command = Some(StreamCommand {
                resume: "approved".into(),
            });
        }

        let request_start = std::time::Instant::now();

        // Use tokio::select! to race HTTP request vs cancellation
        let response = tokio::select! {
            result = self
                .client
                .post(&url)
                .header("X-Api-Key", &self.api_key)
                .json(&request)
                .send() => {
                    result?
            }
            _ = cancel_token.cancelled() => {
                log::info!("HTTP request cancelled before response");
                anyhow::bail!("Query cancelled by user")
            }
        };

        log::info!(
            "[HTTP-IN] POST /runs/stream | status={} | elapsed={}ms",
            response.status(),
            request_start.elapsed().as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Stream run failed ({}): {}", status, error_text);
            anyhow::bail!("Stream run failed ({}): {}", status, error_text);
        }

        log::info!("Starting SSE stream parsing...");
        self.parse_sse_stream(response, cancel_token).await
    }

    /// Parse SSE stream and accumulate AI messages
    /// Returns StreamResult::Complete, CommandApproval, or Question
    async fn parse_sse_stream(
        &self,
        response: reqwest::Response,
        cancel_token: CancellationToken,
    ) -> Result<StreamResult> {
        let mut result = String::new();
        let mut interrupt_data: Option<InterruptData> = None;
        let mut stream = response.bytes_stream();
        let mut current_event: Option<String> = None;
        let mut buffer = String::new();
        let mut chunk_count: u32 = 0;
        let stream_start = std::time::Instant::now();

        log::info!("SSE stream started, waiting for chunks...");

        loop {
            // Use timeout to detect idle streams (no data for STREAM_IDLE_TIMEOUT)
            let chunk_result = match timeout(STREAM_IDLE_TIMEOUT, stream.next()).await {
                Ok(Some(chunk)) => chunk,
                Ok(None) => {
                    // Stream ended naturally
                    break;
                }
                Err(_) => {
                    log::error!(
                        "SSE stream idle timeout: no data received for {}s",
                        STREAM_IDLE_TIMEOUT.as_secs()
                    );
                    anyhow::bail!(
                        "Stream idle timeout: no data received for {} seconds",
                        STREAM_IDLE_TIMEOUT.as_secs()
                    );
                }
            };

            // Check for cancellation FIRST (before processing chunk)
            if cancel_token.is_cancelled() {
                log::info!("SSE stream cancelled by user after {} chunks", chunk_count);
                anyhow::bail!("Query cancelled by user");
            }

            match chunk_result {
                Ok(chunk) => {
                    chunk_count += 1;
                    let text = String::from_utf8_lossy(&chunk);
                    log::debug!(
                        "SSE chunk #{} received ({} bytes) after {}ms",
                        chunk_count,
                        chunk.len(),
                        stream_start.elapsed().as_millis()
                    );
                    buffer.push_str(&text);

                    // Process complete lines from buffer
                    while let Some(newline_pos) = buffer.find('\n') {
                        let line = buffer[..newline_pos].trim_end().to_string();
                        buffer = buffer[newline_pos + 1..].to_string();

                        if line.is_empty() {
                            continue;
                        }

                        // Parse SSE line
                        if let Some(event_type) = line.strip_prefix("event: ") {
                            current_event = Some(event_type.trim().to_string());
                            log::debug!("SSE event type: {}", event_type);
                        } else if let Some(data) = line.strip_prefix("data: ")
                            && let Some(ref event) = current_event
                        {
                            match self.handle_sse_event(event, data, &mut result) {
                                Ok(Some(interrupt)) => {
                                    interrupt_data = Some(interrupt);
                                    // Don't break - continue processing to get any remaining messages
                                }
                                Ok(None) => {}
                                Err(e) => {
                                    log::error!("Error handling SSE event '{}': {}", event, e);
                                    return Err(e);
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "SSE stream error after {} chunks ({}ms): {}",
                        chunk_count,
                        stream_start.elapsed().as_millis(),
                        e
                    );
                    return Err(e.into());
                }
            }
        }

        // Process any remaining data in buffer
        for line in buffer.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            if let Some(data) = line.strip_prefix("data: ")
                && let Some(ref event) = current_event
                && let Some(interrupt) = self.handle_sse_event(event, data, &mut result)?
            {
                interrupt_data = Some(interrupt);
            }
        }

        log::info!(
            "SSE stream completed: {} chunks, {} result chars, {}ms elapsed",
            chunk_count,
            result.len(),
            stream_start.elapsed().as_millis()
        );
        if result.is_empty() {
            log::warn!("SSE stream returned EMPTY result - no AI content extracted");
        } else {
            log::debug!("Final result preview: {}...", truncate_utf8(&result, 200));
        }

        // Return interrupt if detected, otherwise complete
        match interrupt_data {
            Some(InterruptData::CommandApproval { command, message }) => {
                log::info!("Stream interrupted for command approval: {}", command);
                Ok(StreamResult::CommandApproval { command, message })
            }
            Some(InterruptData::Question { question, options }) => {
                log::info!("Stream interrupted with question: {}", question);
                Ok(StreamResult::Question { question, options })
            }
            None => Ok(StreamResult::Complete(result)),
        }
    }

    // ========== SSE Event Parsing Helpers ==========

    /// Check if a JSON message is from the AI (type="ai" OR role="assistant")
    fn is_ai_message(msg: &serde_json::Value) -> bool {
        msg.get("type").and_then(|v| v.as_str()) == Some("ai")
            || msg.get("role").and_then(|v| v.as_str()) == Some("assistant")
    }

    /// Extract text content from a message, handling both string and array formats
    fn extract_message_content(msg: &serde_json::Value) -> Option<String> {
        // Try string content first
        if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
            return if content.is_empty() {
                None
            } else {
                Some(content.to_string())
            };
        }

        // Try array of content blocks
        let content_array = msg.get("content").and_then(|v| v.as_array())?;
        if content_array.is_empty() {
            return None;
        }

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

        if text_parts.is_empty() {
            None
        } else {
            Some(text_parts.join("\n"))
        }
    }

    /// Check if content is valid (not empty, not a handoff message)
    fn is_valid_ai_content(content: &str) -> bool {
        !content.is_empty()
            && !content.starts_with("Transferring")
            && !content.starts_with("Successfully transferred")
    }

    /// Parse an interrupt value to determine if it's CommandApproval or Question
    fn parse_interrupt_value(value: &serde_json::Value) -> InterruptData {
        if value.get("command").is_some() {
            // CommandApproval: has "command" field
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
            InterruptData::CommandApproval { command, message }
        } else {
            // Question: has "question" field or only "message"
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
            InterruptData::Question { question, options }
        }
    }

    // ========== SSE Event Handlers ==========

    /// Handle "metadata" SSE event - log run_id for debugging
    fn handle_metadata_event(data: &str) {
        if let Ok(meta) = serde_json::from_str::<serde_json::Value>(data)
            && let Some(run_id) = meta.get("run_id").and_then(|v| v.as_str())
        {
            log::info!("Run started: {}", run_id);
        }
    }

    /// Handle "messages" SSE event - extract and accumulate AI message content
    fn handle_messages_event(data: &str, result: &mut String) {
        let Ok(messages) = serde_json::from_str::<serde_json::Value>(data) else {
            log::warn!("Failed to parse 'messages' event JSON");
            return;
        };
        let Some(msgs) = messages.as_array() else {
            log::debug!("'messages' event is not an array");
            return;
        };
        log::debug!("Processing {} items in 'messages' event", msgs.len());

        for msg in msgs {
            if Self::is_ai_message(msg)
                && let Some(content) = msg.get("content").and_then(|v| v.as_str())
            {
                if !result.is_empty() {
                    result.push('\n');
                }
                result.push_str(content);
            }
        }
    }

    /// Handle "updates" SSE event - check for human-in-the-loop interrupts
    fn handle_updates_event(data: &str) -> Result<Option<InterruptData>> {
        let Ok(updates) = serde_json::from_str::<serde_json::Value>(data) else {
            return Ok(None);
        };
        let Some(interrupts) = updates.get("__interrupt__").and_then(|v| v.as_array()) else {
            return Ok(None);
        };

        for interrupt in interrupts {
            if let Some(value) = interrupt.get("value") {
                let interrupt_data = Self::parse_interrupt_value(value);
                match &interrupt_data {
                    InterruptData::CommandApproval { command, .. } => {
                        log::info!(
                            "Command approval requested for: {} - awaiting user decision",
                            command
                        );
                    }
                    InterruptData::Question { question, .. } => {
                        log::info!("Question received: {} - awaiting user answer", question);
                    }
                }
                return Ok(Some(interrupt_data));
            }
        }
        Ok(None)
    }

    /// Handle "values" SSE event - extract latest AI message from state update
    fn handle_values_event(data: &str, result: &mut String) {
        let Ok(values) = serde_json::from_str::<serde_json::Value>(data) else {
            log::warn!(
                "Failed to parse 'values' event JSON: {}",
                truncate_utf8(data, 200)
            );
            return;
        };
        let Some(msgs) = values.get("messages").and_then(|v| v.as_array()) else {
            log::debug!(
                "No 'messages' array in values event, keys: {:?}",
                values.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            return;
        };
        log::debug!("Processing {} messages from 'values' event", msgs.len());

        // Get the last AI message with actual content from the values
        // Skip messages with empty content or just handoff messages
        for msg in msgs.iter().rev() {
            if !Self::is_ai_message(msg) {
                continue;
            }

            // Skip handoff messages (they just transfer control)
            let is_handoff = msg
                .get("response_metadata")
                .and_then(|m| m.get("__is_handoff_back"))
                .is_some();
            if is_handoff {
                continue;
            }

            if let Some(content) = Self::extract_message_content(msg)
                && Self::is_valid_ai_content(&content)
            {
                log::info!(
                    "Found AI message content ({} chars): {}...",
                    content.len(),
                    truncate_utf8(&content, 100)
                );
                result.clear(); // Replace with latest AI message
                result.push_str(&content);
                break; // Found a good message, stop searching
            } else if let Some(content) = Self::extract_message_content(msg) {
                log::debug!(
                    "Skipping invalid AI content: {}...",
                    truncate_utf8(&content, 50)
                );
            } else {
                log::debug!("No content extracted from AI message");
            }
        }
    }

    /// Handle "error" SSE event - signal fatal stream error
    fn handle_error_event(data: &str) -> Result<Option<InterruptData>> {
        if let Ok(error) = serde_json::from_str::<serde_json::Value>(data) {
            let msg = error
                .get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("Unknown error");
            log::error!("Stream error: {}", msg);
            anyhow::bail!("Stream error: {}", msg);
        }
        Ok(None)
    }

    // ========== Main SSE Event Dispatcher ==========

    /// Handle a single SSE event - returns interrupt data instead of marker
    /// Returns Some(InterruptData) if an interrupt is detected, None otherwise
    fn handle_sse_event(
        &self,
        event: &str,
        data: &str,
        result: &mut String,
    ) -> Result<Option<InterruptData>> {
        match event {
            "metadata" => Self::handle_metadata_event(data),
            "messages" => Self::handle_messages_event(data, result),
            "updates" => return Self::handle_updates_event(data),
            "values" => Self::handle_values_event(data, result),
            "error" => return Self::handle_error_event(data),
            "end" => log::debug!("Stream ended"),
            _ => log::trace!("Unknown SSE event type: {}", event),
        }
        Ok(None)
    }
}

#[async_trait]
impl LLMClientTrait for HttpLLMClient {
    async fn query(&self, text: &str) -> Result<LLMQueryResult> {
        log::info!("LLM query: {}", text);

        let thread_id = self.ensure_thread().await?;
        let stream_result = self
            .stream_run(&thread_id, Some(text), false, CancellationToken::new())
            .await?;
        Self::convert_stream_result(stream_result)
    }

    async fn query_cancellable(
        &self,
        text: &str,
        cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        log::info!("LLM query (cancellable): {}", text);

        // Use streaming endpoint via threads
        let thread_id = self.ensure_thread().await?;

        let stream_result = self
            .stream_run(&thread_id, Some(text), false, cancel_token)
            .await?;

        // Convert internal StreamResult to public LLMQueryResult
        Self::convert_stream_result(stream_result)
    }

    async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult> {
        let thread_id = self
            .thread_id
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active thread to resume"))?;

        let url = format!("{}/threads/{}/runs/stream", self.base_url, thread_id);
        log::info!("[HTTP-OUT] POST {} | answer_len={}", url, answer.len());

        // Send user's answer as a new message along with resume command
        let request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: Some(StreamInput {
                messages: vec![ChatMessage {
                    role: "user".into(),
                    content: answer.into(),
                }],
            }),
            command: Some(StreamCommand {
                resume: "approved".into(),
            }),
        };

        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let elapsed = request_start.elapsed();
        log::info!(
            "[HTTP-IN] POST /runs/stream | status={} | elapsed={}ms",
            response.status(),
            elapsed.as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Resume with answer failed ({}): {}", status, error_text);
            anyhow::bail!("Resume with answer failed ({}): {}", status, error_text);
        }

        let stream_result = self
            .parse_sse_stream(response, CancellationToken::new())
            .await?;
        Self::convert_stream_result(stream_result)
    }

    async fn resume_with_command_output(
        &self,
        command: &str,
        output: &str,
    ) -> Result<LLMQueryResult> {
        let thread_id = self
            .thread_id
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active thread to resume"))?;

        let url = format!("{}/threads/{}/runs/stream", self.base_url, thread_id);
        log::info!(
            "[HTTP-OUT] POST {} | command_output | cmd_len={} | output_len={}",
            url,
            command.len(),
            output.len()
        );

        // Send command and output as two messages with resume type "command_output"
        let request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: Some(StreamInput {
                messages: vec![
                    ChatMessage {
                        role: "user".into(),
                        content: command.into(),
                    },
                    ChatMessage {
                        role: "user".into(),
                        content: output.into(),
                    },
                ],
            }),
            command: Some(StreamCommand {
                resume: "command_output".into(),
            }),
        };

        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let elapsed = request_start.elapsed();
        log::info!(
            "[HTTP-IN] POST /runs/stream (command_output) | status={} | elapsed={}ms",
            response.status(),
            elapsed.as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!(
                "Resume with command output failed ({}): {}",
                status,
                error_text
            );
            anyhow::bail!(
                "Resume with command output failed ({}): {}",
                status,
                error_text
            );
        }

        let stream_result = self
            .parse_sse_stream(response, CancellationToken::new())
            .await?;
        Self::convert_stream_result(stream_result)
    }

    async fn resume_rejected(&self) -> Result<LLMQueryResult> {
        let thread_id = self
            .thread_id
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active thread to resume"))?;

        let url = format!("{}/threads/{}/runs/stream", self.base_url, thread_id);
        log::info!("[HTTP-OUT] POST {} | rejected", url);

        let request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".into(), "updates".into(), "messages".into()],
            input: None,
            command: Some(StreamCommand {
                resume: "rejected".into(),
            }),
        };

        let request_start = std::time::Instant::now();
        let response = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        let elapsed = request_start.elapsed();
        log::info!(
            "[HTTP-IN] POST /runs/stream (rejected) | status={} | elapsed={}ms",
            response.status(),
            elapsed.as_millis()
        );

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Resume rejected failed ({}): {}", status, error_text);
            anyhow::bail!("Resume rejected failed ({}): {}", status, error_text);
        }

        let stream_result = self
            .parse_sse_stream(response, CancellationToken::new())
            .await?;
        Self::convert_stream_result(stream_result)
    }
}

impl HttpLLMClient {
    /// Convert internal StreamResult to public LLMQueryResult
    fn convert_stream_result(stream_result: StreamResult) -> Result<LLMQueryResult> {
        match stream_result {
            StreamResult::Complete(response) => Ok(LLMQueryResult::Complete(response)),
            StreamResult::CommandApproval { command, message } => {
                Ok(LLMQueryResult::CommandApproval { command, message })
            }
            StreamResult::Question { question, options } => {
                Ok(LLMQueryResult::Question { question, options })
            }
        }
    }
}

/// Mock LLM client for testing and development
#[derive(Debug, Default)]
pub struct MockLLMClient;

impl MockLLMClient {
    pub const fn new() -> Self {
        Self
    }
}

#[async_trait]
impl LLMClientTrait for MockLLMClient {
    async fn query(&self, text: &str) -> Result<LLMQueryResult> {
        // Simple mock responses for testing
        let response = match text.to_lowercase().as_str() {
            s if s.contains("list files") => {
                "To list files, you can use the `ls` command. Some common options:\n\n".to_string()
                    + "- `ls -l` - Long format with details\n"
                    + "- `ls -a` - Show hidden files\n"
                    + "- `ls -lh` - Human-readable file sizes"
            }
            s if s.contains("docker") => {
                "Docker is a containerization platform. Some common commands:\n\n".to_string()
                    + "```bash\n"
                    + "docker ps          # List running containers\n"
                    + "docker images      # List images\n"
                    + "docker run <image> # Run a container\n"
                    + "```"
            }
            s if s.contains("kubernetes") || s.contains("k8s") => {
                "Kubernetes is a container orchestration platform. Common commands:\n\n".to_string()
                    + "```bash\n"
                    + "kubectl get pods              # List pods\n"
                    + "kubectl get services          # List services\n"
                    + "kubectl describe pod <name>   # Get pod details\n"
                    + "```"
            }
            _ => {
                "I'm a mock LLM. In production, I would provide detailed answers ".to_string()
                    + "about DevOps, cloud platforms, and terminal commands."
            }
        };

        Ok(LLMQueryResult::Complete(response))
    }

    async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult> {
        // Mock acknowledges the answer and returns complete
        Ok(LLMQueryResult::Complete(format!(
            "Mock received answer: '{}' - Processing complete.",
            answer
        )))
    }

    async fn resume_with_command_output(
        &self,
        command: &str,
        output: &str,
    ) -> Result<LLMQueryResult> {
        // Mock acknowledges the command output and returns complete
        Ok(LLMQueryResult::Complete(format!(
            "Mock received command output.\nCommand: {}\nOutput ({} chars): {}...",
            command,
            output.len(),
            truncate_utf8(output, 100)
        )))
    }

    async fn resume_rejected(&self) -> Result<LLMQueryResult> {
        // Mock acknowledges the rejection and returns complete
        Ok(LLMQueryResult::Complete(
            "Mock: Command was rejected by user. Task cancelled.".to_string(),
        ))
    }

    async fn query_cancellable(
        &self,
        text: &str,
        cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        // Simulate network delay with cancellation checks
        // 10 chunks × 100ms = 1s total simulated delay
        // Small chunks allow responsive cancellation (checked every 100ms)
        const MOCK_CHUNK_DELAY_MS: u64 = 100;
        const MOCK_CHUNK_COUNT: u64 = 10;

        for i in 0..MOCK_CHUNK_COUNT {
            if cancel_token.is_cancelled() {
                log::info!(
                    "Mock LLM query cancelled after {}ms",
                    i * MOCK_CHUNK_DELAY_MS
                );
                anyhow::bail!("Query cancelled by user");
            }
            tokio::time::sleep(std::time::Duration::from_millis(MOCK_CHUNK_DELAY_MS)).await;
        }

        // Return mock response (same as non-cancellable version)
        self.query(text).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === LLMQueryResult Tests ===

    #[test]
    fn test_llm_query_result_complete() {
        let result = LLMQueryResult::Complete("test response".to_string());
        assert_eq!(result.as_complete(), Some("test response"));
    }

    #[test]
    fn test_llm_query_result_unwrap_complete() {
        let result = LLMQueryResult::Complete("test response".to_string());
        assert_eq!(result.unwrap_complete(), "test response");
    }

    #[test]
    fn test_llm_query_result_command_approval_as_complete_none() {
        let result = LLMQueryResult::CommandApproval {
            command: "ls".to_string(),
            message: "List files?".to_string(),
        };
        assert_eq!(result.as_complete(), None);
    }

    #[test]
    fn test_llm_query_result_question_as_complete_none() {
        let result = LLMQueryResult::Question {
            question: "Which option?".to_string(),
            options: Some(vec!["A".to_string(), "B".to_string()]),
        };
        assert_eq!(result.as_complete(), None);
    }

    #[test]
    #[should_panic(expected = "Expected Complete, got CommandApproval")]
    fn test_llm_query_result_unwrap_complete_panics_on_approval() {
        let result = LLMQueryResult::CommandApproval {
            command: "rm -rf".to_string(),
            message: "Delete?".to_string(),
        };
        let _ = result.unwrap_complete();
    }

    #[test]
    #[should_panic(expected = "Expected Complete, got Question")]
    fn test_llm_query_result_unwrap_complete_panics_on_question() {
        let result = LLMQueryResult::Question {
            question: "What?".to_string(),
            options: None,
        };
        let _ = result.unwrap_complete();
    }

    #[test]
    fn test_llm_query_result_debug() {
        let result = LLMQueryResult::Complete("test".to_string());
        let debug_str = format!("{:?}", result);
        assert!(debug_str.contains("Complete"));
    }

    // === MockLLMClient Tests ===

    #[test]
    fn test_mock_client_creation() {
        let client = MockLLMClient::new();
        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("MockLLMClient"));
    }

    #[test]
    fn test_mock_client_default() {
        let client = MockLLMClient;
        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("MockLLMClient"));
    }

    #[tokio::test]
    async fn test_mock_client_query_list_files() {
        let client = MockLLMClient::new();
        let result = client.query("how do I list files").await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("ls"));
        assert!(response.contains("-l"));
    }

    #[tokio::test]
    async fn test_mock_client_query_docker() {
        let client = MockLLMClient::new();
        let result = client.query("tell me about docker").await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("docker"));
        assert!(response.contains("container"));
    }

    #[tokio::test]
    async fn test_mock_client_query_kubernetes() {
        let client = MockLLMClient::new();
        let result = client.query("how to use k8s").await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("kubectl"));
    }

    #[tokio::test]
    async fn test_mock_client_query_unknown() {
        let client = MockLLMClient::new();
        let result = client.query("random gibberish xyz123").await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("mock LLM"));
    }

    #[tokio::test]
    async fn test_mock_client_resume_with_answer() {
        let client = MockLLMClient::new();
        let result = client.resume_with_answer("my answer").await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("my answer"));
    }

    #[tokio::test]
    async fn test_mock_client_query_cancellable_completes() {
        let client = MockLLMClient::new();
        let cancel_token = CancellationToken::new();

        // Should complete without cancellation
        let result = client.query_cancellable("list files", cancel_token).await;
        assert!(result.is_ok());

        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("ls"));
    }

    #[tokio::test]
    async fn test_mock_client_query_cancellable_cancelled() {
        let client = MockLLMClient::new();
        let cancel_token = CancellationToken::new();

        // Cancel immediately
        cancel_token.cancel();

        let result = client.query_cancellable("list files", cancel_token).await;
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(err.to_string().contains("cancelled"));
    }

    #[tokio::test]
    async fn test_mock_client_case_insensitive() {
        let client = MockLLMClient::new();

        // Test uppercase
        let result = client.query("LIST FILES").await;
        assert!(result.is_ok());
        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("ls"));

        // Test mixed case
        let result = client.query("Docker Commands").await;
        assert!(result.is_ok());
        let response = result.unwrap().unwrap_complete();
        assert!(response.contains("docker"));
    }

    // === HttpLLMClient Tests (basic, no network) ===

    #[test]
    fn test_http_client_debug() {
        let client = HttpLLMClient::new(
            "http://localhost:8000".to_string(),
            "test-api-key".to_string(),
        );
        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("HttpLLMClient"));
        assert!(debug_str.contains("localhost:8000"));
        // API key should NOT be in debug output (security)
        assert!(!debug_str.contains("test-api-key"));
    }

    // === truncate_utf8 ===

    #[test]
    fn test_truncate_utf8_within_limit() {
        assert_eq!(truncate_utf8("hello", 10), "hello");
    }

    #[test]
    fn test_truncate_utf8_exact_limit() {
        assert_eq!(truncate_utf8("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_cuts_ascii() {
        assert_eq!(truncate_utf8("hello world", 5), "hello");
    }

    #[test]
    fn test_truncate_utf8_respects_char_boundary() {
        // '€' is 3 bytes; truncating at byte 2 must back up to 0
        assert_eq!(truncate_utf8("€abc", 2), "");
        // truncating at byte 3 keeps the full '€'
        assert_eq!(truncate_utf8("€abc", 3), "€");
    }

    #[test]
    fn test_truncate_utf8_empty() {
        assert_eq!(truncate_utf8("", 10), "");
        assert_eq!(truncate_utf8("", 0), "");
    }

    // === is_ai_message ===

    #[test]
    fn test_is_ai_message_by_type() {
        let msg = serde_json::json!({"type": "ai", "content": "hi"});
        assert!(HttpLLMClient::is_ai_message(&msg));
    }

    #[test]
    fn test_is_ai_message_by_role() {
        let msg = serde_json::json!({"role": "assistant", "content": "hi"});
        assert!(HttpLLMClient::is_ai_message(&msg));
    }

    #[test]
    fn test_is_ai_message_user() {
        let msg = serde_json::json!({"type": "human", "role": "user"});
        assert!(!HttpLLMClient::is_ai_message(&msg));
    }

    #[test]
    fn test_is_ai_message_empty() {
        let msg = serde_json::json!({});
        assert!(!HttpLLMClient::is_ai_message(&msg));
    }

    // === extract_message_content ===

    #[test]
    fn test_extract_content_string() {
        let msg = serde_json::json!({"content": "hello world"});
        assert_eq!(
            HttpLLMClient::extract_message_content(&msg),
            Some("hello world".to_string())
        );
    }

    #[test]
    fn test_extract_content_empty_string() {
        let msg = serde_json::json!({"content": ""});
        assert_eq!(HttpLLMClient::extract_message_content(&msg), None);
    }

    #[test]
    fn test_extract_content_array_text_blocks() {
        let msg = serde_json::json!({
            "content": [
                {"type": "text", "text": "part1"},
                {"type": "text", "text": "part2"}
            ]
        });
        assert_eq!(
            HttpLLMClient::extract_message_content(&msg),
            Some("part1\npart2".to_string())
        );
    }

    #[test]
    fn test_extract_content_array_skips_non_text() {
        let msg = serde_json::json!({
            "content": [
                {"type": "tool_use", "id": "123"},
                {"type": "text", "text": "only this"}
            ]
        });
        assert_eq!(
            HttpLLMClient::extract_message_content(&msg),
            Some("only this".to_string())
        );
    }

    #[test]
    fn test_extract_content_empty_array() {
        let msg = serde_json::json!({"content": []});
        assert_eq!(HttpLLMClient::extract_message_content(&msg), None);
    }

    #[test]
    fn test_extract_content_no_content_field() {
        let msg = serde_json::json!({"role": "assistant"});
        assert_eq!(HttpLLMClient::extract_message_content(&msg), None);
    }

    // === is_valid_ai_content ===

    #[test]
    fn test_valid_ai_content() {
        assert!(HttpLLMClient::is_valid_ai_content("Here is the answer"));
    }

    #[test]
    fn test_invalid_ai_content_empty() {
        assert!(!HttpLLMClient::is_valid_ai_content(""));
    }

    #[test]
    fn test_invalid_ai_content_transferring() {
        assert!(!HttpLLMClient::is_valid_ai_content("Transferring to agent"));
        assert!(!HttpLLMClient::is_valid_ai_content(
            "Successfully transferred control"
        ));
    }

    // === parse_interrupt_value ===

    #[test]
    fn test_parse_interrupt_command_approval() {
        let value = serde_json::json!({
            "command": "ls -la",
            "message": "List directory contents"
        });
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::CommandApproval { command, message } => {
                assert_eq!(command, "ls -la");
                assert_eq!(message, "List directory contents");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_parse_interrupt_command_defaults() {
        let value = serde_json::json!({"command": null});
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::CommandApproval { command, message } => {
                assert_eq!(command, "unknown");
                assert_eq!(message, "Command requires approval");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_parse_interrupt_question() {
        let value = serde_json::json!({
            "question": "Which environment?",
            "options": ["dev", "staging", "prod"]
        });
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "Which environment?");
                assert_eq!(
                    options,
                    Some(vec![
                        "dev".to_string(),
                        "staging".to_string(),
                        "prod".to_string()
                    ])
                );
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_parse_interrupt_question_no_options() {
        let value = serde_json::json!({"question": "What name?"});
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "What name?");
                assert!(options.is_none());
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_parse_interrupt_fallback_message_as_question() {
        // No "command" and no "question" field, falls back to "message"
        let value = serde_json::json!({"message": "Please provide input"});
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::Question { question, .. } => {
                assert_eq!(question, "Please provide input");
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_parse_interrupt_empty_object() {
        let value = serde_json::json!({});
        let result = HttpLLMClient::parse_interrupt_value(&value);
        match result {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "Agent is asking for input");
                assert!(options.is_none());
            }
            _ => panic!("Expected Question"),
        }
    }

    // === handle_messages_event ===

    #[test]
    fn test_handle_messages_event_ai_messages() {
        let data = serde_json::json!([
            {"type": "ai", "content": "Hello"},
            {"type": "human", "content": "ignored"},
            {"type": "ai", "content": "World"}
        ])
        .to_string();
        let mut result = String::new();
        HttpLLMClient::handle_messages_event(&data, &mut result);
        assert_eq!(result, "Hello\nWorld");
    }

    #[test]
    fn test_handle_messages_event_no_ai() {
        let data = serde_json::json!([
            {"type": "human", "content": "user msg"}
        ])
        .to_string();
        let mut result = String::new();
        HttpLLMClient::handle_messages_event(&data, &mut result);
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_messages_event_invalid_json() {
        let mut result = String::new();
        HttpLLMClient::handle_messages_event("not json", &mut result);
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_messages_event_not_array() {
        let mut result = String::new();
        HttpLLMClient::handle_messages_event("{\"key\": \"value\"}", &mut result);
        assert!(result.is_empty());
    }

    // === handle_updates_event ===

    #[test]
    fn test_handle_updates_event_command_interrupt() {
        let data = serde_json::json!({
            "__interrupt__": [{
                "value": {
                    "command": "rm -rf /tmp/test",
                    "message": "Delete temp files"
                }
            }]
        })
        .to_string();
        let result = HttpLLMClient::handle_updates_event(&data).unwrap();
        match result {
            Some(InterruptData::CommandApproval { command, message }) => {
                assert_eq!(command, "rm -rf /tmp/test");
                assert_eq!(message, "Delete temp files");
            }
            _ => panic!("Expected CommandApproval interrupt"),
        }
    }

    #[test]
    fn test_handle_updates_event_question_interrupt() {
        let data = serde_json::json!({
            "__interrupt__": [{
                "value": {
                    "question": "Which DB?",
                    "options": ["postgres", "mysql"]
                }
            }]
        })
        .to_string();
        let result = HttpLLMClient::handle_updates_event(&data).unwrap();
        assert!(matches!(result, Some(InterruptData::Question { .. })));
    }

    #[test]
    fn test_handle_updates_event_no_interrupt() {
        let data = serde_json::json!({"some_key": "some_value"}).to_string();
        let result = HttpLLMClient::handle_updates_event(&data).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_handle_updates_event_invalid_json() {
        let result = HttpLLMClient::handle_updates_event("bad json").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_handle_updates_event_interrupt_no_value() {
        let data = serde_json::json!({
            "__interrupt__": [{"no_value_field": true}]
        })
        .to_string();
        let result = HttpLLMClient::handle_updates_event(&data).unwrap();
        assert!(result.is_none());
    }

    // === handle_values_event ===

    #[test]
    fn test_handle_values_event_extracts_last_ai() {
        let data = serde_json::json!({
            "messages": [
                {"type": "human", "content": "hello"},
                {"type": "ai", "content": "first answer"},
                {"type": "ai", "content": "latest answer"}
            ]
        })
        .to_string();
        let mut result = String::new();
        HttpLLMClient::handle_values_event(&data, &mut result);
        assert_eq!(result, "latest answer");
    }

    #[test]
    fn test_handle_values_event_skips_handoff() {
        let data = serde_json::json!({
            "messages": [
                {"type": "ai", "content": "real answer"},
                {"type": "ai", "content": "Transferring to X", "response_metadata": {"__is_handoff_back": true}}
            ]
        })
        .to_string();
        let mut result = String::new();
        HttpLLMClient::handle_values_event(&data, &mut result);
        assert_eq!(result, "real answer");
    }

    #[test]
    fn test_handle_values_event_skips_transfer_content() {
        let data = serde_json::json!({
            "messages": [
                {"type": "ai", "content": "good content"},
                {"type": "ai", "content": "Transferring control to agent"}
            ]
        })
        .to_string();
        let mut result = String::new();
        HttpLLMClient::handle_values_event(&data, &mut result);
        assert_eq!(result, "good content");
    }

    #[test]
    fn test_handle_values_event_no_messages() {
        let data = serde_json::json!({"other": "data"}).to_string();
        let mut result = String::new();
        HttpLLMClient::handle_values_event(&data, &mut result);
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_values_event_replaces_previous_result() {
        let data = serde_json::json!({
            "messages": [{"type": "ai", "content": "new content"}]
        })
        .to_string();
        let mut result = "old content".to_string();
        HttpLLMClient::handle_values_event(&data, &mut result);
        assert_eq!(result, "new content");
    }

    #[test]
    fn test_handle_values_event_invalid_json() {
        let mut result = "unchanged".to_string();
        HttpLLMClient::handle_values_event("not json", &mut result);
        assert_eq!(result, "unchanged");
    }

    // === handle_error_event ===

    #[test]
    fn test_handle_error_event_with_message() {
        let data = serde_json::json!({"message": "rate limit exceeded"}).to_string();
        let err = HttpLLMClient::handle_error_event(&data).unwrap_err();
        assert!(err.to_string().contains("rate limit exceeded"));
    }

    #[test]
    fn test_handle_error_event_no_message() {
        let data = serde_json::json!({}).to_string();
        let err = HttpLLMClient::handle_error_event(&data).unwrap_err();
        assert!(err.to_string().contains("Unknown error"));
    }

    #[test]
    fn test_handle_error_event_invalid_json() {
        let result = HttpLLMClient::handle_error_event("not json").unwrap();
        assert!(result.is_none());
    }

    // === handle_sse_event (dispatcher) ===

    #[test]
    fn test_handle_sse_event_messages() {
        let client = HttpLLMClient::new("http://test".into(), "key".into());
        let data = serde_json::json!([{"type": "ai", "content": "test"}]).to_string();
        let mut result = String::new();
        let interrupt = client
            .handle_sse_event("messages", &data, &mut result)
            .unwrap();
        assert!(interrupt.is_none());
        assert_eq!(result, "test");
    }

    #[test]
    fn test_handle_sse_event_updates_with_interrupt() {
        let client = HttpLLMClient::new("http://test".into(), "key".into());
        let data = serde_json::json!({
            "__interrupt__": [{"value": {"command": "ls", "message": "list"}}]
        })
        .to_string();
        let mut result = String::new();
        let interrupt = client
            .handle_sse_event("updates", &data, &mut result)
            .unwrap();
        assert!(matches!(
            interrupt,
            Some(InterruptData::CommandApproval { .. })
        ));
    }

    #[test]
    fn test_handle_sse_event_error() {
        let client = HttpLLMClient::new("http://test".into(), "key".into());
        let data = serde_json::json!({"message": "boom"}).to_string();
        let mut result = String::new();
        let err = client.handle_sse_event("error", &data, &mut result);
        assert!(err.is_err());
    }

    #[test]
    fn test_handle_sse_event_end() {
        let client = HttpLLMClient::new("http://test".into(), "key".into());
        let mut result = String::new();
        let interrupt = client.handle_sse_event("end", "", &mut result).unwrap();
        assert!(interrupt.is_none());
    }

    #[test]
    fn test_handle_sse_event_unknown() {
        let client = HttpLLMClient::new("http://test".into(), "key".into());
        let mut result = String::new();
        let interrupt = client
            .handle_sse_event("custom_event", "data", &mut result)
            .unwrap();
        assert!(interrupt.is_none());
    }

    // === convert_stream_result ===

    #[test]
    fn test_convert_stream_result_complete() {
        let result =
            HttpLLMClient::convert_stream_result(StreamResult::Complete("done".into())).unwrap();
        assert_eq!(result.as_complete(), Some("done"));
    }

    #[test]
    fn test_convert_stream_result_command_approval() {
        let result = HttpLLMClient::convert_stream_result(StreamResult::CommandApproval {
            command: "ls".into(),
            message: "list".into(),
        })
        .unwrap();
        match result {
            LLMQueryResult::CommandApproval { command, message } => {
                assert_eq!(command, "ls");
                assert_eq!(message, "list");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_convert_stream_result_question() {
        let result = HttpLLMClient::convert_stream_result(StreamResult::Question {
            question: "which?".into(),
            options: Some(vec!["a".into(), "b".into()]),
        })
        .unwrap();
        match result {
            LLMQueryResult::Question { question, options } => {
                assert_eq!(question, "which?");
                assert_eq!(options.unwrap().len(), 2);
            }
            _ => panic!("Expected Question"),
        }
    }

    // === MockLLMClient resume_with_command_output / resume_rejected ===

    #[tokio::test]
    async fn test_mock_client_resume_with_command_output() {
        let client = MockLLMClient::new();
        let result = client
            .resume_with_command_output("uname -s", "Linux")
            .await
            .unwrap();
        let text = result.unwrap_complete();
        assert!(text.contains("uname -s"));
        assert!(text.contains("Linux"));
    }

    #[tokio::test]
    async fn test_mock_client_resume_rejected() {
        let client = MockLLMClient::new();
        let result = client.resume_rejected().await.unwrap();
        let text = result.unwrap_complete();
        assert!(text.contains("rejected"));
    }
}
