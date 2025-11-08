/// LLM client for natural language queries
use anyhow::Result;
use async_trait::async_trait;
use reqwest;
use serde::{Deserialize, Serialize};

/// Request to the LLM backend
#[derive(Debug, Serialize)]
pub struct LLMRequest {
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Response from the LLM backend
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct LLMResponse {
    pub text: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Trait for LLM client implementations
///
/// This trait allows different LLM backends (mock, HTTP, OpenAI, etc.)
/// to be used interchangeably via dependency injection
#[async_trait]
pub trait LLMClientTrait: Send + Sync {
    /// Query the LLM with natural language input
    async fn query(&self, text: &str) -> Result<String>;

    /// Query with additional context
    async fn query_with_context(&self, text: &str, _context: Option<String>) -> Result<String> {
        // Default implementation ignores context
        self.query(text).await
    }

    /// Query with command history context (M2/M3)
    #[allow(dead_code)]
    async fn query_with_history(&self, text: &str, command_history: &[String]) -> Result<String> {
        let context = if !command_history.is_empty() {
            Some(format!("Recent commands:\n{}", command_history.join("\n")))
        } else {
            None
        };

        self.query_with_context(text, context).await
    }
}

/// HTTP-based LLM client for production use
pub struct HttpLLMClient {
    base_url: String,
    client: reqwest::Client,
}

impl HttpLLMClient {
    /// Create a new HTTP LLM client
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Create a new HTTP LLM client with custom timeout (M2/M3)
    #[allow(dead_code)]
    pub fn with_timeout(base_url: String, timeout_secs: u64) -> Result<Self> {
        Ok(Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build()?,
        })
    }
}

#[async_trait]
impl LLMClientTrait for HttpLLMClient {
    async fn query(&self, text: &str) -> Result<String> {
        self.query_with_context(text, None).await
    }

    async fn query_with_context(&self, text: &str, context: Option<String>) -> Result<String> {
        let request = LLMRequest {
            query: text.to_string(),
            context,
        };

        let response = self
            .client
            .post(format!("{}/query", self.base_url))
            .json(&request)
            .send()
            .await?;

        // Check for errors
        if !response.status().is_success() {
            anyhow::bail!("LLM request failed with status: {}", response.status());
        }

        let llm_response: LLMResponse = response.json().await?;

        Ok(llm_response.text)
    }
}

/// Mock LLM client for testing and development
#[derive(Default)]
pub struct MockLLMClient;

impl MockLLMClient {
    pub fn new() -> Self {
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
}
