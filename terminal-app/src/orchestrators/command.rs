/// Command execution orchestrator
///
/// This orchestrator is responsible for:
/// - Handling built-in commands (like "clear")
/// - Checking command existence
/// - Executing commands
/// - Formatting command output
use anyhow::Result;

use crate::executor::{CommandExecutor, PackageInstaller};
use crate::terminal::{TerminalState, TerminalUI};
use crate::utils::MessageFormatter;

/// Orchestrates command execution workflow
#[derive(Debug, Default)]
pub struct CommandOrchestrator;

impl CommandOrchestrator {
    /// Create a new command orchestrator
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self
    }

    /// Handle command execution with all the necessary logic
    ///
    /// This method encapsulates:
    /// - Built-in command handling (e.g., "clear")
    /// - Command existence checking
    /// - Command execution
    /// - Output formatting and display
    pub async fn handle_command(
        &self,
        cmd: &str,
        args: &[String],
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()> {
        // Handle special built-in commands that would interfere with TUI
        if cmd == "clear" {
            return self.handle_clear_command(state, ui);
        }

        // Check if command exists
        if !CommandExecutor::command_exists(cmd) {
            self.handle_command_not_found(cmd, state);
            return Ok(());
        }

        // Execute the command
        self.execute_and_display(cmd, args, state).await
    }

    /// Handle the built-in "clear" command
    fn handle_clear_command(&self, state: &mut TerminalState, ui: &mut TerminalUI) -> Result<()> {
        // Clear the output buffer instead of executing the system clear command
        state.output.clear();
        // Force a complete terminal clear to prevent spurious characters
        ui.clear()?;
        Ok(())
    }

    /// Handle command not found scenario
    fn handle_command_not_found(&self, cmd: &str, state: &mut TerminalState) {
        state.add_output(MessageFormatter::command_not_found(cmd));
        state.add_output(MessageFormatter::install_suggestion(
            PackageInstaller::is_available_static(),
        ));
    }

    /// Execute command and display formatted output
    async fn execute_and_display(
        &self,
        cmd: &str,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        match CommandExecutor::execute(cmd, args).await {
            Ok(output) => {
                // Show stdout as-is
                if !output.stdout.is_empty() {
                    for line in output.stdout.lines() {
                        state.add_output(line.to_string());
                    }
                }

                // Show stderr - only colorize red if command failed
                if !output.stderr.is_empty() {
                    for line in output.stderr.lines() {
                        if output.is_success() {
                            // Command succeeded, stderr is just informational
                            state.add_output(line.to_string());
                        } else {
                            // Command failed, highlight stderr in red
                            state.add_output(MessageFormatter::stderr_error(line));
                        }
                    }
                }

                if !output.is_success() {
                    state.add_output(MessageFormatter::command_failed(output.exit_code));
                }
            }
            Err(e) => {
                state.add_output(MessageFormatter::execution_error(e.to_string()));
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::TerminalState;

    #[tokio::test]
    async fn test_command_not_found() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        orchestrator.handle_command_not_found("nonexistent", &mut state);

        assert!(!state.output.lines().is_empty());
        assert!(state.output.lines()[0].contains("nonexistent"));
        assert!(state.output.lines()[0].contains("not found"));
    }

    #[tokio::test]
    async fn test_execute_simple_command() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Execute "echo hello"
        orchestrator
            .execute_and_display("echo", &["hello".to_string()], &mut state)
            .await
            .unwrap();

        // Should have output
        assert!(!state.output.lines().is_empty());
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("hello")));
    }

    #[tokio::test]
    async fn test_execute_command_with_failure() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Execute command that fails
        orchestrator
            .execute_and_display("sh", &["-c".to_string(), "exit 1".to_string()], &mut state)
            .await
            .unwrap();

        // Should have error message about exit code
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("exited")));
    }

    #[tokio::test]
    async fn test_execute_command_with_stderr_success() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Command succeeds but outputs to stderr
        orchestrator
            .execute_and_display(
                "sh",
                &["-c".to_string(), "echo warning >&2".to_string()],
                &mut state,
            )
            .await
            .unwrap();

        // Should have stderr output (not colorized since command succeeded)
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("warning")));
    }

    #[tokio::test]
    async fn test_execute_command_with_stderr_failure() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Command fails and outputs to stderr
        orchestrator
            .execute_and_display(
                "sh",
                &["-c".to_string(), "echo error >&2; exit 1".to_string()],
                &mut state,
            )
            .await
            .unwrap();

        // Should have stderr output (colorized since command failed)
        let output_str = state.output.lines().join("\n");
        assert!(output_str.contains("error"));
    }

    #[test]
    fn test_orchestrator_default() {
        let _ = CommandOrchestrator;
    }

    #[test]
    fn test_orchestrator_debug() {
        let orchestrator = CommandOrchestrator::new();
        let debug_str = format!("{:?}", orchestrator);
        assert!(debug_str.contains("CommandOrchestrator"));
    }

    #[tokio::test]
    async fn test_execute_nonexistent_command() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Try to execute using execute_and_display on nonexistent command
        // This would fail before reaching execute_and_display in real flow,
        // but we test the executor's error handling
        let result = orchestrator
            .execute_and_display("nonexistentcmd123", &[], &mut state)
            .await;

        // Should complete successfully (error is captured in state)
        assert!(result.is_ok());
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Error executing")));
    }

    #[tokio::test]
    async fn test_execute_command_empty_output() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Execute command with no output
        orchestrator
            .execute_and_display("true", &[], &mut state)
            .await
            .unwrap();

        // true command produces no output, state might be empty or have minimal output
        // Just verify it doesn't panic
    }
}
