//! Shell command execution tool for rig-rs
//!
//! This module implements a proper rig-rs Tool for executing shell commands
//! with HITL (Human-in-the-Loop) approval support.
//!
//! Note: These tools are currently not integrated with rig-rs's automatic
//! tool execution due to complex type handling in rig 0.28. They are kept
//! as reference implementations for future integration.

// Shell command tool integrated with rig-rs native function calling

use std::process::Stdio;

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};
use tokio::process::Command;
use tokio::time::{Duration, timeout};

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

/// Result of a shell command execution
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
    /// Command was executed successfully
    Executed {
        /// The command that was executed
        command: String,
        /// Combined stdout and stderr output
        output: String,
        /// Whether the command exited successfully (exit code 0)
        success: bool,
        /// Exit code if available
        exit_code: Option<i32>,
    },
    /// Command execution failed
    Failed {
        /// The command that failed
        command: String,
        /// Error message
        error: String,
    },
}

/// Error type for shell command execution
#[derive(Debug, thiserror::Error)]
pub enum ShellError {
    /// Command execution failed due to I/O error
    #[error("Command execution failed: {0}")]
    ExecutionFailed(String),

    /// Command timed out
    #[error("Command timed out after {0} seconds")]
    Timeout(u64),

    /// Command was not approved (reserved for future HITL rejection handling)
    #[error("Command was not approved by user")]
    #[allow(dead_code)]
    NotApproved,

    /// Command contains dangerous patterns
    #[error("Dangerous command blocked: {0}")]
    DangerousCommand(String),
}

/// Tool for executing shell commands with HITL approval
///
/// This tool integrates with rig-rs's native function calling system.
/// When the LLM wants to execute a shell command, it will call this tool
/// with the command and explanation. The tool returns a `PendingApproval`
/// status, which the orchestrator converts into an interrupt event.
#[derive(Debug, Clone)]
pub struct ShellCommandTool {
    /// Whether to require user approval before execution
    pub require_approval: bool,
    /// Timeout in seconds for command execution
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
    /// By default, requires approval and has a 30 second timeout.
    pub fn new() -> Self {
        Self {
            require_approval: true,
            timeout_secs: 30,
        }
    }

    /// Set whether to require approval (builder pattern)
    #[allow(dead_code)] // Public API for future configurability
    pub fn with_approval(mut self, require: bool) -> Self {
        self.require_approval = require;
        self
    }

    /// Set the timeout in seconds (builder pattern)
    #[allow(dead_code)] // Public API for future configurability
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

    /// Execute a shell command asynchronously with safety checks
    ///
    /// Uses `tokio::process::Command` for non-blocking execution.
    /// Validates command against dangerous patterns before execution.
    pub async fn execute(&self, command: &str) -> Result<ShellCommandResult, ShellError> {
        // Validate command before execution
        Self::validate_command(command)?;

        // Spawn with safety settings
        let child = Command::new("sh")
            .arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| ShellError::ExecutionFailed(e.to_string()))?;

        let output = timeout(
            Duration::from_secs(self.timeout_secs),
            child.wait_with_output(),
        )
        .await
        .map_err(|_| ShellError::Timeout(self.timeout_secs))?
        .map_err(|e| ShellError::ExecutionFailed(e.to_string()))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);

        let combined = if stdout.is_empty() && stderr.is_empty() {
            "(no output)".to_string()
        } else if stderr.is_empty() {
            stdout.to_string()
        } else if stdout.is_empty() {
            stderr.to_string()
        } else {
            format!("{}\n{}", stdout.trim_end(), stderr.trim_end())
        };

        Ok(ShellCommandResult::Executed {
            command: command.to_string(),
            output: combined,
            success: output.status.success(),
            exit_code: output.status.code(),
        })
    }
}

impl Tool for ShellCommandTool {
    const NAME: &'static str = "execute_shell_command";

    type Error = ShellError;
    type Args = ShellCommandArgs;
    type Output = ShellCommandResult;

    // Note: rig-rs 0.28 requires `impl Future` signature, not `async fn`
    #[allow(clippy::manual_async_fn)]
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

    // Note: rig-rs 0.28 requires `impl Future` signature, not `async fn`
    #[allow(clippy::manual_async_fn)]
    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        let require_approval = self.require_approval;
        let timeout_secs = self.timeout_secs;
        let command = args.command.clone();
        let explanation = args.explanation.clone();

        let needs_continuation = args.needs_continuation;

        async move {
            if require_approval {
                // Return pending approval - the orchestrator will handle HITL
                Ok(ShellCommandResult::PendingApproval {
                    command,
                    explanation,
                    needs_continuation,
                })
            } else {
                // Direct execution (used after approval)
                let tool = ShellCommandTool {
                    require_approval: false,
                    timeout_secs,
                };
                tool.execute(&command).await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shell_tool_default() {
        let tool = ShellCommandTool::new();
        assert!(tool.require_approval);
        assert_eq!(tool.timeout_secs, 30);
    }

    #[test]
    fn test_shell_tool_builder() {
        let tool = ShellCommandTool::new()
            .with_approval(false)
            .with_timeout(60);

        assert!(!tool.require_approval);
        assert_eq!(tool.timeout_secs, 60);
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
            _ => panic!("Expected PendingApproval"),
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
            _ => panic!("Expected PendingApproval"),
        }
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let tool = ShellCommandTool::new().with_approval(false);
        let result = tool.execute("echo hello").await.unwrap();

        match result {
            ShellCommandResult::Executed {
                output, success, ..
            } => {
                assert!(success);
                assert!(output.contains("hello"));
            }
            _ => panic!("Expected Executed"),
        }
    }

    #[tokio::test]
    async fn test_execute_failing_command() {
        let tool = ShellCommandTool::new().with_approval(false);
        let result = tool.execute("exit 1").await.unwrap();

        match result {
            ShellCommandResult::Executed {
                success, exit_code, ..
            } => {
                assert!(!success);
                assert_eq!(exit_code, Some(1));
            }
            _ => panic!("Expected Executed"),
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
