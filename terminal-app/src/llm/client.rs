//! LLM client for natural language queries.

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Result of an LLM query.
#[derive(Debug, Clone)]
pub enum LLMQueryResult {
    /// Query completed with a final response.
    Complete(String),
    /// LLM wants to execute a command and needs approval (y/n).
    CommandApproval {
        /// The command the LLM wants to execute.
        command: String,
        /// Description/reason from the LLM.
        message: String,
    },
    /// LLM is asking a question (free-form text answer).
    Question {
        /// The question being asked.
        question: String,
        /// Optional predefined choices.
        options: Option<Vec<String>>,
    },
}

/// Request to the LLM backend.
#[derive(Debug, Serialize)]
struct LLMRequest {
    query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    context: Option<String>,
}

/// Response from the LLM backend.
#[derive(Debug, Deserialize)]
struct LLMResponse {
    text: String,
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    question: Option<String>,
    #[serde(default)]
    options: Option<Vec<String>>,
}

/// LLM client configuration.
#[derive(Debug, Clone)]
pub struct LLMClientConfig {
    /// Backend URL.
    pub base_url: String,
    /// API key for authentication.
    pub api_key: Option<String>,
}

impl Default for LLMClientConfig {
    fn default() -> Self {
        Self {
            base_url: std::env::var("INFRAWARE_BACKEND_URL")
                .unwrap_or_else(|_| "http://localhost:8000".to_string()),
            api_key: std::env::var("BACKEND_API_KEY").ok(),
        }
    }
}

/// LLM client for sending queries to the backend.
#[derive(Debug, Clone)]
pub struct LLMClient {
    config: LLMClientConfig,
    client: reqwest::Client,
}

impl LLMClient {
    /// Create a new LLM client with default configuration.
    pub fn new() -> Self {
        Self::with_config(LLMClientConfig::default())
    }

    /// Create a new LLM client with custom configuration.
    pub fn with_config(config: LLMClientConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    /// Query the LLM with a failed command.
    pub async fn query_failed_command(&self, command: &str) -> Result<LLMQueryResult> {
        let query = format!(
            "I tried to run '{}' but got 'command not found'. What should I do?",
            command
        );

        self.query(&query).await
    }

    /// Query the LLM with natural language.
    pub async fn query(&self, text: &str) -> Result<LLMQueryResult> {
        let request = LLMRequest {
            query: text.to_string(),
            context: None,
        };

        let mut request_builder = self
            .client
            .post(format!("{}/query", self.config.base_url))
            .json(&request);

        // Add API key if configured
        if let Some(ref api_key) = self.config.api_key {
            request_builder = request_builder.bearer_auth(api_key);
        }

        let response = request_builder.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            anyhow::bail!("LLM request failed with status {}: {}", status, body);
        }

        let llm_response: LLMResponse = response.json().await?;

        // Determine result type based on response
        if let Some(command) = llm_response.command {
            Ok(LLMQueryResult::CommandApproval {
                command,
                message: llm_response.text,
            })
        } else if let Some(question) = llm_response.question {
            Ok(LLMQueryResult::Question {
                question,
                options: llm_response.options,
            })
        } else {
            Ok(LLMQueryResult::Complete(llm_response.text))
        }
    }

    /// Resume with an answer to a question.
    pub async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult> {
        self.query(answer).await
    }
}

impl Default for LLMClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_llm_query_result_variants() {
        let complete = LLMQueryResult::Complete("Hello".to_string());
        assert!(matches!(complete, LLMQueryResult::Complete(_)));

        let approval = LLMQueryResult::CommandApproval {
            command: "ls".to_string(),
            message: "List files".to_string(),
        };
        assert!(matches!(approval, LLMQueryResult::CommandApproval { .. }));

        let question = LLMQueryResult::Question {
            question: "What do you want?".to_string(),
            options: Some(vec!["A".to_string(), "B".to_string()]),
        };
        assert!(matches!(question, LLMQueryResult::Question { .. }));
    }
}
