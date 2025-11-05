/// Command execution module
use anyhow::Result;
use std::process::Stdio;
use tokio::process::Command as TokioCommand;

/// Output from a command execution
#[derive(Debug, Clone)]
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
    /// Execute a command asynchronously
    pub async fn execute(cmd: &str, args: &[String]) -> Result<CommandOutput> {
        // Check if command exists
        if !Self::command_exists(cmd) {
            anyhow::bail!("Command '{}' not found", cmd);
        }

        // Execute the command
        let output = TokioCommand::new(cmd)
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

    /// Check if a command exists in the PATH
    pub fn command_exists(cmd: &str) -> bool {
        which::which(cmd).is_ok()
    }

    /// Get the full path of a command
    pub fn get_command_path(cmd: &str) -> Option<String> {
        which::which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
    }

    /// Execute a command with sudo privileges
    pub async fn execute_sudo(cmd: &str, args: &[String]) -> Result<CommandOutput> {
        let mut sudo_args = vec!["sudo".to_string(), cmd.to_string()];
        sudo_args.extend(args.iter().cloned());

        Self::execute("sudo", &sudo_args).await
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
}
