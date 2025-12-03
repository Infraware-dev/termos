/// LLM client for natural language queries
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// Request to the LLM backend (legacy - kept for backward compatibility)
#[derive(Debug, Serialize)]
#[allow(dead_code)] // Legacy API for M2/M3
pub struct LLMRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Response from the LLM backend
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Legacy API for M2/M3
pub struct LLMResponse {
    pub text: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
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
    /// Query the LLM with natural language input
    /// Returns LLMQueryResult which can be Complete or Interrupted (for HITL)
    async fn query(&self, text: &str) -> Result<LLMQueryResult>;

    /// Query with additional context
    async fn query_with_context(
        &self,
        text: &str,
        _context: Option<String>,
    ) -> Result<LLMQueryResult> {
        // Default implementation ignores context
        self.query(text).await
    }

    /// Resume an interrupted run after user approval (for command approval)
    async fn resume_run(&self) -> Result<LLMQueryResult>;

    /// Resume an interrupted run with a text answer (for questions)
    async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult>;

    /// Query with command history context (M2/M3)
    async fn query_with_history(
        &self,
        text: &str,
        command_history: &[String],
    ) -> Result<LLMQueryResult> {
        let context = if command_history.is_empty() {
            None
        } else {
            Some(format!("Recent commands:\n{}", command_history.join("\n")))
        };

        self.query_with_context(text, context).await
    }

    /// Query with cancellation support (default: no cancellation)
    async fn query_cancellable(
        &self,
        text: &str,
        _cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        // Default: ignore cancellation token
        self.query(text).await
    }

    /// Resume with cancellation support
    async fn resume_run_cancellable(
        &self,
        _cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        self.resume_run().await
    }

    /// Resume with answer and cancellation support
    async fn resume_with_answer_cancellable(
        &self,
        answer: &str,
        _cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        self.resume_with_answer(answer).await
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
                .timeout(std::time::Duration::from_secs(60)) // Overall request timeout (reduced from 120s)
                .local_address(std::net::IpAddr::V4(std::net::Ipv4Addr::new(127, 0, 0, 1))) // Force IPv4
                .pool_max_idle_per_host(0) // Disable connection pooling for SSE
                .build()
                .unwrap_or_default(),
            api_key,
            thread_id: RwLock::new(None),
        }
    }

    /// Create a new HTTP LLM client with custom timeout
    #[allow(dead_code)] // Constructor with custom timeout for testing
    pub fn with_timeout(base_url: String, api_key: String, timeout_secs: u64) -> Result<Self> {
        Ok(Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build()?,
            api_key,
            thread_id: RwLock::new(None),
        })
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

        while let Some(chunk_result) = stream.next().await {
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
                            log::trace!("SSE event type: {}", event_type);
                        } else if let Some(data) = line.strip_prefix("data: ") {
                            if let Some(ref event) = current_event {
                                match self.handle_sse_event_v2(event, data, &mut result) {
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
            if let Some(data) = line.strip_prefix("data: ") {
                if let Some(ref event) = current_event {
                    if let Some(interrupt) = self.handle_sse_event_v2(event, data, &mut result)? {
                        interrupt_data = Some(interrupt);
                    }
                }
            }
        }

        log::debug!(
            "SSE stream completed: {} chunks, {} chars, {}ms elapsed",
            chunk_count,
            result.len(),
            stream_start.elapsed().as_millis()
        );

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

    /// Handle a single SSE event (legacy - used by tests)
    #[cfg(test)]
    fn handle_sse_event(&self, event: &str, data: &str, result: &mut String) -> Result<()> {
        match event {
            "metadata" => {
                // Log run_id for debugging
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(run_id) = meta.get("run_id").and_then(|v| v.as_str()) {
                        log::info!("Run started: {}", run_id);
                    }
                }
            }
            "messages" => {
                // Extract AI message content
                if let Ok(messages) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(msgs) = messages.as_array() {
                        for msg in msgs {
                            let is_ai = msg.get("type").and_then(|v| v.as_str()) == Some("ai")
                                || msg.get("role").and_then(|v| v.as_str()) == Some("assistant");
                            if is_ai {
                                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                                    if !result.is_empty() {
                                        result.push('\n');
                                    }
                                    result.push_str(content);
                                }
                            }
                        }
                    }
                }
            }
            "updates" => {
                // Check for interrupt (human-in-the-loop for command approval)
                if let Ok(updates) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(interrupts) =
                        updates.get("__interrupt__").and_then(|v| v.as_array())
                    {
                        for interrupt in interrupts {
                            if let Some(value) = interrupt.get("value") {
                                let interrupt_type =
                                    value.get("type").and_then(|v| v.as_str()).unwrap_or("");
                                if interrupt_type == "command_approval" {
                                    let command = value
                                        .get("command")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("unknown");
                                    log::info!(
                                        "Command approval requested for: {} - auto-approving",
                                        command
                                    );
                                    // Signal that we need to resume with approval
                                    // The calling code will handle the resume
                                    result.push_str("__INTERRUPT_RESUME__");
                                }
                            }
                        }
                    }
                }
            }
            "values" => {
                // State updates contain the full message history including AI responses
                if let Ok(values) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(msgs) = values.get("messages").and_then(|v| v.as_array()) {
                        // Get the last AI message with actual content from the values
                        // Skip messages with empty content or just handoff messages
                        for msg in msgs.iter().rev() {
                            let is_ai = msg.get("type").and_then(|v| v.as_str()) == Some("ai")
                                || msg.get("role").and_then(|v| v.as_str()) == Some("assistant");

                            // Skip handoff messages (they just transfer control)
                            let is_handoff = msg
                                .get("response_metadata")
                                .and_then(|m| m.get("__is_handoff_back"))
                                .is_some();

                            if is_ai && !is_handoff {
                                // Handle content as string or array of content blocks
                                let content_text = if let Some(content) =
                                    msg.get("content").and_then(|v| v.as_str())
                                {
                                    // Content is a simple string
                                    if !content.is_empty() {
                                        Some(content.to_string())
                                    } else {
                                        None
                                    }
                                } else if let Some(content_array) =
                                    msg.get("content").and_then(|v| v.as_array())
                                {
                                    // Content is an array of blocks (text, tool_use, etc.)
                                    // Skip if array is empty
                                    if content_array.is_empty() {
                                        None
                                    } else {
                                        let text_parts: Vec<String> = content_array
                                            .iter()
                                            .filter_map(|block| {
                                                if block.get("type").and_then(|v| v.as_str())
                                                    == Some("text")
                                                {
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
                                            Some(text_parts.join("\n"))
                                        } else {
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };

                                if let Some(content) = content_text {
                                    // Only update if we have new meaningful content
                                    if !content.is_empty()
                                        && !content.starts_with("Transferring")
                                        && !content.starts_with("Successfully transferred")
                                    {
                                        result.clear(); // Replace with latest AI message
                                        result.push_str(&content);
                                        break; // Found a good message, stop searching
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "error" => {
                if let Ok(error) = serde_json::from_str::<serde_json::Value>(data) {
                    let msg = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    log::error!("Stream error: {}", msg);
                    anyhow::bail!("Stream error: {}", msg);
                }
            }
            "end" => {
                log::debug!("Stream ended");
            }
            _ => {
                log::trace!("Unknown SSE event type: {}", event);
            }
        }
        Ok(())
    }

    /// Handle a single SSE event (v2 - returns interrupt data instead of marker)
    /// Returns Some(InterruptData) if an interrupt is detected, None otherwise
    fn handle_sse_event_v2(
        &self,
        event: &str,
        data: &str,
        result: &mut String,
    ) -> Result<Option<InterruptData>> {
        match event {
            "metadata" => {
                // Log run_id for debugging
                if let Ok(meta) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(run_id) = meta.get("run_id").and_then(|v| v.as_str()) {
                        log::info!("Run started: {}", run_id);
                    }
                }
            }
            "messages" => {
                // Extract AI message content
                if let Ok(messages) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(msgs) = messages.as_array() {
                        for msg in msgs {
                            let is_ai = msg.get("type").and_then(|v| v.as_str()) == Some("ai")
                                || msg.get("role").and_then(|v| v.as_str()) == Some("assistant");
                            if is_ai {
                                if let Some(content) = msg.get("content").and_then(|v| v.as_str()) {
                                    if !result.is_empty() {
                                        result.push('\n');
                                    }
                                    result.push_str(content);
                                }
                            }
                        }
                    }
                }
            }
            "updates" => {
                // Check for interrupt (human-in-the-loop)
                if let Ok(updates) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(interrupts) =
                        updates.get("__interrupt__").and_then(|v| v.as_array())
                    {
                        for interrupt in interrupts {
                            if let Some(value) = interrupt.get("value") {
                                // Detect interrupt type by field presence (compatible with Python backend)
                                // Backend sends: {"command": "...", "message": "..."} for approvals
                                // or {"question": "...", "options": [...]} for questions
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
                                    log::info!(
                                        "Command approval requested for: {} - awaiting user decision",
                                        command
                                    );
                                    return Ok(Some(InterruptData::CommandApproval {
                                        command,
                                        message,
                                    }));
                                } else {
                                    // Question: has "question" field or only "message"
                                    let question = value
                                        .get("question")
                                        .or_else(|| value.get("message"))
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("Agent is asking for input")
                                        .to_string();
                                    let options = value
                                        .get("options")
                                        .and_then(|v| v.as_array())
                                        .map(|arr| {
                                            arr.iter()
                                                .filter_map(|v| v.as_str().map(String::from))
                                                .collect()
                                        });
                                    log::info!(
                                        "Question received: {} - awaiting user answer",
                                        question
                                    );
                                    return Ok(Some(InterruptData::Question { question, options }));
                                }
                            }
                        }
                    }
                }
            }
            "values" => {
                // State updates contain the full message history including AI responses
                if let Ok(values) = serde_json::from_str::<serde_json::Value>(data) {
                    if let Some(msgs) = values.get("messages").and_then(|v| v.as_array()) {
                        // Get the last AI message with actual content from the values
                        // Skip messages with empty content or just handoff messages
                        for msg in msgs.iter().rev() {
                            let is_ai = msg.get("type").and_then(|v| v.as_str()) == Some("ai")
                                || msg.get("role").and_then(|v| v.as_str()) == Some("assistant");

                            // Skip handoff messages (they just transfer control)
                            let is_handoff = msg
                                .get("response_metadata")
                                .and_then(|m| m.get("__is_handoff_back"))
                                .is_some();

                            if is_ai && !is_handoff {
                                // Handle content as string or array of content blocks
                                let content_text = if let Some(content) =
                                    msg.get("content").and_then(|v| v.as_str())
                                {
                                    // Content is a simple string
                                    if !content.is_empty() {
                                        Some(content.to_string())
                                    } else {
                                        None
                                    }
                                } else if let Some(content_array) =
                                    msg.get("content").and_then(|v| v.as_array())
                                {
                                    // Content is an array of blocks (text, tool_use, etc.)
                                    // Skip if array is empty
                                    if content_array.is_empty() {
                                        None
                                    } else {
                                        let text_parts: Vec<String> = content_array
                                            .iter()
                                            .filter_map(|block| {
                                                if block.get("type").and_then(|v| v.as_str())
                                                    == Some("text")
                                                {
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
                                            Some(text_parts.join("\n"))
                                        } else {
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };

                                if let Some(content) = content_text {
                                    // Only update if we have new meaningful content
                                    if !content.is_empty()
                                        && !content.starts_with("Transferring")
                                        && !content.starts_with("Successfully transferred")
                                    {
                                        result.clear(); // Replace with latest AI message
                                        result.push_str(&content);
                                        break; // Found a good message, stop searching
                                    }
                                }
                            }
                        }
                    }
                }
            }
            "error" => {
                if let Ok(error) = serde_json::from_str::<serde_json::Value>(data) {
                    let msg = error
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    log::error!("Stream error: {}", msg);
                    anyhow::bail!("Stream error: {}", msg);
                }
            }
            "end" => {
                log::debug!("Stream ended");
            }
            _ => {
                log::trace!("Unknown SSE event type: {}", event);
            }
        }
        Ok(None)
    }
}

#[async_trait]
impl LLMClientTrait for HttpLLMClient {
    async fn query(&self, text: &str) -> Result<LLMQueryResult> {
        self.query_with_context(text, None).await
    }

    async fn query_with_context(
        &self,
        text: &str,
        context: Option<String>,
    ) -> Result<LLMQueryResult> {
        log::info!("LLM query: {} (context: {:?})", text, context.is_some());

        // Use streaming endpoint via threads
        let thread_id = self.ensure_thread().await?;

        // Combine context with text if provided
        let full_query = match context {
            Some(ctx) => format!("{}\n\nContext:\n{}", text, ctx),
            None => text.to_string(),
        };

        // Create a default non-cancelled token for non-cancellable query
        let stream_result = self
            .stream_run(
                &thread_id,
                Some(&full_query),
                false,
                CancellationToken::new(),
            )
            .await?;

        // Convert internal StreamResult to public LLMQueryResult
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

    async fn resume_run(&self) -> Result<LLMQueryResult> {
        log::debug!("Resuming LLM run after user approval");

        let thread_id = self
            .thread_id
            .read()
            .await
            .clone()
            .ok_or_else(|| anyhow::anyhow!("No active thread to resume"))?;

        let stream_result = self
            .stream_run(&thread_id, None, true, CancellationToken::new())
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
                "To list files, you can use the `ls` command. Some common options:\n\n\
                 - `ls -l` - Long format with details\n\
                 - `ls -a` - Show hidden files\n\
                 - `ls -lh` - Human-readable file sizes"
            }
            s if s.contains("docker") => {
                "Docker is a containerization platform. Some common commands:\n\n\
                 ```bash\n\
                 docker ps          # List running containers\n\
                 docker images      # List images\n\
                 docker run <image> # Run a container\n\
                 ```"
            }
            s if s.contains("kubernetes") || s.contains("k8s") => {
                "Kubernetes is a container orchestration platform. Common commands:\n\n\
                 ```bash\n\
                 kubectl get pods              # List pods\n\
                 kubectl get services          # List services\n\
                 kubectl describe pod <name>   # Get pod details\n\
                 ```"
            }
            _ => {
                "I'm a mock LLM. In production, I would provide detailed answers \
                 about DevOps, cloud platforms, and terminal commands."
            }
        };

        Ok(LLMQueryResult::Complete(response.to_string()))
    }

    async fn resume_run(&self) -> Result<LLMQueryResult> {
        // Mock always returns complete (no real interrupt handling)
        Ok(LLMQueryResult::Complete(
            "Mock resume completed.".to_string(),
        ))
    }

    async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult> {
        // Mock acknowledges the answer and returns complete
        Ok(LLMQueryResult::Complete(format!(
            "Mock received answer: '{}' - Processing complete.",
            answer
        )))
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

    #[test]
    fn test_llm_client_new() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        assert_eq!(client.base_url, "http://localhost:8080");
        assert_eq!(client.api_key, "test-key");
    }

    #[test]
    fn test_llm_client_with_timeout() {
        let client = HttpLLMClient::with_timeout(
            "http://localhost:8080".to_string(),
            "test-key".to_string(),
            30,
        );
        assert!(client.is_ok());
        let client = client.unwrap();
        assert_eq!(client.base_url, "http://localhost:8080");
        assert_eq!(client.api_key, "test-key");
    }

    #[test]
    fn test_llm_request_serialization() {
        let request = LLMRequest {
            query: "test query".to_string(),
            context: Some("test context".to_string()),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test query"));
        assert!(json.contains("test context"));
    }

    #[test]
    fn test_llm_request_serialization_no_context() {
        let request = LLMRequest {
            query: "test query".to_string(),
            context: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test query"));
        // Context should be skipped when None
        assert!(!json.contains("context"));
    }

    #[test]
    fn test_llm_response_deserialization() {
        let json = r#"{"text":"response text","metadata":{"key":"value"}}"#;
        let response: LLMResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "response text");
        assert!(response.metadata.is_some());
    }

    #[test]
    fn test_llm_response_deserialization_no_metadata() {
        let json = r#"{"text":"response text"}"#;
        let response: LLMResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.text, "response text");
        assert!(response.metadata.is_none());
    }

    #[tokio::test]
    async fn test_mock_llm() {
        let mock = MockLLMClient;
        let response = mock
            .query("how to list files")
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("ls"));
    }

    #[tokio::test]
    async fn test_mock_llm_docker() {
        let mock = MockLLMClient;
        let response = mock
            .query("what is docker")
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("Docker"));
    }

    #[tokio::test]
    async fn test_mock_llm_kubernetes() {
        let mock = MockLLMClient;
        let response = mock
            .query("what is kubernetes")
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("Kubernetes"));
        assert!(response.contains("kubectl"));
    }

    #[tokio::test]
    async fn test_mock_llm_k8s() {
        let mock = MockLLMClient;
        let response = mock.query("help with k8s").await.unwrap().unwrap_complete();
        assert!(response.contains("Kubernetes"));
    }

    #[tokio::test]
    async fn test_mock_llm_fallback() {
        let mock = MockLLMClient;
        let response = mock
            .query("something random")
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("mock LLM"));
    }

    #[tokio::test]
    async fn test_mock_llm_case_insensitive() {
        let mock = MockLLMClient;
        let response = mock
            .query("DOCKER containers")
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("Docker"));
    }

    #[test]
    fn test_handle_sse_metadata_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"run_id":"test-run-123","attempt":1}"#;

        // Should not fail, just logs
        let outcome = client.handle_sse_event("metadata", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty()); // metadata doesn't add to result
    }

    #[test]
    fn test_handle_sse_values_event_string_content() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":"Hello, this is a response"}]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert_eq!(result, "Hello, this is a response");
    }

    #[test]
    fn test_handle_sse_values_event_array_content() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":[{"type":"text","text":"First part"},{"type":"text","text":"Second part"}]}]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.contains("First part"));
        assert!(result.contains("Second part"));
    }

    #[test]
    fn test_handle_sse_values_skips_empty_content_array() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":[]}]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_sse_values_skips_handoff_messages() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":"Transferring back to supervisor","response_metadata":{"__is_handoff_back":true}}]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty()); // Handoff messages are skipped
    }

    #[test]
    fn test_handle_sse_values_skips_transfer_messages() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":"Successfully transferred to agent"}]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty()); // Transfer messages are skipped
    }

    #[test]
    fn test_handle_sse_values_gets_last_meaningful_ai_message() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        // Multiple messages - should get the last meaningful AI message
        let data = r#"{"messages":[
            {"type":"human","content":"What OS am I using?"},
            {"type":"ai","content":"Let me check that for you."},
            {"type":"ai","content":"You are using Linux on WSL2."}
        ]}"#;

        let outcome = client.handle_sse_event("values", data, &mut result);
        assert!(outcome.is_ok());
        assert_eq!(result, "You are using Linux on WSL2.");
    }

    #[test]
    fn test_handle_sse_interrupt_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"__interrupt__":[{"value":{"type":"command_approval","command":"uname -a","message":"Approve?"}}]}"#;

        let outcome = client.handle_sse_event("updates", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.contains("__INTERRUPT_RESUME__"));
    }

    #[test]
    fn test_handle_sse_updates_no_interrupt() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"supervisor":{"messages":[]}}"#;

        let outcome = client.handle_sse_event("updates", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty()); // No interrupt marker
    }

    #[test]
    fn test_handle_sse_error_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"message":"Something went wrong"}"#;

        let outcome = client.handle_sse_event("error", data, &mut result);
        assert!(outcome.is_err());
        assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("Something went wrong"));
    }

    #[test]
    fn test_handle_sse_end_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();

        let outcome = client.handle_sse_event("end", "", &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_sse_unknown_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();

        let outcome = client.handle_sse_event("unknown_event", "{}", &mut result);
        assert!(outcome.is_ok());
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_sse_messages_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"[{"type":"ai","content":"Response from AI"}]"#;

        let outcome = client.handle_sse_event("messages", data, &mut result);
        assert!(outcome.is_ok());
        assert_eq!(result, "Response from AI");
    }

    #[test]
    fn test_handle_sse_messages_multiple_ai() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"[{"type":"ai","content":"First"},{"type":"ai","content":"Second"}]"#;

        let outcome = client.handle_sse_event("messages", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.contains("First"));
        assert!(result.contains("Second"));
    }

    #[test]
    fn test_handle_sse_messages_skips_human() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data =
            r#"[{"type":"human","content":"User message"},{"type":"ai","content":"AI response"}]"#;

        let outcome = client.handle_sse_event("messages", data, &mut result);
        assert!(outcome.is_ok());
        assert!(!result.contains("User message"));
        assert!(result.contains("AI response"));
    }

    #[test]
    fn test_stream_command_serialization() {
        let cmd = StreamCommand {
            resume: "approved".to_string(),
        };
        let json = serde_json::to_string(&cmd).unwrap();
        assert!(json.contains("approved"));
    }

    #[test]
    fn test_stream_run_request_serialization() {
        let request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".to_string()],
            input: Some(StreamInput {
                messages: vec![ChatMessage {
                    role: "user".to_string(),
                    content: "Hello".to_string(),
                }],
            }),
            command: None,
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("supervisor"));
        assert!(json.contains("values"));
        assert!(json.contains("Hello"));
        assert!(!json.contains("command")); // command is None, should be skipped
    }

    #[test]
    fn test_stream_run_request_with_command() {
        let request = StreamRunRequest {
            assistant_id: "supervisor".to_string(),
            stream_mode: vec!["values".to_string()],
            input: None,
            command: Some(StreamCommand {
                resume: "approved".to_string(),
            }),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("approved"));
        assert!(!json.contains("input")); // input is None, should be skipped
    }

    #[test]
    fn test_create_thread_request_serialization() {
        let request = CreateThreadRequest {
            metadata: serde_json::json!({"key": "value"}),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("metadata"));
        assert!(json.contains("key"));
    }

    #[test]
    fn test_create_thread_response_deserialization() {
        let json = r#"{"thread_id":"abc-123"}"#;
        let response: CreateThreadResponse = serde_json::from_str(json).unwrap();
        assert_eq!(response.thread_id, "abc-123");
    }

    #[test]
    fn test_http_client_debug_redacts_api_key() {
        let client = HttpLLMClient::new(
            "http://localhost:8080".to_string(),
            "secret-key".to_string(),
        );
        let debug_str = format!("{:?}", client);
        assert!(debug_str.contains("<redacted>"));
        assert!(!debug_str.contains("secret-key"));
    }

    #[tokio::test]
    async fn test_query_with_history_empty() {
        let mock = MockLLMClient;
        let response = mock
            .query_with_history("list files", &[])
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("ls"));
    }

    #[tokio::test]
    async fn test_query_with_history_has_commands() {
        let mock = MockLLMClient;
        let history = vec!["cd /home".to_string(), "ls -la".to_string()];
        let response = mock
            .query_with_history("list files", &history)
            .await
            .unwrap()
            .unwrap_complete();
        assert!(response.contains("ls"));
    }

    #[tokio::test]
    async fn test_mock_llm_cancellation_immediate() {
        let client = MockLLMClient;
        let token = CancellationToken::new();
        token.cancel(); // Cancel immediately before query

        let result = client.query_cancellable("test query", token).await;
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cancelled"),
            "Expected 'cancelled' in error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_mock_llm_query_success_no_cancellation() {
        let client = MockLLMClient;
        let token = CancellationToken::new();
        // Don't cancel - query should complete (but takes ~1s due to mock delay)
        // Use timeout to avoid hanging tests
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            client.query_cancellable("how to list files", token),
        )
        .await;

        assert!(result.is_ok(), "Query timed out");
        let query_result = result.unwrap();
        assert!(
            query_result.is_ok(),
            "Query failed: {:?}",
            query_result.err()
        );
    }

    #[tokio::test]
    async fn test_mock_llm_cancellation_during_delay() {
        let client = MockLLMClient;
        let token = CancellationToken::new();
        let token_clone = token.clone();

        // Spawn the query in background
        let handle =
            tokio::spawn(async move { client.query_cancellable("test query", token_clone).await });

        // Cancel after 150ms (between first and second 100ms chunks)
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        token.cancel();

        let result = handle.await.unwrap();
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("cancelled"),
            "Expected 'cancelled' in error: {err_msg}"
        );
    }

    #[tokio::test]
    async fn test_http_client_debug_no_secrets() {
        // Verify HttpLLMClient doesn't leak secrets in debug output
        let client = HttpLLMClient::new(
            "http://localhost:8080".to_string(),
            "sk-secret-key-12345".to_string(),
        );
        let debug_output = format!("{:?}", client);

        // Should contain type name
        assert!(debug_output.contains("HttpLLMClient"));
        // Should NOT contain the actual API key
        assert!(
            !debug_output.contains("sk-secret-key-12345"),
            "Debug output should not contain API key"
        );
        // Should show redacted
        assert!(debug_output.contains("<redacted>"));
    }

    #[tokio::test]
    async fn test_cancellation_token_cloning() {
        // Test that cancellation tokens work correctly when cloned
        let token = CancellationToken::new();
        let clone1 = token.clone();
        let clone2 = token.clone();

        assert!(!token.is_cancelled());
        assert!(!clone1.is_cancelled());
        assert!(!clone2.is_cancelled());

        token.cancel();

        // All clones should see the cancellation
        assert!(token.is_cancelled());
        assert!(clone1.is_cancelled());
        assert!(clone2.is_cancelled());
    }

    // =========================================================================
    // Tests for handle_sse_event_v2 (production SSE parser)
    // =========================================================================

    #[test]
    fn test_handle_sse_event_v2_metadata() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"run_id":"run-12345","attempt":1}"#;

        let outcome = client.handle_sse_event_v2("metadata", data, &mut result);
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_none()); // No interrupt
        assert!(result.is_empty());
    }

    #[test]
    fn test_handle_sse_event_v2_messages_ai() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"[{"type":"ai","content":"Hello from AI"}]"#;

        let outcome = client.handle_sse_event_v2("messages", data, &mut result);
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_none());
        assert_eq!(result, "Hello from AI");
    }

    #[test]
    fn test_handle_sse_event_v2_messages_assistant_role() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"[{"role":"assistant","content":"Response"}]"#;

        let outcome = client.handle_sse_event_v2("messages", data, &mut result);
        assert!(outcome.is_ok());
        assert_eq!(result, "Response");
    }

    #[test]
    fn test_handle_sse_event_v2_command_approval() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"__interrupt__":[{"value":{"type":"command_approval","command":"rm -rf /tmp/test","message":"Delete test files?"}}]}"#;

        let outcome = client.handle_sse_event_v2("updates", data, &mut result);
        assert!(outcome.is_ok());
        let interrupt = outcome.unwrap();
        assert!(interrupt.is_some());

        match interrupt.unwrap() {
            InterruptData::CommandApproval { command, message } => {
                assert_eq!(command, "rm -rf /tmp/test");
                assert_eq!(message, "Delete test files?");
            }
            _ => panic!("Expected CommandApproval interrupt"),
        }
    }

    #[test]
    fn test_handle_sse_event_v2_question_interrupt() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"__interrupt__":[{"value":{"type":"question","question":"What database?","options":["PostgreSQL","MySQL"]}}]}"#;

        let outcome = client.handle_sse_event_v2("updates", data, &mut result);
        assert!(outcome.is_ok());
        let interrupt = outcome.unwrap();
        assert!(interrupt.is_some());

        match interrupt.unwrap() {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "What database?");
                assert!(options.is_some());
                let opts = options.unwrap();
                assert_eq!(opts.len(), 2);
                assert!(opts.contains(&"PostgreSQL".to_string()));
            }
            _ => panic!("Expected Question interrupt"),
        }
    }

    #[test]
    fn test_handle_sse_event_v2_question_without_options() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data =
            r#"{"__interrupt__":[{"value":{"type":"question","message":"What is your name?"}}]}"#;

        let outcome = client.handle_sse_event_v2("updates", data, &mut result);
        assert!(outcome.is_ok());
        let interrupt = outcome.unwrap();
        assert!(interrupt.is_some());

        match interrupt.unwrap() {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "What is your name?");
                assert!(options.is_none());
            }
            _ => panic!("Expected Question interrupt"),
        }
    }

    #[test]
    fn test_handle_sse_event_v2_values_simple_content() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":"Simple response"}]}"#;

        let outcome = client.handle_sse_event_v2("values", data, &mut result);
        assert!(outcome.is_ok());
        assert_eq!(result, "Simple response");
    }

    #[test]
    fn test_handle_sse_event_v2_values_array_content() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"messages":[{"type":"ai","content":[{"type":"text","text":"Part 1"},{"type":"text","text":"Part 2"}]}]}"#;

        let outcome = client.handle_sse_event_v2("values", data, &mut result);
        assert!(outcome.is_ok());
        assert!(result.contains("Part 1"));
        assert!(result.contains("Part 2"));
    }

    #[test]
    fn test_handle_sse_event_v2_error() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();
        let data = r#"{"message":"API error occurred"}"#;

        let outcome = client.handle_sse_event_v2("error", data, &mut result);
        assert!(outcome.is_err());
        assert!(outcome.unwrap_err().to_string().contains("API error"));
    }

    #[test]
    fn test_handle_sse_event_v2_end() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();

        let outcome = client.handle_sse_event_v2("end", "", &mut result);
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_none());
    }

    #[test]
    fn test_handle_sse_event_v2_unknown_event() {
        let client =
            HttpLLMClient::new("http://localhost:8080".to_string(), "test-key".to_string());
        let mut result = String::new();

        let outcome = client.handle_sse_event_v2("unknown_event_type", "{}", &mut result);
        assert!(outcome.is_ok());
        assert!(outcome.unwrap().is_none());
    }

    // =========================================================================
    // Tests for LLMQueryResult
    // =========================================================================

    #[test]
    fn test_llm_query_result_complete() {
        let result = LLMQueryResult::Complete("Test response".to_string());
        assert_eq!(result.unwrap_complete(), "Test response");
    }

    #[test]
    fn test_llm_query_result_command_approval() {
        let result = LLMQueryResult::CommandApproval {
            command: "ls -la".to_string(),
            message: "List files".to_string(),
        };

        match result {
            LLMQueryResult::CommandApproval { command, message } => {
                assert_eq!(command, "ls -la");
                assert_eq!(message, "List files");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_llm_query_result_question() {
        let result = LLMQueryResult::Question {
            question: "Choose option".to_string(),
            options: Some(vec!["A".to_string(), "B".to_string()]),
        };

        match result {
            LLMQueryResult::Question { question, options } => {
                assert_eq!(question, "Choose option");
                assert!(options.is_some());
                assert_eq!(options.unwrap().len(), 2);
            }
            _ => panic!("Expected Question"),
        }
    }

    // =========================================================================
    // Tests for InterruptData
    // =========================================================================

    #[test]
    fn test_interrupt_data_command_approval() {
        let data = InterruptData::CommandApproval {
            command: "docker ps".to_string(),
            message: "Check containers".to_string(),
        };

        match data {
            InterruptData::CommandApproval { command, message } => {
                assert_eq!(command, "docker ps");
                assert_eq!(message, "Check containers");
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_interrupt_data_question_with_options() {
        let data = InterruptData::Question {
            question: "Select environment".to_string(),
            options: Some(vec!["dev".to_string(), "prod".to_string()]),
        };

        match data {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "Select environment");
                let opts = options.unwrap();
                assert!(opts.contains(&"dev".to_string()));
                assert!(opts.contains(&"prod".to_string()));
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_interrupt_data_question_without_options() {
        let data = InterruptData::Question {
            question: "Enter project name".to_string(),
            options: None,
        };

        match data {
            InterruptData::Question { question, options } => {
                assert_eq!(question, "Enter project name");
                assert!(options.is_none());
            }
            _ => panic!("Expected Question"),
        }
    }
}
