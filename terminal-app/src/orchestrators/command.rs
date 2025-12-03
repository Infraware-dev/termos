/// Command execution orchestrator
///
/// This orchestrator is responsible for:
/// - Handling built-in commands (like "clear", "jobs")
/// - Checking command existence
/// - Executing commands (foreground and background)
/// - Formatting command output
use anyhow::Result;

use crate::executor::command::CommandOutput;
use crate::executor::{CommandExecutor, JobInfo, JobStatus, PackageInstaller, SharedJobManager};
use crate::input::shell_builtins::ShellBuiltinHandler;
use crate::terminal::{TerminalState, TerminalUI};
use crate::utils::MessageFormatter;

/// Orchestrates command execution workflow
#[derive(Debug, Default)]
pub struct CommandOrchestrator;

impl CommandOrchestrator {
    /// Create a new command orchestrator
    #[allow(dead_code)] // Constructor used in tests, Default trait is preferred
    pub const fn new() -> Self {
        Self
    }

    /// Handle command execution with all the necessary logic
    ///
    /// This method encapsulates:
    /// - Built-in command handling (e.g., "clear", "jobs")
    /// - Background command handling (commands ending with &)
    /// - Command existence checking
    /// - Command execution
    /// - Output formatting and display
    ///
    /// # Arguments
    /// * `cmd` - The command name
    /// * `args` - The command arguments
    /// * `original_input` - Optional original input string (for shell operators like pipes)
    /// * `state` - Terminal state
    /// * `ui` - Terminal UI
    /// * `job_manager` - Shared job manager for background processes
    pub async fn handle_command(
        &self,
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
        job_manager: &SharedJobManager,
    ) -> Result<()> {
        // Handle special built-in commands that would interfere with TUI
        if cmd == "clear" {
            return self.handle_clear_command(state, ui);
        }

        // Handle reload-aliases built-in command
        if cmd == "reload-aliases" {
            return self.handle_reload_aliases_command(state).await;
        }

        // Handle reload-commands built-in command
        if cmd == "reload-commands" {
            return self.handle_reload_commands_command(state);
        }

        // Handle jobs built-in command
        if cmd == "jobs" {
            return self.handle_jobs_command(state, job_manager);
        }

        // Check for background command (ends with &)
        if let Some(input) = original_input {
            if CommandExecutor::is_background_command(input) {
                return self
                    .execute_background_and_display(input, state, job_manager)
                    .await;
            }
        }

        // Check if command exists BEFORE trying any execution
        // (skip check if using shell interpretation, shell builtin, or history expansion)
        // Shell builtins don't exist in PATH but are valid commands that must be executed via shell
        // History expansions (!!, !$, etc.) should have been expanded by HistoryExpansionHandler
        let is_history_expansion = cmd.starts_with('!');

        if original_input.is_none()
            && !ShellBuiltinHandler::requires_shell_execution(cmd)
            && !is_history_expansion
            && !CommandExecutor::command_exists(cmd)
        {
            self.handle_command_not_found(cmd, state);
            return Ok(());
        }

        // Check if command requires interactive execution (command exists at this point)
        if CommandExecutor::requires_interactive(cmd) {
            return self
                .execute_interactive_and_display(cmd, args, state, ui)
                .await;
        }

        // Execute the command
        self.execute_and_display(cmd, args, original_input, state)
            .await
    }

    /// Handle the built-in "clear" command
    fn handle_clear_command(&self, state: &mut TerminalState, ui: &mut TerminalUI) -> Result<()> {
        // Clear the output buffer instead of executing the system clear command
        state.output.clear();
        // Force a complete terminal clear to prevent spurious characters
        ui.clear()?;
        Ok(())
    }

    /// Handle the built-in "reload-aliases" command
    ///
    /// Reloads system aliases from /etc/bash.bashrc, /etc/bashrc, etc.
    /// Uses spawn_blocking to avoid blocking the async executor during file I/O.
    async fn handle_reload_aliases_command(&self, state: &mut TerminalState) -> Result<()> {
        use crate::input::discovery::CommandCache;

        state.add_output(MessageFormatter::info("Reloading system aliases..."));

        // Spawn blocking task to avoid blocking the async executor
        // File I/O is blocking, so we use spawn_blocking as recommended by Tokio
        let result = tokio::task::spawn_blocking(CommandCache::load_system_aliases).await;

        match result {
            Ok(Ok(())) => {
                state.add_output(MessageFormatter::success(
                    "System aliases reloaded successfully",
                ));
            }
            Ok(Err(e)) => {
                state.add_output(MessageFormatter::error(format!(
                    "Failed to reload aliases: {e}"
                )));
            }
            Err(e) => {
                state.add_output(MessageFormatter::error(format!("Task panicked: {e}")));
            }
        }

        Ok(())
    }

    /// Handle the built-in "reload-commands" command
    ///
    /// Clears the command cache (available/unavailable) so that newly installed
    /// commands will be discovered on next use. Aliases are preserved.
    fn handle_reload_commands_command(&self, state: &mut TerminalState) -> Result<()> {
        use crate::input::discovery::CommandCache;

        state.add_output(MessageFormatter::info("Clearing command cache..."));

        CommandCache::clear_commands();

        state.add_output(MessageFormatter::success(
            "Command cache cleared. New commands will be discovered on next use.",
        ));

        Ok(())
    }

    /// Handle the built-in "jobs" command
    ///
    /// Lists all background jobs with their status.
    fn handle_jobs_command(
        &self,
        state: &mut TerminalState,
        job_manager: &SharedJobManager,
    ) -> Result<()> {
        let jobs: Vec<JobInfo> = {
            let mgr = job_manager
                .read()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            mgr.list_jobs()
        };

        if jobs.is_empty() {
            state.add_output(MessageFormatter::info("No background jobs"));
        } else {
            for job in jobs {
                let status_str = match job.status {
                    JobStatus::Running => "Running".to_string(),
                    JobStatus::Done(code) => format!("Done (exit: {})", code),
                    JobStatus::Terminated => "Terminated".to_string(),
                };
                state.add_output(format!(
                    "[{}] {} {} (PID: {})",
                    job.id, status_str, job.command, job.pid
                ));
            }
        }

        Ok(())
    }

    /// Execute a command in the background and display status
    async fn execute_background_and_display(
        &self,
        command: &str,
        state: &mut TerminalState,
        job_manager: &SharedJobManager,
    ) -> Result<()> {
        match CommandExecutor::execute_background(command, job_manager).await {
            Ok((job_id, pid)) => {
                state.add_output(format!("[{}] {} (PID: {})", job_id, command.trim(), pid));
            }
            Err(e) => {
                state.add_output(MessageFormatter::execution_error(e.to_string()));
            }
        }

        Ok(())
    }

    /// Handle command not found scenario
    fn handle_command_not_found(&self, cmd: &str, state: &mut TerminalState) {
        log::warn!("Command not found: {}", cmd);
        state.add_output(MessageFormatter::command_not_found(cmd));
        state.add_output(MessageFormatter::install_suggestion(
            PackageInstaller::is_available_static(),
        ));
    }

    /// Execute interactive command with TUI suspension and display result
    async fn execute_interactive_and_display(
        &self,
        cmd: &str,
        args: &[String],
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()> {
        match CommandExecutor::execute_interactive(cmd, args, ui).await {
            Ok(output) => {
                // Interactive commands don't capture stdout/stderr
                // Just show completion message with exit code
                if output.is_success() {
                    state.add_output(format!(
                        "Interactive command '{cmd}' completed successfully"
                    ));
                } else {
                    state.add_output(MessageFormatter::command_failed(output.exit_code));
                }
            }
            Err(e) => {
                state.add_output(MessageFormatter::execution_error(e.to_string()));
            }
        }

        Ok(())
    }

    /// Execute command and display formatted output
    async fn execute_and_display(
        &self,
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
        state: &mut TerminalState,
    ) -> Result<()> {
        match CommandExecutor::execute(cmd, args, original_input).await {
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

                // Only show "Command failed" for truly problematic exit codes
                // Exit code 1 is often used semantically (grep no match, diff found differences)
                // so we suppress the error message if the command produced output
                if !output.is_success() && !self.is_benign_failure(&output) {
                    state.add_output(MessageFormatter::command_failed(output.exit_code));
                }
            }
            Err(e) => {
                state.add_output(MessageFormatter::execution_error(e.to_string()));
            }
        }

        Ok(())
    }

    /// Check if a non-zero exit code is likely benign (semantic result, not error)
    ///
    /// Commands like grep, diff, test use exit code 1 to indicate semantic results:
    /// - grep: no matches found (exit 1, no output)
    /// - diff: files differ (exit 1, with differences)
    /// - test/[: condition false (exit 1, no output)
    ///
    /// Exit code 1 is commonly used for semantic results rather than errors.
    /// Exit code 2+ usually indicates actual errors (syntax error, file not found, etc.)
    const fn is_benign_failure(&self, output: &CommandOutput) -> bool {
        // Exit code 1 is often semantic (grep no match, diff differences, test false)
        // Exit code 2+ usually indicates real errors
        output.exit_code == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            .execute_and_display("echo", &["hello".to_string()], None, &mut state)
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

        // Execute command that fails with exit 1 (benign failure)
        orchestrator
            .execute_and_display(
                "sh",
                &["-c".to_string(), "exit 1".to_string()],
                None,
                &mut state,
            )
            .await
            .unwrap();

        // Exit 1 is benign, should NOT show "exited with code" message
        assert!(!state
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
                None,
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
                None,
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
        let debug_str = format!("{orchestrator:?}");
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
            .execute_and_display("nonexistentcmd123", &[], None, &mut state)
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
            .execute_and_display("true", &[], None, &mut state)
            .await
            .unwrap();

        // true command produces no output, state might be empty or have minimal output
        // Just verify it doesn't panic
    }

    #[tokio::test]
    async fn test_grep_no_match_exit_1_benign() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // grep with no match returns exit 1 (benign, not an error)
        orchestrator
            .execute_and_display(
                "sh",
                &[],
                Some("echo 'hello world' | grep 'nonexistent'"),
                &mut state,
            )
            .await
            .unwrap();

        // Should NOT show "Command exited with code 1" message
        // because exit 1 is benign for grep
        let output_str = state.output.lines().join("\n");
        assert!(!output_str.contains("exited with code"));
    }

    #[tokio::test]
    async fn test_exit_code_2_shows_error() {
        let orchestrator = CommandOrchestrator::new();
        let mut state = TerminalState::new();

        // Exit code 2 should show error message (real error)
        orchestrator
            .execute_and_display(
                "sh",
                &["-c".to_string(), "exit 2".to_string()],
                None,
                &mut state,
            )
            .await
            .unwrap();

        // Should show "Command exited with code 2" message
        let output_str = state.output.lines().join("\n");
        assert!(output_str.contains("exited with code 2"));
    }

    #[test]
    fn test_is_benign_failure() {
        let orchestrator = CommandOrchestrator::new();

        // Exit code 1 is benign
        let benign = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 1,
        };
        assert!(orchestrator.is_benign_failure(&benign));

        // Exit code 0 is success (not a failure at all)
        let success = CommandOutput {
            stdout: "output".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        assert!(!orchestrator.is_benign_failure(&success));

        // Exit code 2+ is a real error
        let error = CommandOutput {
            stdout: String::new(),
            stderr: "error".to_string(),
            exit_code: 2,
        };
        assert!(!orchestrator.is_benign_failure(&error));

        // Exit code 127 (command not found) is a real error
        let not_found = CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 127,
        };
        assert!(!orchestrator.is_benign_failure(&not_found));
    }
}
