//! Configuration for the Rig engine adapter

use std::env;

use anyhow::{Context, Result};

/// Default system prompt for the DevOps assistant
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a Unix/Linux command assistant integrated into a terminal.

When the user asks to perform a task that requires a shell command:
1. Respond ONLY with the command in this exact format: [EXECUTE: command_here]
2. Add a brief explanation on the next line
3. Do NOT run multiple commands - suggest ONE at a time

Example response for "list files":
[EXECUTE: ls -la]
This will list all files including hidden ones with detailed information.

When you need to ask a question, use this format:
[QUESTION: your question here]
[OPTIONS: option1, option2, option3]

CRITICAL: Always use the [EXECUTE: ...] format for commands. Never just describe what a command would do."#;

/// Configuration for the Rig engine
#[derive(Debug, Clone)]
pub struct RigEngineConfig {
    /// Anthropic API key (required)
    pub api_key: String,
    /// Model to use (default: claude-sonnet-4-20250514)
    pub model: String,
    /// Maximum tokens for responses (required by Anthropic)
    pub max_tokens: u32,
    /// System prompt for the agent
    pub system_prompt: String,
    /// Timeout in seconds for API calls
    pub timeout_secs: u64,
    /// Temperature for response generation
    pub temperature: f32,
}

impl RigEngineConfig {
    /// Create a new configuration with the given API key
    pub fn new(api_key: impl Into<String>) -> Self {
        Self {
            api_key: api_key.into(),
            model: "claude-sonnet-4-20250514".to_string(),
            max_tokens: 4096,
            system_prompt: DEFAULT_SYSTEM_PROMPT.to_string(),
            timeout_secs: 300,
            temperature: 0.7,
        }
    }

    /// Load configuration from environment variables
    pub fn from_env() -> Result<Self> {
        let api_key = env::var("ANTHROPIC_API_KEY")
            .context("ANTHROPIC_API_KEY environment variable is required for rig engine")?;

        let model =
            env::var("ANTHROPIC_MODEL").unwrap_or_else(|_| "claude-sonnet-4-20250514".to_string());

        let max_tokens = env::var("RIG_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(4096);

        let system_prompt =
            env::var("RIG_SYSTEM_PROMPT").unwrap_or_else(|_| DEFAULT_SYSTEM_PROMPT.to_string());

        let timeout_secs = env::var("RIG_TIMEOUT_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);

        let temperature = env::var("RIG_TEMPERATURE")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.7);

        Ok(Self {
            api_key,
            model,
            max_tokens,
            system_prompt,
            timeout_secs,
            temperature,
        })
    }

    /// Set the model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Set the system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Set the timeout in seconds
    pub fn with_timeout(mut self, timeout_secs: u64) -> Self {
        self.timeout_secs = timeout_secs;
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = RigEngineConfig::new("test-api-key");
        assert_eq!(config.api_key, "test-api-key");
        assert_eq!(config.model, "claude-sonnet-4-20250514");
        assert_eq!(config.max_tokens, 4096);
        assert_eq!(config.timeout_secs, 300);
        assert!((config.temperature - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn test_config_builder() {
        let config = RigEngineConfig::new("test-key")
            .with_model("claude-3-5-haiku-20241022")
            .with_max_tokens(2048)
            .with_timeout(600)
            .with_temperature(0.5);

        assert_eq!(config.model, "claude-3-5-haiku-20241022");
        assert_eq!(config.max_tokens, 2048);
        assert_eq!(config.timeout_secs, 600);
        assert!((config.temperature - 0.5).abs() < f32::EPSILON);
    }
}
