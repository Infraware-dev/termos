/// LLM client for natural language queries
use anyhow::Result;
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
pub struct LLMResponse {
    pub text: String,
    #[serde(default)]
    pub metadata: Option<serde_json::Value>,
}

/// Client for interacting with the LLM backend
pub struct LLMClient {
    base_url: String,
    client: reqwest::Client,
    timeout_secs: u64,
}

impl LLMClient {
    /// Create a new LLM client
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
            timeout_secs: 30,
        }
    }

    /// Create a new LLM client with custom timeout
    pub fn with_timeout(base_url: String, timeout_secs: u64) -> Self {
        Self {
            base_url,
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(timeout_secs))
                .build()
                .unwrap(),
            timeout_secs,
        }
    }

    /// Query the LLM with a natural language input
    pub async fn query(&self, text: &str, context: Option<String>) -> Result<String> {
        let request = LLMRequest {
            query: text.to_string(),
            context,
        };

        let response = self
            .client
            .post(&format!("{}/query", self.base_url))
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

    /// Query with command history context
    pub async fn query_with_history(
        &self,
        text: &str,
        command_history: &[String],
    ) -> Result<String> {
        let context = if !command_history.is_empty() {
            Some(format!("Recent commands:\n{}", command_history.join("\n")))
        } else {
            None
        };

        self.query(text, context).await
    }
}

/// Mock LLM client for testing
pub struct MockLLMClient;

impl MockLLMClient {
    /// Create a mock response for testing
    pub async fn query(&self, text: &str) -> Result<String> {
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
