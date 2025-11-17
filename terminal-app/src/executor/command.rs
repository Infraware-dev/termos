/// Command execution module
use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, Duration};

/// Output from a command execution
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    /// Check if the command was successful
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get combined output (stdout + stderr)
    #[allow(dead_code)]
    pub fn combined_output(&self) -> String {
        let mut result = String::new();
        if !self.stdout.is_empty() {
            result.push_str(&self.stdout);
        }
        if !self.stderr.is_empty() {
            if !result.is_empty() {
                result.push('\n');
            }
            result.push_str(&self.stderr);
        }
        result
    }
}

/// Command executor for running shell commands
pub struct CommandExecutor;

impl CommandExecutor {
    /// Execute a command asynchronously with a 5-minute timeout
    pub async fn execute(cmd: &str, args: &[String]) -> Result<CommandOutput> {
        // Check if command exists
        if !Self::command_exists(cmd) {
            anyhow::bail!("Command '{}' not found", cmd);
        }

        // Execute the command with timeout
        let execution = TokioCommand::new(cmd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output();

        let output = timeout(Duration::from_secs(300), execution)
            .await
            .context("Command execution timed out after 5 minutes")??;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }

    /// Check if a command exists in the PATH
    pub fn command_exists(cmd: &str) -> bool {
        which::which(cmd).is_ok()
    }

    /// Get the full path of a command
    #[allow(dead_code)]
    pub fn get_command_path(cmd: &str) -> Option<String> {
        which::which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
    }

    /// Execute a command with sudo privileges (M2/M3)
    #[allow(dead_code)]
    pub async fn execute_sudo(cmd: &str, args: &[String]) -> Result<CommandOutput> {
        // Check if command exists
        if !Self::command_exists(cmd) {
            anyhow::bail!("Command '{}' not found", cmd);
        }

        // Use TokioCommand directly to ensure proper argument separation
        // and avoid command injection vulnerabilities
        let output = TokioCommand::new("sudo")
            .arg(cmd)
            .args(args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await?;

        Ok(CommandOutput {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code().unwrap_or(-1),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_simple_command() {
        let output = CommandExecutor::execute("echo", &["hello".to_string()])
            .await
            .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let result = CommandExecutor::execute("nonexistentcommand123", &[]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_command_exists() {
        assert!(CommandExecutor::command_exists("echo"));
        assert!(!CommandExecutor::command_exists("nonexistentcommand123"));
    }

    #[tokio::test]
    async fn test_command_with_multiple_args() {
        let output = CommandExecutor::execute("echo", &["hello".to_string(), "world".to_string()])
            .await
            .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_command_with_stderr() {
        // Use a command that outputs to stderr (grep with no match)
        let output =
            CommandExecutor::execute("sh", &["-c".to_string(), "echo error >&2".to_string()])
                .await
                .unwrap();
        assert!(output.is_success());
        assert!(output.stderr.contains("error"));
    }

    #[tokio::test]
    async fn test_command_exit_code() {
        // Use false command which exits with code 1
        let output = CommandExecutor::execute("sh", &["-c".to_string(), "exit 42".to_string()])
            .await
            .unwrap();
        assert!(!output.is_success());
        assert_eq!(output.exit_code, 42);
    }

    #[test]
    fn test_combined_output_both() {
        let output = CommandOutput {
            stdout: "out".to_string(),
            stderr: "err".to_string(),
            exit_code: 0,
        };
        let combined = output.combined_output();
        assert!(combined.contains("out"));
        assert!(combined.contains("err"));
    }

    #[test]
    fn test_combined_output_stdout_only() {
        let output = CommandOutput {
            stdout: "out".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert_eq!(output.combined_output(), "out");
    }

    #[test]
    fn test_combined_output_stderr_only() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: "err".to_string(),
            exit_code: 0,
        };
        assert_eq!(output.combined_output(), "err");
    }

    #[test]
    fn test_combined_output_empty() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert_eq!(output.combined_output(), "");
    }

    #[test]
    fn test_get_command_path() {
        let path = CommandExecutor::get_command_path("echo");
        assert!(path.is_some());
        assert!(path.unwrap().contains("echo"));
    }

    #[test]
    fn test_get_command_path_not_found() {
        let path = CommandExecutor::get_command_path("nonexistentcommand123");
        assert!(path.is_none());
    }

    #[test]
    fn test_is_success_false() {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 1,
        };
        assert!(!output.is_success());
    }

    #[test]
    fn test_command_output_equality() {
        let output1 = CommandOutput {
            stdout: "test".to_string(),
            stderr: "error".to_string(),
            exit_code: 0,
        };
        let output2 = CommandOutput {
            stdout: "test".to_string(),
            stderr: "error".to_string(),
            exit_code: 0,
        };
        assert_eq!(output1, output2);
    }

    #[test]
    fn test_command_output_clone() {
        let output1 = CommandOutput {
            stdout: "test".to_string(),
            stderr: "error".to_string(),
            exit_code: 0,
        };
        let output2 = output1.clone();
        assert_eq!(output1, output2);
    }

    #[test]
    fn test_command_output_debug() {
        let output = CommandOutput {
            stdout: "test".to_string(),
            stderr: "error".to_string(),
            exit_code: 0,
        };
        let debug_str = format!("{:?}", output);
        assert!(debug_str.contains("stdout"));
        assert!(debug_str.contains("test"));
    }

    #[tokio::test]
    async fn test_execute_with_empty_args() {
        let output = CommandExecutor::execute("pwd", &[]).await.unwrap();
        assert!(output.is_success());
        assert!(!output.stdout.is_empty());
    }
}
