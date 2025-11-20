/// Command execution module
use anyhow::{Context, Result};
use std::process::Stdio;
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, Duration};

use crate::input::shell_builtins::ShellBuiltinHandler;

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
    /// List of interactive commands that should not be executed in non-interactive mode
    /// These commands require a TTY and will block or behave incorrectly without one
    const INTERACTIVE_COMMANDS: &'static [&'static str] = &[
        // Text editors
        "vi",
        "vim",
        "nvim",
        "emacs",
        "nano",
        "pico",
        "ed",
        // Interactive system monitors
        "top",
        "htop",
        "btop",
        "atop",
        "iotop",
        "iftop",
        "nethogs",
        "watch", // Runs commands repeatedly
        // Interactive file managers
        "mc",
        "ranger",
        "nnn",
        "lf",
        "vifm",
        // Interactive programs
        "less",
        "more",
        "most",
        "man",
        "info",
        "ssh",
        "telnet",
        "ftp",
        "sftp",
        "screen",
        "tmux",
        "python",
        "python3",
        "irb",
        "node",
        "ipython", // REPLs without args
        "mysql",
        "psql",
        "sqlite3",
        "mongo",
        "redis-cli",
        // Other interactive tools
        "gdb",
        "lldb",
        "pdb",
        "w3m",
        "lynx",
        "links",
        // System administration
        "passwd",
        "visudo",
        // Note: sh, bash, zsh, fish are NOT blocked when used with -c flag
        // They are already used internally for shell operator interpretation
    ];

    /// Check if a command is interactive and should be blocked
    fn is_interactive_command(cmd: &str) -> bool {
        Self::INTERACTIVE_COMMANDS.contains(&cmd)
    }

    /// Check if a command is a shell builtin that must be executed through a shell
    fn is_shell_builtin(cmd: &str) -> bool {
        ShellBuiltinHandler::requires_shell_execution(cmd)
    }

    /// Get the platform-appropriate shell and shell command flag
    ///
    /// Returns a tuple of (shell_executable, command_flag):
    /// - Unix/Linux/macOS: ("sh", "-c")
    /// - Windows: ("cmd", "/C")
    fn get_platform_shell() -> (&'static str, &'static str) {
        #[cfg(target_os = "windows")]
        {
            ("cmd", "/C")
        }
        #[cfg(not(target_os = "windows"))]
        {
            ("sh", "-c")
        }
    }

    /// Execute a command asynchronously with a 5-minute timeout
    ///
    /// # Arguments
    /// * `cmd` - The command name
    /// * `args` - The command arguments
    /// * `original_input` - Optional original input string. When present, the command
    ///   contains shell operators (pipes, redirects, etc.) and will be executed via
    ///   `sh -c` for proper shell interpretation.
    ///
    /// # Shell Interpretation
    /// If `original_input` is provided, the entire command is passed to `sh -c` for
    /// proper shell operator handling (pipes, redirects, subshells, etc.).
    /// Otherwise, the command is executed directly for better security and performance.
    ///
    /// # Interactive Commands
    /// Interactive commands (vim, top, etc.) are blocked and return an error message.
    /// Use non-interactive alternatives or specific flags instead.
    pub async fn execute(
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
    ) -> Result<CommandOutput> {
        // Block interactive commands
        if Self::is_interactive_command(cmd) {
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Interactive command '{}' is not supported in this terminal.\n\
                     Suggestions:\n\
                     - For 'top': use 'ps aux' or 'top -b -n 1' for batch mode\n\
                     - For 'vim/nano': use 'cat' to view files, or edit externally\n\
                     - For 'less': use 'cat' or 'head/tail'\n\
                     - For shells: commands are already executed in a shell\n\
                     - For REPLs: pass code as argument (e.g., 'python -c \"print(1+1)\"')",
                    cmd
                ),
                exit_code: 1,
            });
        }

        // If command is a shell builtin, execute through shell
        // Builtins like '.', ':', '[[', 'source', 'export' don't exist as standalone executables
        if Self::is_shell_builtin(cmd) {
            // Check if this is a Unix-only builtin on Windows
            #[cfg(target_os = "windows")]
            {
                if ShellBuiltinHandler::is_unix_only(cmd) {
                    return Ok(CommandOutput {
                        stdout: String::new(),
                        stderr: format!(
                            "Shell builtin '{}' is not available on Windows.\n\
                             This is a Unix/Linux shell builtin that requires bash or sh.",
                            cmd
                        ),
                        exit_code: 1,
                    });
                }
            }

            // Reconstruct the full command from cmd + args
            let full_command = if args.is_empty() {
                cmd.to_string()
            } else {
                format!("{} {}", cmd, args.join(" "))
            };

            let (shell, shell_flag) = Self::get_platform_shell();
            let execution = TokioCommand::new(shell)
                .arg(shell_flag)
                .arg(&full_command)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            let output = timeout(Duration::from_secs(300), execution)
                .await
                .context("Command execution timed out after 5 minutes")??;

            return Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            });
        }

        // If original_input is provided, use platform shell for shell operator interpretation
        if let Some(shell_input) = original_input {
            let (shell, shell_flag) = Self::get_platform_shell();
            let execution = TokioCommand::new(shell)
                .arg(shell_flag)
                .arg(shell_input)
                .stdin(Stdio::null()) // Prevent interactive programs from blocking
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output();

            let output = timeout(Duration::from_secs(300), execution)
                .await
                .context("Command execution timed out after 5 minutes")??;

            return Ok(CommandOutput {
                stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                exit_code: output.status.code().unwrap_or(-1),
            });
        }

        // Direct execution (no shell operators)
        // Check if command exists
        if !Self::command_exists(cmd) {
            anyhow::bail!("Command '{}' not found", cmd);
        }

        // Execute the command with timeout
        let execution = TokioCommand::new(cmd)
            .args(args)
            .stdin(Stdio::null()) // Prevent interactive programs from blocking
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
    #[allow(dead_code)] // Used by package_manager.rs
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
        let output = CommandExecutor::execute("echo", &["hello".to_string()], None)
            .await
            .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_command_not_found() {
        let result = CommandExecutor::execute("nonexistentcommand123", &[], None).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_command_exists() {
        assert!(CommandExecutor::command_exists("echo"));
        assert!(!CommandExecutor::command_exists("nonexistentcommand123"));
    }

    #[tokio::test]
    async fn test_command_with_multiple_args() {
        let output =
            CommandExecutor::execute("echo", &["hello".to_string(), "world".to_string()], None)
                .await
                .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello world");
    }

    #[tokio::test]
    async fn test_command_with_stderr() {
        // Use a command that outputs to stderr (grep with no match)
        let output = CommandExecutor::execute(
            "sh",
            &["-c".to_string(), "echo error >&2".to_string()],
            None,
        )
        .await
        .unwrap();
        assert!(output.is_success());
        assert!(output.stderr.contains("error"));
    }

    #[tokio::test]
    async fn test_command_exit_code() {
        // Use false command which exits with code 1
        let output =
            CommandExecutor::execute("sh", &["-c".to_string(), "exit 42".to_string()], None)
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

    #[tokio::test]
    async fn test_interactive_command_blocked() {
        // Test that interactive commands are blocked
        let output = CommandExecutor::execute("vim", &[], None).await.unwrap();
        assert!(!output.is_success());
        assert_eq!(output.exit_code, 1);
        assert!(output.stderr.contains("Interactive command"));
        assert!(output.stderr.contains("not supported"));
    }

    #[tokio::test]
    async fn test_top_command_blocked() {
        let output = CommandExecutor::execute("top", &[], None).await.unwrap();
        assert!(!output.is_success());
        assert!(output.stderr.contains("Interactive command"));
        assert!(output.stderr.contains("top -b -n 1"));
    }

    #[tokio::test]
    async fn test_nano_command_blocked() {
        let output = CommandExecutor::execute("nano", &[], None).await.unwrap();
        assert!(!output.is_success());
        assert!(output.stderr.contains("Interactive command"));
    }

    #[test]
    fn test_is_interactive_command() {
        assert!(CommandExecutor::is_interactive_command("vim"));
        assert!(CommandExecutor::is_interactive_command("top"));
        assert!(CommandExecutor::is_interactive_command("nano"));
        assert!(CommandExecutor::is_interactive_command("htop"));
        assert!(CommandExecutor::is_interactive_command("less"));
        assert!(!CommandExecutor::is_interactive_command("ls"));
        assert!(!CommandExecutor::is_interactive_command("ps"));
        assert!(!CommandExecutor::is_interactive_command("cat"));
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
        let output = CommandExecutor::execute("pwd", &[], None).await.unwrap();
        assert!(output.is_success());
        assert!(!output.stdout.is_empty());
    }

    #[tokio::test]
    async fn test_pipe_execution() {
        // Test pipe execution via original_input
        let output = CommandExecutor::execute(
            "echo",
            &["hello".to_string()],
            Some("echo hello | grep hello"),
        )
        .await
        .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "hello");
    }

    #[tokio::test]
    async fn test_pipe_with_multiple_commands() {
        // Test multiple pipes
        let output = CommandExecutor::execute(
            "echo",
            &[],
            Some("echo 'line1\nline2\nline3' | grep line2 | wc -l"),
        )
        .await
        .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "1");
    }

    #[tokio::test]
    async fn test_redirect_execution() {
        // Test redirect via original_input
        // Create temp file, write to it, read it back
        let output = CommandExecutor::execute(
            "echo",
            &[],
            Some("echo test > /tmp/test_redirect.txt && cat /tmp/test_redirect.txt && rm /tmp/test_redirect.txt"),
        )
        .await
        .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "test");
    }

    #[tokio::test]
    async fn test_logical_and_operator() {
        // Test && operator
        let output = CommandExecutor::execute("echo", &[], Some("echo first && echo second"))
            .await
            .unwrap();
        assert!(output.is_success());
        assert!(output.stdout.contains("first"));
        assert!(output.stdout.contains("second"));
    }

    #[tokio::test]
    async fn test_subshell_execution() {
        // Test subshell via $()
        let output = CommandExecutor::execute("echo", &[], Some("echo $(echo nested)"))
            .await
            .unwrap();
        assert!(output.is_success());
        assert_eq!(output.stdout.trim(), "nested");
    }
}
