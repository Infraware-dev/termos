/// Command execution module
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::process::Stdio;
use tokio::process::Command as TokioCommand;
use tokio::time::{timeout, Duration};

use super::job_manager::SharedJobManager;
use crate::input::shell_builtins::ShellBuiltinHandler;

/// Output from a command execution
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
}

impl CommandOutput {
    /// Check if the command was successful
    #[must_use]
    pub const fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Get combined output (stdout + stderr)
    #[must_use]
    #[allow(dead_code)] // Public API for M2/M3
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

/// Commands that require interactive execution (TUI suspension)
const REQUIRES_INTERACTIVE: &[&str] = &[
    // Text editors
    "vim", "nvim", "nano", "emacs", "pico", "ed", "vi", // Pagers
    "less", "more", "most", "man", "info", // File managers
    "mc", "ranger", "nnn", "lf", "vifm",  // Watchers
    "watch", // System monitors (non-root)
    "top", "htop", "btop", "atop", // Privilege escalation (needs password input)
    "sudo", // CLI tools with interactive features
    "gh",
];

/// Commands that are interactive but NOT supported (blocked entirely)
const INTERACTIVE_BLOCKED: &[&str] = &[
    // Remote/session
    "ssh",
    "telnet",
    "ftp",
    "sftp",
    "screen",
    "tmux",
    // REPLs
    "python",
    "python3",
    "irb",
    "node",
    "ipython",
    "mysql",
    "psql",
    "sqlite3",
    "mongo",
    "redis-cli",
    // Debuggers
    "gdb",
    "lldb",
    "pdb",
    // Terminal browsers
    "w3m",
    "lynx",
    "links",
    // Admin tools
    "passwd",
    "visudo",
    // System monitors that require root
    "iotop",
    "iftop",
    "nethogs",
    // Infinite output commands
    "yes", // Produces infinite "y" output - would freeze terminal
];

/// Device paths that produce infinite output
const INFINITE_DEVICES: &[&str] = &["/dev/zero", "/dev/urandom", "/dev/random"];

/// All interactive commands (for `is_interactive_command` check)
static ALL_INTERACTIVE: std::sync::LazyLock<HashSet<&'static str>> =
    std::sync::LazyLock::new(|| {
        REQUIRES_INTERACTIVE
            .iter()
            .chain(INTERACTIVE_BLOCKED.iter())
            .copied()
            .collect()
    });

/// Set of commands that require interactive execution (O(1) lookup)
static REQUIRES_INTERACTIVE_SET: std::sync::LazyLock<HashSet<&'static str>> =
    std::sync::LazyLock::new(|| REQUIRES_INTERACTIVE.iter().copied().collect());

/// Check if command targets an infinite device (e.g., cat /dev/zero)
///
/// For `dd`, allows execution if `count=` is specified (limits output).
fn targets_infinite_device(cmd: &str, args: &[String]) -> bool {
    if cmd == "cat" {
        // cat: block any infinite device argument
        return args
            .iter()
            .any(|arg| INFINITE_DEVICES.iter().any(|dev| arg == *dev));
    }

    if cmd == "dd" {
        // dd: check for infinite input source
        let has_infinite_input = args.iter().any(|arg| {
            INFINITE_DEVICES
                .iter()
                .any(|dev| arg.starts_with("if=") && arg[3..].starts_with(dev))
        });

        // Allow if count= is specified (limits output)
        let has_count_limit = args.iter().any(|arg| arg.starts_with("count="));

        return has_infinite_input && !has_count_limit;
    }

    false
}

/// Check if ping is missing a limiting flag (would run infinitely)
///
/// Supports multiple platforms:
/// - `-c` count (Linux/macOS)
/// - `-n` count (Windows)
/// - `-w` deadline seconds (Linux)
/// - `-W` timeout (macOS)
fn is_infinite_ping(cmd: &str, args: &[String]) -> bool {
    if cmd != "ping" {
        return false;
    }

    // Check for any flag that limits ping duration
    let has_limit = args.iter().any(|a| {
        a == "-c"
            || a.starts_with("-c")
            || a == "-n"
            || a.starts_with("-n")
            || a == "-w"
            || a.starts_with("-w")
            || a == "-W"
            || a.starts_with("-W")
    });

    !has_limit
}

/// Check if a shell command string contains references to infinite devices
///
/// Used to detect bypasses via shell interpretation (e.g., `sh -c "cat /dev/zero"`)
fn shell_command_has_infinite_device(shell_input: &str) -> bool {
    // Check for infinite device references
    let has_infinite_device = INFINITE_DEVICES.iter().any(|dev| shell_input.contains(dev));

    if !has_infinite_device {
        return false;
    }

    // Allow if output is piped to a limiting command
    let has_output_limit = shell_input.contains("| head")
        || shell_input.contains("|head")
        || shell_input.contains("| tail")
        || shell_input.contains("|tail")
        || shell_input.contains("count=");

    has_infinite_device && !has_output_limit
}

/// Command executor for running shell commands
#[derive(Debug)]
pub struct CommandExecutor;

impl CommandExecutor {
    /// Check if a command is interactive and should be blocked
    fn is_interactive_command(cmd: &str) -> bool {
        ALL_INTERACTIVE.contains(cmd)
    }

    /// Check if a command requires interactive execution with TUI suspension
    ///
    /// These commands need a real TTY and will be executed with the TUI suspended.
    ///
    /// # Platform Support
    /// - **Unix/Linux/macOS**: Fully supported via TUI suspension
    /// - **Windows**: Returns true but execution will fail with error message
    pub fn requires_interactive(cmd: &str) -> bool {
        REQUIRES_INTERACTIVE_SET.contains(cmd)
    }

    /// Check if a command is a shell builtin that must be executed through a shell
    fn is_shell_builtin(cmd: &str) -> bool {
        ShellBuiltinHandler::requires_shell_execution(cmd)
    }

    /// Get the platform-appropriate shell and shell command flag
    ///
    /// Returns a tuple of (`shell_executable`, `command_flag`):
    /// - Unix/Linux/macOS: ("bash", "-c") - bash supports brace expansion {1..3}
    /// - Windows: ("cmd", "/C")
    const fn get_platform_shell() -> (&'static str, &'static str) {
        #[cfg(target_os = "windows")]
        {
            ("cmd", "/C")
        }
        #[cfg(not(target_os = "windows"))]
        {
            ("bash", "-c")
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
        log::debug!("Executing command: {} {:?}", cmd, args);

        // Block interactive commands that are NOT supported via TUI suspension
        // Commands in requires_interactive() will be handled separately
        if Self::is_interactive_command(cmd) && !Self::requires_interactive(cmd) {
            log::warn!("Blocked interactive command: {}", cmd);
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Interactive command '{cmd}' is not supported in this terminal.\n\
                     Suggestions:\n\
                     - For 'top': use 'ps aux' or 'top -b -n 1' for batch mode\n\
                     - For 'ssh/tmux/screen': use in a separate terminal window\n\
                     - For REPLs: pass code as argument (e.g., 'python -c \"print(1+1)\"')"
                ),
                exit_code: 1,
            });
        }

        // Block commands targeting infinite devices (e.g., cat /dev/zero)
        if targets_infinite_device(cmd, args) {
            log::warn!("Blocked infinite device command: {} {:?}", cmd, args);
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Command '{} {}' is blocked.\n\n\
                     Reason: Reading from infinite device would freeze the terminal.\n\n\
                     Suggestion: Use 'head' to limit output, e.g., '{} {} | head -c 1000'",
                    cmd,
                    args.join(" "),
                    cmd,
                    args.join(" ")
                ),
                exit_code: 1,
            });
        }

        // Block ping without count flag (would run infinitely)
        if is_infinite_ping(cmd, args) {
            log::warn!("Blocked infinite ping: {} {:?}", cmd, args);
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Command 'ping' without count limit is blocked.\n\n\
                     Reason: Infinite ping would freeze the terminal.\n\n\
                     Suggestion: Use '-c N' to limit, e.g., 'ping -c 4 {}'",
                    args.first().map_or("host", String::as_str)
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

            // CRITICAL: Check for infinite device bypass in shell builtin arguments
            if shell_command_has_infinite_device(&full_command) {
                log::warn!(
                    "Blocked shell builtin with infinite device: {}",
                    full_command
                );
                return Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: "Command blocked: contains reference to infinite device.\n\n\
                             Reason: Reading from /dev/zero, /dev/urandom, or /dev/random would freeze the terminal.\n\n\
                             Suggestion: Pipe output through 'head' to limit, e.g., 'cat /dev/urandom | head -c 100'"
                        .to_string(),
                    exit_code: 1,
                });
            }

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
            // CRITICAL: Check for infinite device bypass in shell command
            if shell_command_has_infinite_device(shell_input) {
                log::warn!(
                    "Blocked shell command with infinite device: {}",
                    shell_input
                );
                return Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: "Command blocked: contains reference to infinite device.\n\n\
                             Reason: Reading from /dev/zero, /dev/urandom, or /dev/random would freeze the terminal.\n\n\
                             Suggestion: Pipe output through 'head' to limit, e.g., 'cat /dev/urandom | head -c 100'"
                        .to_string(),
                    exit_code: 1,
                });
            }

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
            log::error!("Command not found: {}", cmd);
            anyhow::bail!("Command '{cmd}' not found");
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

    /// Execute interactive command with TUI suspension
    ///
    /// # Arguments
    /// * `cmd` - Command to execute
    /// * `args` - Command arguments
    /// * `ui` - TUI instance (will be suspended/resumed)
    ///
    /// # Returns
    /// `CommandOutput` with exit code (stdout/stderr not captured)
    ///
    /// # Unix-Only
    /// This method is Unix-only (Linux/macOS). On Windows, it returns an error.
    pub async fn execute_interactive(
        cmd: &str,
        args: &[String],
        ui: &mut crate::terminal::TerminalUI,
    ) -> Result<CommandOutput> {
        #[cfg(target_os = "windows")]
        {
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Interactive command '{}' is not supported on Windows.\n\
                     Interactive commands are only available on Linux and macOS.",
                    cmd
                ),
                exit_code: 1,
            });
        }

        #[cfg(not(target_os = "windows"))]
        {
            // RAII guard that ensures resume() is called even on panic
            struct TuiGuard<'a> {
                ui: &'a mut crate::terminal::TerminalUI,
                suspended: bool,
            }

            impl Drop for TuiGuard<'_> {
                fn drop(&mut self) {
                    if self.suspended {
                        // Best-effort resume, ignore errors in panic path
                        let _ = self.ui.resume();
                    }
                }
            }

            // Suspend TUI
            ui.suspend().context("Failed to suspend TUI")?;
            let mut guard = TuiGuard {
                ui,
                suspended: true,
            };

            // Clone for move into spawn_blocking
            let cmd = cmd.to_string();
            let args = args.to_vec();

            // Run blocking command on dedicated thread pool
            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new(&cmd).args(&args).status()
            })
            .await
            .context("Interactive command task panicked")?;

            // Resume TUI (guard ensures this happens even on panic)
            guard.ui.resume().context("Failed to resume TUI")?;
            guard.suspended = false; // Mark as successfully resumed

            // Return result
            match result {
                Ok(exit_status) => Ok(CommandOutput {
                    stdout: String::new(), // Not captured
                    stderr: String::new(),
                    exit_code: exit_status.code().unwrap_or(-1),
                }),
                Err(e) => Err(anyhow::anyhow!("Command failed: {e}")),
            }
        }
    }

    /// Check if a command exists in the PATH
    #[must_use]
    pub fn command_exists(cmd: &str) -> bool {
        which::which(cmd).is_ok()
    }

    /// Get the full path of a command
    #[must_use]
    #[allow(dead_code)] // Public API for M2/M3
    pub fn get_command_path(cmd: &str) -> Option<String> {
        which::which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
    }

    /// Execute a command with sudo privileges (M2/M3)
    #[allow(dead_code)] // Used by package manager implementations for privileged operations
    pub async fn execute_sudo(cmd: &str, args: &[String]) -> Result<CommandOutput> {
        // Check if command exists
        if !Self::command_exists(cmd) {
            anyhow::bail!("Command '{cmd}' not found");
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

    /// Check if a command should be executed in the background
    ///
    /// Returns true if the command ends with `&` but NOT `&&`.
    /// Also checks that the `&` is not inside quotes.
    ///
    /// # Examples
    /// ```
    /// use infraware_terminal::executor::CommandExecutor;
    ///
    /// assert!(CommandExecutor::is_background_command("sleep 10 &"));
    /// assert!(CommandExecutor::is_background_command("echo hello &"));
    /// assert!(!CommandExecutor::is_background_command("cmd1 && cmd2"));
    /// assert!(!CommandExecutor::is_background_command("echo hello"));
    /// assert!(!CommandExecutor::is_background_command("echo \"a & b\""));
    /// ```
    #[must_use]
    pub fn is_background_command(input: &str) -> bool {
        let trimmed = input.trim();

        // Must end with & but not &&
        if !trimmed.ends_with('&') || trimmed.ends_with("&&") {
            return false;
        }

        // Check that the trailing & is not inside quotes and not escaped
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut last_was_backslash = false;
        let chars: Vec<char> = trimmed.chars().collect();

        for (i, &c) in chars.iter().enumerate() {
            let is_last = i == chars.len() - 1;

            if last_was_backslash {
                // This character is escaped
                if is_last && c == '&' {
                    // The trailing & is escaped
                    return false;
                }
                last_was_backslash = false;
                continue;
            }

            match c {
                '\\' if !in_single_quote => {
                    // Backslash escapes next char (unless in single quotes)
                    last_was_backslash = true;
                }
                '\'' if !in_double_quote => {
                    in_single_quote = !in_single_quote;
                }
                '"' if !in_single_quote => {
                    in_double_quote = !in_double_quote;
                }
                _ => {}
            }
        }

        // If we're still inside quotes at the end, the & is quoted
        !in_single_quote && !in_double_quote
    }

    /// Execute a command in the background without waiting
    ///
    /// Spawns the process and immediately returns, adding it to the job manager.
    /// The job manager tracks the process and can report when it completes.
    ///
    /// # Arguments
    /// * `command` - The full command string (including the trailing &)
    /// * `job_manager` - Shared job manager for tracking
    ///
    /// # Returns
    /// Tuple of (job_id, pid) on success
    pub async fn execute_background(
        command: &str,
        job_manager: &SharedJobManager,
    ) -> Result<(usize, u32)> {
        let (shell, flag) = Self::get_platform_shell();

        log::info!("Spawning background command: {}", command);

        let child = TokioCommand::new(shell)
            .arg(flag)
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::null()) // Don't capture output for background
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn background command")?;

        let pid = child.id().unwrap_or(0);

        let job_id = {
            let mut mgr = job_manager
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            mgr.add_job(command.to_string(), pid, child)
        };

        log::info!("Background job [{}] started with PID {}", job_id, pid);

        Ok((job_id, pid))
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
    async fn test_unsupported_interactive_command_blocked() {
        // Test that unsupported interactive commands (not in requires_interactive) are blocked
        // ssh is in INTERACTIVE_COMMANDS but NOT in requires_interactive
        let output = CommandExecutor::execute("ssh", &[], None).await.unwrap();
        assert!(!output.is_success());
        assert_eq!(output.exit_code, 1);
        assert!(output.stderr.contains("Interactive command"));
        assert!(output.stderr.contains("not supported"));
    }

    #[tokio::test]
    async fn test_python_command_blocked() {
        // python is in INTERACTIVE_BLOCKED, not in requires_interactive
        let output = CommandExecutor::execute("python", &[], None).await.unwrap();
        assert!(!output.is_success());
        assert!(output.stderr.contains("Interactive command"));
    }

    #[test]
    fn test_requires_interactive() {
        // Text editors
        assert!(CommandExecutor::requires_interactive("vim"));
        assert!(CommandExecutor::requires_interactive("nano"));

        // Pagers
        assert!(CommandExecutor::requires_interactive("less"));
        assert!(CommandExecutor::requires_interactive("man"));

        // File managers
        assert!(CommandExecutor::requires_interactive("mc"));

        // System monitors
        assert!(CommandExecutor::requires_interactive("top"));
        assert!(CommandExecutor::requires_interactive("htop"));
        assert!(CommandExecutor::requires_interactive("atop"));

        // System monitors that require root (blocked)
        assert!(!CommandExecutor::requires_interactive("iotop"));
        assert!(!CommandExecutor::requires_interactive("iftop"));
        assert!(!CommandExecutor::requires_interactive("nethogs"));

        // Package managers are NOT interactive (output captured for scrolling)
        assert!(!CommandExecutor::requires_interactive("apt"));
        assert!(!CommandExecutor::requires_interactive("apt-get"));
        assert!(!CommandExecutor::requires_interactive("yum"));
        assert!(!CommandExecutor::requires_interactive("dnf"));
        assert!(!CommandExecutor::requires_interactive("pacman"));

        // Privilege escalation requires interactive (password prompt)
        assert!(CommandExecutor::requires_interactive("sudo"));

        // Test that blocked commands return false
        assert!(!CommandExecutor::requires_interactive("ssh"));
        assert!(!CommandExecutor::requires_interactive("python"));
        assert!(!CommandExecutor::requires_interactive("ls"));
    }

    #[test]
    fn test_is_interactive_command() {
        // Supported interactive
        assert!(CommandExecutor::is_interactive_command("vim"));
        assert!(CommandExecutor::is_interactive_command("top"));
        assert!(CommandExecutor::is_interactive_command("nano"));
        assert!(CommandExecutor::is_interactive_command("htop"));
        assert!(CommandExecutor::is_interactive_command("less"));
        assert!(CommandExecutor::is_interactive_command("sudo"));

        // Blocked interactive
        assert!(CommandExecutor::is_interactive_command("ssh"));
        assert!(CommandExecutor::is_interactive_command("python"));
        assert!(CommandExecutor::is_interactive_command("iotop"));
        assert!(CommandExecutor::is_interactive_command("iftop"));

        // Non-interactive (including package managers)
        assert!(!CommandExecutor::is_interactive_command("ls"));
        assert!(!CommandExecutor::is_interactive_command("ps"));
        assert!(!CommandExecutor::is_interactive_command("cat"));
        assert!(!CommandExecutor::is_interactive_command("apt"));
        assert!(!CommandExecutor::is_interactive_command("yum"));
        assert!(!CommandExecutor::is_interactive_command("dnf"));
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
        let debug_str = format!("{output:?}");
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

    // ========== Background Command Detection Tests ==========

    #[test]
    fn test_is_background_command_simple() {
        assert!(CommandExecutor::is_background_command("sleep 10 &"));
        assert!(CommandExecutor::is_background_command("echo hello &"));
        assert!(CommandExecutor::is_background_command("  sleep 5 &  ")); // with whitespace
    }

    #[test]
    fn test_is_background_command_not_double_ampersand() {
        assert!(!CommandExecutor::is_background_command("cmd1 && cmd2"));
        assert!(!CommandExecutor::is_background_command("echo a && echo b"));
    }

    #[test]
    fn test_is_background_command_no_ampersand() {
        assert!(!CommandExecutor::is_background_command("echo hello"));
        assert!(!CommandExecutor::is_background_command("ls -la"));
    }

    #[test]
    fn test_is_background_command_ampersand_in_quotes() {
        // Ampersand inside quotes is NOT a background operator
        assert!(!CommandExecutor::is_background_command("echo \"a & b\""));
        assert!(!CommandExecutor::is_background_command(
            "echo 'run in background &'"
        ));
    }

    #[test]
    fn test_is_background_command_escaped_ampersand() {
        // Escaped ampersand is NOT a background operator
        assert!(!CommandExecutor::is_background_command("echo hello \\&"));
    }

    #[test]
    fn test_is_background_command_complex() {
        // Multiple commands with final background
        assert!(CommandExecutor::is_background_command("cmd1; cmd2 &"));
        // Pipe with background
        assert!(CommandExecutor::is_background_command(
            "cat file | grep pattern &"
        ));
    }
}
