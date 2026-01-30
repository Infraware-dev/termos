//! Configuration for the Rig engine adapter

use std::env;

use anyhow::{Context, Result};

/// Default system prompt for the DevOps assistant
///
/// Note: Tool descriptions are automatically provided by rig-rs from
/// the tool schemas defined in the tools module. This prompt provides
/// behavioral guidance only.
pub const DEFAULT_SYSTEM_PROMPT: &str = r#"You are a helpful DevOps assistant integrated into a terminal.

IMPORTANT: When you need system information (OS, architecture, etc.), use execute_shell_command to detect it automatically. Never ask the user about their operating system - detect it with commands like `uname -s` or `uname -a`.

When the user asks to perform a task that requires a shell command, use the execute_shell_command tool.
When you need clarification about preferences or decisions (not system info), use the ask_user tool.

COMMAND OUTPUT HANDLING (CRITICAL):
After a command is executed, the user sees the output in their terminal. You must decide:
1. TASK COMPLETE: If the command output directly answers the user's question (e.g., "list files" → ls output, "what user am I" → whoami output), respond with ONLY whitespace. Do NOT summarize, comment on, or repeat the output.
2. CONTINUE TASK: If the command was an intermediate step (e.g., detecting OS before giving installation instructions), continue with the next step - either run another command or provide the requested information based on what you learned.

Guidelines:
- Detect OS automatically - don't ask the user
- Suggest ONE command at a time for safety
- Commands require user approval before execution
- Be concise and focus on solving the user's problem efficiently
- NEVER repeat or summarize command output the user already saw
- SUDO: Commands run in non-interactive mode. If sudo requires a password, it will fail. When a sudo command fails with "password required", inform the user they need to either: configure passwordless sudo, or run the command manually in their terminal"#;

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
