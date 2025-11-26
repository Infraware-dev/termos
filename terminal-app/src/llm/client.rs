/// LLM client for natural language queries
use anyhow::Result;
use async_trait::async_trait;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;

/// Request to the LLM backend (legacy - kept for backward compatibility)
#[derive(Debug, Serialize)]
#[allow(dead_code)] // Legacy struct - may be used for non-streaming endpoints
pub struct LLMRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Response from the LLM backend
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Struct fields used in HTTP deserialization, metadata for M2/M3
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

/// Trait for LLM client implementations
///
/// This trait allows different LLM backends (mock, HTTP, OpenAI, etc.)
/// to be used interchangeably via dependency injection
#[async_trait]
pub trait LLMClientTrait: Send + Sync + std::fmt::Debug {
    /// Query the LLM with natural language input
    async fn query(&self, text: &str) -> Result<String>;

    /// Query with additional context
    async fn query_with_context(&self, text: &str, _context: Option<String>) -> Result<String> {
        // Default implementation ignores context
        self.query(text).await
    }

    /// Query with command history context (M2/M3)
    #[allow(dead_code)] // Context-aware LLM API for M2/M3
    async fn query_with_history(&self, text: &str, command_history: &[String]) -> Result<String> {
        let context = if command_history.is_empty() {
            None
        } else {
            Some(format!("Recent commands:\n{}", command_history.join("\n")))
        };

        self.query_with_context(text, context).await
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
                .timeout(std::time::Duration::from_secs(120)) // SSE streams need longer timeout
                .build()
                .unwrap_or_default(),
            api_key,
            thread_id: RwLock::new(None),
        }
    }

    /// Create a new HTTP LLM client with custom timeout
    #[allow(dead_code)] // Constructor for custom timeout configuration
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
        log::debug!("Creating new LLM thread at {}/threads", self.base_url);

        let request = CreateThreadRequest {
            metadata: serde_json::json!({}),
        };

        let response = self
            .client
            .post(format!("{}/threads", self.base_url))
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

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
    async fn stream_run(
        &self,
        thread_id: &str,
        input: Option<&str>,
        resume: bool,
    ) -> Result<String> {
        let url = format!("{}/threads/{}/runs/stream", self.base_url, thread_id);
        log::debug!("Starting stream run at {}", url);

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

        let response = self
            .client
            .post(&url)
            .header("X-Api-Key", &self.api_key)
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            log::error!("Stream run failed ({}): {}", status, error_text);
            anyhow::bail!("Stream run failed ({}): {}", status, error_text);
        }

        self.parse_sse_stream(response).await
    }

    /// Parse SSE stream and accumulate AI messages
    async fn parse_sse_stream(&self, response: reqwest::Response) -> Result<String> {
        let mut result = String::new();
        let mut stream = response.bytes_stream();
        let mut current_event: Option<String> = None;
        let mut buffer = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result?;
            let text = String::from_utf8_lossy(&chunk);
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
                        self.handle_sse_event(event, data, &mut result)?;
                    }
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
                    self.handle_sse_event(event, data, &mut result)?;
                }
            }
        }

        log::debug!("Stream completed, result length: {} chars", result.len());
        Ok(result)
    }

    /// Handle a single SSE event
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
                // Check for interrupt (M2: human-in-the-loop)
                if let Ok(updates) = serde_json::from_str::<serde_json::Value>(data) {
                    if updates.get("__interrupt__").is_some() {
                        log::warn!("Run interrupted - human approval required (M2 feature)");
                        // For now, we just log it - M2 will implement approval flow
                    }
                }
            }
            "values" => {
                // State updates - logged for debugging
                log::trace!("State update received");
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
}

#[async_trait]
impl LLMClientTrait for HttpLLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        self.query_with_context(text, None).await
    }

    async fn query_with_context(&self, text: &str, context: Option<String>) -> Result<String> {
        log::debug!("LLM query: {} (context: {:?})", text, context.is_some());

        // Use streaming endpoint via threads
        let thread_id = self.ensure_thread().await?;

        // Combine context with text if provided
        let full_query = match context {
            Some(ctx) => format!("{}\n\nContext:\n{}", text, ctx),
            None => text.to_string(),
        };

        self.stream_run(&thread_id, Some(&full_query), false).await
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
    async fn query(&self, text: &str) -> Result<String> {
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

        Ok(response.to_string())
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
        let response = mock.query("how to list files").await.unwrap();
        assert!(response.contains("ls"));
    }

    #[tokio::test]
    async fn test_mock_llm_docker() {
        let mock = MockLLMClient;
        let response = mock.query("what is docker").await.unwrap();
        assert!(response.contains("Docker"));
    }

    #[tokio::test]
    async fn test_mock_llm_kubernetes() {
        let mock = MockLLMClient;
        let response = mock.query("what is kubernetes").await.unwrap();
        assert!(response.contains("Kubernetes"));
        assert!(response.contains("kubectl"));
    }

    #[tokio::test]
    async fn test_mock_llm_k8s() {
        let mock = MockLLMClient;
        let response = mock.query("help with k8s").await.unwrap();
        assert!(response.contains("Kubernetes"));
    }

    #[tokio::test]
    async fn test_mock_llm_fallback() {
        let mock = MockLLMClient;
        let response = mock.query("something random").await.unwrap();
        assert!(response.contains("mock LLM"));
    }

    #[tokio::test]
    async fn test_mock_llm_case_insensitive() {
        let mock = MockLLMClient;
        let response = mock.query("DOCKER containers").await.unwrap();
        assert!(response.contains("Docker"));
    }
}
