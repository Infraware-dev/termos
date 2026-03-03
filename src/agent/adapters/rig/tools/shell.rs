//! Shell command tool for rig-rs
//!
//! This module implements a rig-rs Tool for shell commands with HITL
//! (Human-in-the-Loop) approval. The tool is pure schema + validation:
//! it always returns `PendingApproval` and never executes commands itself.
//! Actual execution happens via the PTY session after user approval.

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

/// Arguments for the shell command tool
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct ShellCommandArgs {
    /// The shell command to execute
    #[schemars(description = "The shell command to execute")]
    pub command: String,

    /// Brief explanation of what this command does
    #[schemars(description = "A brief explanation of what this command does and why it's needed")]
    pub explanation: String,

    /// Whether the agent needs to process the command output after execution.
    /// Set to false when the command output directly answers the user's question (e.g., ls, whoami).
    /// Set to true when the output is needed for the agent to continue (e.g., detecting OS before installation).
    #[schemars(
        description = "Set to false if command output directly answers the user's question (e.g., 'list files' -> ls). Set to true if you need to process the output to continue the task (e.g., detect OS -> then provide installation instructions)."
    )]
    #[serde(default)]
    pub needs_continuation: bool,
}

/// Result of a shell command tool call
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ShellCommandResult {
    /// Command requires user approval before execution
    PendingApproval {
        /// The command that needs approval
        command: String,
        /// Explanation of what the command does
        explanation: String,
        /// Whether agent needs to process output after execution
        needs_continuation: bool,
    },
}

/// Error type for shell command tool
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// Command was not approved (reserved for future HITL rejection handling)
    #[error("Command was not approved by user")]
    #[expect(dead_code, reason = "reserved for future HITL rejection handling")]
    NotApproved,

    /// Command contains dangerous patterns
    #[error("Dangerous command blocked: {0}")]
    DangerousCommand(String),
}

/// Tool for shell commands with HITL approval
///
/// This tool integrates with rig-rs's native function calling system.
/// When the LLM wants to execute a shell command, it calls this tool
/// with the command and explanation. The tool validates the command and
/// returns `PendingApproval`, which the orchestrator converts into an
/// interrupt event. Actual execution happens via the PTY session.
#[derive(Debug, Clone)]
pub struct ShellCommandTool {
    /// Timeout hint in seconds (metadata for the frontend, not enforced here)
    pub timeout_secs: u64,
}

impl Default for ShellCommandTool {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellCommandTool {
    /// Create a new shell command tool with default settings
    ///
    /// Default timeout hint is 30 seconds.
    pub fn new() -> Self {
        Self { timeout_secs: 30 }
    }

    /// Set the timeout hint in seconds (builder pattern)
    #[expect(dead_code, reason = "public API for future configurability")]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Validate a command for dangerous patterns
    fn validate_command(command: &str) -> Result<(), ShellError> {
        const DANGEROUS_PATTERNS: &[&str] = &[
            "rm -rf /",
            "rm -fr /",
            "dd if=",
            "> /dev/sd",
            "> /dev/nvme",
            "mkfs.",
            ":(){ :|:& };:",
            "chmod -R 777 /",
            "chown -R",
            "> /etc/passwd",
            "> /etc/shadow",
            "curl | sh",
            "curl | bash",
            "wget | sh",
            "wget | bash",
        ];

        let cmd_lower = command.to_lowercase();

        for pattern in DANGEROUS_PATTERNS {
            if cmd_lower.contains(pattern) {
                return Err(ShellError::DangerousCommand(format!(
                    "blocked pattern: {}",
                    pattern
                )));
            }
        }

        if command.contains(';') {
            return Err(ShellError::DangerousCommand(
                "command chaining with ';' not allowed".to_string(),
            ));
        }

        if command.contains('`') {
            return Err(ShellError::DangerousCommand(
                "backtick command substitution not allowed".to_string(),
            ));
        }

        Ok(())
    }
}

impl Tool for ShellCommandTool {
    const NAME: &'static str = "execute_shell_command";

    type Error = ShellError;
    type Args = ShellCommandArgs;
    type Output = ShellCommandResult;

    // Note: rig-rs 0.28 requires `impl Future` signature, not `async fn`
    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs 0.28 requires impl Future signature"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Execute a shell command on the system. Use this tool when the user \
                    asks to perform file operations, run programs, check system status, or \
                    interact with the operating system in any way. Always provide a clear \
                    explanation of what the command does."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(ShellCommandArgs))
                    .expect("Failed to generate JSON schema for ShellCommandArgs"),
            }
        }
    }

    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        let command = args.command.clone();
        let explanation = args.explanation.clone();
        let needs_continuation = args.needs_continuation;

        async move {
            Self::validate_command(&command)?;
            Ok(ShellCommandResult::PendingApproval {
                command,
                explanation,
                needs_continuation,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_tool_default() {
        let tool = ShellCommandTool::new();
        assert_eq!(tool.timeout_secs, 30);
    }

    #[tokio::test]
    async fn test_tool_call_returns_pending() {
        let tool = ShellCommandTool::new();
        let args = ShellCommandArgs {
            command: "ls -la".to_string(),
            explanation: "List files".to_string(),
            needs_continuation: false,
        };

        let result = tool.call(args).await.unwrap();

        match result {
            ShellCommandResult::PendingApproval {
                command,
                needs_continuation,
                ..
            } => {
                assert_eq!(command, "ls -la");
                assert!(!needs_continuation);
            }
        }
    }

    #[tokio::test]
    async fn test_tool_call_with_continuation() {
        let tool = ShellCommandTool::new();
        let args = ShellCommandArgs {
            command: "uname -s".to_string(),
            explanation: "Detect OS".to_string(),
            needs_continuation: true,
        };

        let result = tool.call(args).await.unwrap();

        match result {
            ShellCommandResult::PendingApproval {
                needs_continuation, ..
            } => {
                assert!(needs_continuation);
            }
        }
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = ShellCommandTool::new();
        let def = tool.definition(String::new()).await;

        assert_eq!(def.name, "execute_shell_command");
        assert!(def.description.contains("shell command"));
        assert!(def.parameters.is_object());
    }

    #[test]
    fn test_shell_command_args_schema() {
        let schema = schema_for!(ShellCommandArgs);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("command"));
        assert!(json.contains("explanation"));
    }
}
