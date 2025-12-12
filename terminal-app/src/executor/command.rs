/// Command execution module
use anyhow::{Context, Result};
use std::collections::HashSet;
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::mpsc;
use tokio::time::{timeout, Duration};
use tokio_util::sync::CancellationToken;

use super::job_manager::SharedJobManager;

/// Command execution timeout in seconds (5 minutes).
///
/// Rationale: Long enough for package installations and large builds,
/// short enough to prevent hung processes from blocking the terminal indefinitely.
/// Based on typical CI/CD timeout values.
///
/// Side effects of changing:
/// - Too low: Package managers (`apt install`, `cargo build`) may timeout prematurely
/// - Too high: Hung processes won't be killed promptly, blocking user interaction
const COMMAND_TIMEOUT_SECS: u64 = 300;

/// Shorter timeout for non-whitelisted commands (30 seconds).
///
/// Commands not in UNLIMITED_OUTPUT_COMMANDS get this shorter timeout to prevent
/// slow infinite commands (like `ping` without -c) from blocking for too long.
const LIMITED_COMMAND_TIMEOUT_SECS: u64 = 30;

/// Maximum output lines for non-whitelisted commands.
///
/// Commands not in UNLIMITED_OUTPUT_COMMANDS will be terminated after this many lines.
/// This protects against infinite output commands like `yes` or `seq 1 999999`.
const MAX_OUTPUT_LINES: usize = 1000;

/// Commands that are safe to have unlimited output.
///
/// These are standard Unix utilities that don't produce infinite output.
/// Commands not in this list are limited to MAX_OUTPUT_LINES.
const UNLIMITED_OUTPUT_COMMANDS: &[&str] = &[
    // File listing
    "ls", "dir", "find", "locate", "tree", // File reading
    "cat", "head", "tail", "less", "more", // Text processing
    "grep", "awk", "sed", "cut", "sort", "uniq", "wc", // System info
    "ps", "top", "df", "du", "free", "uname", "whoami", "id", // Network
    "ifconfig", "ip", "netstat", "ss", "curl", "wget", // Package managers
    "apt", "apt-get", "yum", "dnf", "pacman", "brew", // Development
    "git", "cargo", "npm", "pip", "docker", "kubectl", // Other common utilities
    "echo", "printf", "date", "env", "which", "whereis", "file", "stat", "cp", "mv", "rm", "mkdir",
    "touch", "chmod", "chown", "tar", "gzip", "gunzip", "zip", "unzip", "diff", "patch", "make",
    "cmake", "man", "info", "help",
];

/// Check if a command is in the unlimited output whitelist
fn is_unlimited_output_command(cmd: &str) -> bool {
    UNLIMITED_OUTPUT_COMMANDS.contains(&cmd)
}

/// Commands that produce infinite output and should NOT be run in background.
///
/// These commands run forever and would consume CPU indefinitely.
/// Block them from background execution entirely.
const INFINITE_OUTPUT_COMMANDS: &[&str] = &["yes"];

/// Check if a command produces infinite output
fn is_infinite_output_command(cmd: &str) -> bool {
    INFINITE_OUTPUT_COMMANDS.contains(&cmd)
}

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
    "top", "htop", "btop", "atop", // CLI tools with interactive features
    "gh",
    // Note: "sudo" removed - now handled via root mode in orchestrator
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
];

/// Commands with specific subcommands that are interactive (open browser, etc.)
/// Format: (command, subcommand) - blocks "command subcommand ..."
const INTERACTIVE_SUBCOMMANDS: &[(&str, &str)] = &[
    // Cloud CLI auth commands that open browser
    ("gcloud", "auth"),    // gcloud auth login opens browser
    ("az", "login"),       // az login opens browser
    ("aws", "sso"),        // aws sso login opens browser
    ("gh", "auth"),        // gh auth login opens browser
    ("firebase", "login"), // firebase login opens browser
    ("heroku", "login"),   // heroku login opens browser
    ("netlify", "login"),  // netlify login opens browser
    ("vercel", "login"),   // vercel login opens browser
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

/// Check if arguments contain glob patterns that need shell expansion
///
/// Detects patterns like:
/// - `*` - match any characters (e.g., `file*`)
/// - `?` - match single character (e.g., `file?.txt`)
/// - `[...]` - character class (e.g., `file[123].txt`)
/// - `{...}` - brace expansion (e.g., `file{1..3}`)
///
/// Returns true if any argument contains a glob pattern that requires shell expansion.
fn has_glob_patterns(args: &[String]) -> bool {
    args.iter().any(|arg| {
        // Check for glob metacharacters
        arg.contains('*')
            || arg.contains('?')
            || (arg.contains('[') && arg.contains(']'))
            || (arg.contains('{') && arg.contains('}'))
    })
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

/// Send SIGINT to a child process (Unix only).
///
/// This sends the interrupt signal (Ctrl+C equivalent) to allow the process
/// to handle it gracefully (e.g., ping prints statistics before exiting).
#[cfg(unix)]
fn send_sigint(child: &Child) {
    if let Some(pid) = child.id() {
        // Safety: We're sending a standard signal to a process we own
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGINT);
        }
    }
}

/// Send termination signal to a child process (Windows fallback).
///
/// On Windows, there's no SIGINT equivalent, so we just mark for later kill.
#[cfg(not(unix))]
fn send_sigint(_child: &Child) {
    // On Windows, we can't send SIGINT. The process will be killed later.
}

/// Execute a child process with output line limiting, cancellation, and optional streaming.
///
/// Reads stdout/stderr incrementally and terminates the process if it exceeds
/// MAX_OUTPUT_LINES (for non-whitelisted commands) or if cancellation is requested.
///
/// # Arguments
/// * `child` - The spawned child process
/// * `unlimited` - If true, no line limit is applied (for whitelisted commands)
/// * `line_tx` - Optional channel sender for streaming output lines in real-time
/// * `cancel_token` - Optional cancellation token to interrupt execution
///
/// # Returns
/// `CommandOutput` with stdout, stderr, and exit code. If truncated or cancelled,
/// stdout includes an appropriate message.
async fn execute_with_limit(
    mut child: Child,
    unlimited: bool,
    line_tx: Option<mpsc::UnboundedSender<String>>,
    cancel_token: Option<CancellationToken>,
) -> Result<CommandOutput> {
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();

    let mut stdout_lines: Vec<String> = Vec::new();
    let mut stderr_lines: Vec<String> = Vec::new();
    let mut truncated = false;
    let mut cancelled = false;

    // Read stdout with optional line limit and cancellation check
    if let Some(out) = stdout {
        let reader = BufReader::new(out);
        let mut lines = reader.lines();

        // Track if we've already sent SIGINT (to avoid sending multiple times)
        let mut sigint_sent = false;

        loop {
            // Check for cancellation before reading next line
            // Send SIGINT once, then continue reading to capture final output (e.g., ping stats)
            if !sigint_sent {
                if let Some(ref token) = cancel_token {
                    if token.is_cancelled() {
                        cancelled = true;
                        // Send SIGINT to allow graceful shutdown (e.g., ping prints stats)
                        send_sigint(&child);
                        sigint_sent = true;
                        // Don't break - continue reading to capture final output
                    }
                }
            }

            // Use select to check both line reading and cancellation
            let line_result = if !sigint_sent {
                if let Some(ref token) = cancel_token {
                    tokio::select! {
                        biased;
                        _ = token.cancelled() => {
                            cancelled = true;
                            // Send SIGINT to allow graceful shutdown (e.g., ping prints stats)
                            send_sigint(&child);
                            sigint_sent = true;
                            // Continue reading to capture final output
                            continue;
                        }
                        result = lines.next_line() => result,
                    }
                } else {
                    lines.next_line().await
                }
            } else {
                // After SIGINT, just read remaining output without cancellation check
                lines.next_line().await
            };

            match line_result {
                Ok(Some(line)) => {
                    // Stream line in real-time if sender provided
                    if let Some(ref tx) = line_tx {
                        // Ignore send errors (receiver may have been dropped)
                        let _ = tx.send(line.clone());
                    }

                    stdout_lines.push(line);

                    // Check limit only for non-whitelisted commands
                    if !unlimited && stdout_lines.len() >= MAX_OUTPUT_LINES {
                        truncated = true;
                        // Kill the process to stop infinite output
                        child.kill().await.ok();
                        break;
                    }
                }
                Ok(None) => break, // EOF
                Err(_) => break,   // Error reading
            }
        }
    }

    // Read stderr (always with reasonable limit to catch errors)
    // Read even after cancellation to capture final error output
    if let Some(err) = stderr {
        let reader = BufReader::new(err);
        let mut lines = reader.lines();

        while let Ok(Some(line)) = lines.next_line().await {
            // Stream stderr lines too (prefixed to distinguish)
            if let Some(ref tx) = line_tx {
                let _ = tx.send(format!("[stderr] {}", line));
            }

            stderr_lines.push(line);
            // Limit stderr too to prevent memory issues
            if stderr_lines.len() > MAX_OUTPUT_LINES {
                break;
            }
        }
    }

    // Wait for process to finish (may already be dead if killed)
    let exit_code = match child.wait().await {
        Ok(status) => status.code().unwrap_or(-1),
        Err(_) => -1, // Process was killed
    };

    let mut stdout_output = stdout_lines.join("\n");

    if cancelled {
        let cancel_msg = "\n\n[Interrupted by Ctrl+C]".to_string();
        stdout_output.push_str(&cancel_msg);
        if let Some(ref tx) = line_tx {
            let _ = tx.send(cancel_msg);
        }
    } else if truncated {
        let truncation_msg = format!("\n\n[Output truncated at {} lines]", MAX_OUTPUT_LINES);
        stdout_output.push_str(&truncation_msg);
        // Also stream the truncation message
        if let Some(ref tx) = line_tx {
            let _ = tx.send(truncation_msg);
        }
    }

    Ok(CommandOutput {
        stdout: stdout_output,
        stderr: stderr_lines.join("\n"),
        exit_code,
    })
}

/// Command executor for running shell commands
#[derive(Debug)]
pub struct CommandExecutor;

impl CommandExecutor {
    /// Check if a command is interactive and should be blocked
    fn is_interactive_command(cmd: &str) -> bool {
        ALL_INTERACTIVE.contains(cmd)
    }

    /// Check if a command with its arguments matches an interactive subcommand pattern
    ///
    /// Returns Some((cmd, subcmd)) if blocked, None otherwise.
    /// Example: "gcloud auth login" matches ("gcloud", "auth")
    fn is_interactive_subcommand(
        cmd: &str,
        args: &[String],
    ) -> Option<(&'static str, &'static str)> {
        let first_arg = args.first().map_or("", |s| s.as_str());

        INTERACTIVE_SUBCOMMANDS
            .iter()
            .find(|(blocked_cmd, blocked_subcmd)| {
                cmd == *blocked_cmd && first_arg == *blocked_subcmd
            })
            .copied()
    }

    /// Check if shell input contains an interactive subcommand pattern
    fn shell_has_interactive_subcommand(shell_input: &str) -> Option<(&'static str, &'static str)> {
        let normalized = shell_input.trim();

        INTERACTIVE_SUBCOMMANDS
            .iter()
            .find(|(cmd, subcmd)| {
                // Match "cmd subcmd" at start of input (with word boundary)
                let pattern = format!("{} {}", cmd, subcmd);
                normalized.starts_with(&pattern)
                    && normalized
                        .get(pattern.len()..pattern.len() + 1)
                        .is_none_or(|c| c == " " || c.is_empty())
            })
            .copied()
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

    /// Check if a command triggers root mode entry (sudo su, su, sudo -i, etc.)
    ///
    /// These commands would normally open a new shell with root privileges.
    /// Instead, we enter "root mode" where all subsequent commands are
    /// executed with sudo, without leaving Infraware Terminal.
    pub fn is_enter_root_command(cmd: &str, args: &[String]) -> bool {
        match cmd {
            // Plain "su" or "su -" or "su root" enters root mode
            "su" => {
                args.is_empty()
                    || args
                        .first()
                        .map(|a| a == "-" || a == "-l" || a == "--login" || a == "root")
                        .unwrap_or(false)
            }
            // "sudo su", "sudo -i", "sudo -s", "sudo bash", etc.
            "sudo" => {
                if let Some(first_arg) = args.first() {
                    matches!(
                        first_arg.as_str(),
                        "su" | "-i" | "-s" | "bash" | "zsh" | "sh" | "-" | "--login"
                    )
                } else {
                    false
                }
            }
            _ => false,
        }
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

    /// Execute a command via shell with 5-minute timeout
    ///
    /// Helper function to reduce code duplication across shell execution paths.
    async fn execute_via_shell(command: &str) -> Result<CommandOutput> {
        let (shell, shell_flag) = Self::get_platform_shell();

        // Extract first command for whitelist check
        let first_cmd = command.split_whitespace().next().unwrap_or("");
        let unlimited = is_unlimited_output_command(first_cmd);

        // Use shorter timeout for non-whitelisted commands (30s vs 5min)
        let timeout_secs = if unlimited {
            COMMAND_TIMEOUT_SECS
        } else {
            LIMITED_COMMAND_TIMEOUT_SECS
        };

        let child = TokioCommand::new(shell)
            .arg(shell_flag)
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn shell command")?;

        let execution = execute_with_limit(child, unlimited, None, None);

        timeout(Duration::from_secs(timeout_secs), execution)
            .await
            .context(format!("Command timed out after {} seconds", timeout_secs))?
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

        // Block interactive subcommands (e.g., "gcloud auth login" opens browser)
        if let Some((blocked_cmd, blocked_subcmd)) = Self::is_interactive_subcommand(cmd, args) {
            log::warn!(
                "Blocked interactive subcommand: {} {}",
                blocked_cmd,
                blocked_subcmd
            );
            return Ok(CommandOutput {
                stdout: String::new(),
                stderr: format!(
                    "Command '{blocked_cmd} {blocked_subcmd}' is not supported in this terminal.\n\n\
                     Reason: This command opens a browser or requires interactive input.\n\n\
                     Suggestions:\n\
                     - Run '{blocked_cmd} {blocked_subcmd}' in a separate terminal window\n\
                     - Use non-interactive authentication (service accounts, tokens, etc.)"
                ),
                exit_code: 1,
            });
        }

        // Block commands targeting infinite devices (e.g., cat /dev/zero)
        // BUT allow if piped to a limiting command (e.g., cat /dev/zero | head -c 100)
        if targets_infinite_device(cmd, args) {
            // Check if original_input has a pipe to a limiting command
            let has_pipe_limit = original_input.is_some_and(|input| {
                input.contains("| head")
                    || input.contains("|head")
                    || input.contains("| tail")
                    || input.contains("|tail")
            });

            if !has_pipe_limit {
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
            // Has pipe limit - will be executed via shell path below
        }

        // Note: ping without -c is no longer blocked - it will be truncated at 1000 lines
        // by execute_with_limit(), which is a better user experience than blocking entirely.

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

            return Self::execute_via_shell(&full_command).await;
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

            // Check for interactive subcommands in shell input (e.g., "gcloud auth login")
            if let Some((blocked_cmd, blocked_subcmd)) =
                Self::shell_has_interactive_subcommand(shell_input)
            {
                log::warn!(
                    "Blocked interactive subcommand in shell: {} {}",
                    blocked_cmd,
                    blocked_subcmd
                );
                return Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: format!(
                        "Command '{blocked_cmd} {blocked_subcmd}' is not supported in this terminal.\n\n\
                         Reason: This command opens a browser or requires interactive input.\n\n\
                         Suggestions:\n\
                         - Run '{blocked_cmd} {blocked_subcmd}' in a separate terminal window\n\
                         - Use non-interactive authentication (service accounts, tokens, etc.)"
                    ),
                    exit_code: 1,
                });
            }

            return Self::execute_via_shell(shell_input).await;
        }

        // Check if arguments contain glob patterns (*, ?, [...], {...})
        // If so, execute through shell for proper expansion
        if has_glob_patterns(args) {
            log::debug!(
                "Command '{}' has glob patterns, executing through shell",
                cmd
            );

            // Reconstruct full command for shell execution
            let full_command = if args.is_empty() {
                cmd.to_string()
            } else {
                format!("{} {}", cmd, args.join(" "))
            };

            return Self::execute_via_shell(&full_command).await;
        }

        // Direct execution (no shell operators, no glob patterns)
        // Check if command exists
        if !Self::command_exists(cmd) {
            log::error!("Command not found: {}", cmd);
            anyhow::bail!("Command '{cmd}' not found");
        }

        // Check if command is in unlimited output whitelist
        let unlimited = is_unlimited_output_command(cmd);

        // Execute the command with streaming and output limit
        let child = TokioCommand::new(cmd)
            .args(args)
            .stdin(Stdio::null()) // Prevent interactive programs from blocking
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn command")?;

        // Use shorter timeout for non-whitelisted commands
        let timeout_secs = if unlimited {
            COMMAND_TIMEOUT_SECS
        } else {
            LIMITED_COMMAND_TIMEOUT_SECS
        };

        let execution = execute_with_limit(child, unlimited, None, None);

        timeout(Duration::from_secs(timeout_secs), execution)
            .await
            .context(format!("Command timed out after {} seconds", timeout_secs))?
    }

    /// Execute a command with streaming output and cancellation support.
    ///
    /// Returns a channel receiver that receives output lines in real-time,
    /// plus a JoinHandle that resolves to the final CommandOutput when complete.
    ///
    /// # Arguments
    /// * `cmd` - The command name
    /// * `args` - The command arguments
    /// * `original_input` - Optional original input string for shell execution
    /// * `cancel_token` - Cancellation token to interrupt execution on Ctrl+C
    ///
    /// # Returns
    /// Tuple of (receiver for streaming lines, handle for final result)
    pub fn execute_streaming(
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
        cancel_token: CancellationToken,
    ) -> (
        mpsc::UnboundedReceiver<String>,
        tokio::task::JoinHandle<Result<CommandOutput>>,
    ) {
        let (tx, rx) = mpsc::unbounded_channel();

        let cmd = cmd.to_string();
        let args = args.to_vec();
        let original_input = original_input.map(String::from);

        let handle = tokio::spawn(async move {
            Self::execute_with_streaming(&cmd, &args, original_input.as_deref(), tx, cancel_token)
                .await
        });

        (rx, handle)
    }

    /// Internal helper for streaming execution
    async fn execute_with_streaming(
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
        line_tx: mpsc::UnboundedSender<String>,
        cancel_token: CancellationToken,
    ) -> Result<CommandOutput> {
        // Check whitelist
        let unlimited = is_unlimited_output_command(cmd);

        // Use shorter timeout for non-whitelisted commands
        let timeout_secs = if unlimited {
            COMMAND_TIMEOUT_SECS
        } else {
            LIMITED_COMMAND_TIMEOUT_SECS
        };

        let (shell, shell_flag) = Self::get_platform_shell();

        // Determine execution path
        let child = if let Some(shell_input) = original_input {
            // Shell execution with original input
            TokioCommand::new(shell)
                .arg(shell_flag)
                .arg(shell_input)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn shell command")?
        } else {
            // Direct execution
            TokioCommand::new(cmd)
                .args(args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .context("Failed to spawn command")?
        };

        let execution = execute_with_limit(child, unlimited, Some(line_tx), Some(cancel_token));

        timeout(Duration::from_secs(timeout_secs), execution)
            .await
            .context(format!("Command timed out after {} seconds", timeout_secs))?
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
            // IMPORTANT: Explicitly inherit stdin/stdout/stderr so vim/nano/etc
            // get full terminal access. Without this, input can be lost or delayed.
            let result = tokio::task::spawn_blocking(move || {
                std::process::Command::new(&cmd)
                    .args(&args)
                    .stdin(Stdio::inherit())
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .status()
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
    pub fn get_command_path(cmd: &str) -> Option<String> {
        which::which(cmd)
            .ok()
            .and_then(|p| p.to_str().map(String::from))
    }

    /// Execute a command with sudo privileges (M2/M3)
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

        // Fast path: Must end with & but not &&
        if !trimmed.ends_with('&') || trimmed.ends_with("&&") {
            return false;
        }

        // Zero-allocation quote tracking via character iterator with peekable
        let mut in_single_quote = false;
        let mut in_double_quote = false;
        let mut last_was_backslash = false;
        let mut chars = trimmed.chars().peekable();

        while let Some(c) = chars.next() {
            let is_last = chars.peek().is_none();

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

        // Remove the trailing & from the command - we handle backgrounding ourselves
        // If we pass "sleep 10 &" to bash, bash will fork internally and exit immediately,
        // making it impossible to track the actual sleep process.
        let command_without_amp = command.trim().trim_end_matches('&').trim();

        // Block infinite output commands from running in background
        // They would run forever and consume CPU indefinitely
        let first_cmd = command_without_amp.split_whitespace().next().unwrap_or("");
        if is_infinite_output_command(first_cmd) {
            anyhow::bail!(
                "Command '{}' produces infinite output and cannot be run in background.\n\
                 Use it in foreground with output limit, or pipe to a limiting command:\n\
                 Example: {} | head -n 100",
                first_cmd,
                first_cmd
            );
        }

        // Block commands targeting infinite devices (e.g., cat /dev/zero &)
        if shell_command_has_infinite_device(command_without_amp) {
            anyhow::bail!(
                "Command contains reference to infinite device (/dev/zero, /dev/urandom, /dev/random).\n\
                 Cannot run in background - would consume resources indefinitely.\n\
                 Use with output limit: cat /dev/urandom | head -c 1000 &"
            );
        }

        log::info!(
            "Spawning background command: {} (original: {})",
            command_without_amp,
            command
        );

        let child = TokioCommand::new(shell)
            .arg(flag)
            .arg(command_without_amp)
            .stdin(Stdio::null())
            .stdout(Stdio::null()) // Don't capture output for background
            .stderr(Stdio::null())
            .spawn()
            .context("Failed to spawn background command")?;

        let pid = child
            .id()
            .context("Failed to get PID for background process - this should never happen")?;

        let job_id = {
            let mut mgr = match job_manager.write() {
                Ok(guard) => guard,
                Err(_poisoned) => {
                    // Lock poisoning indicates a previous panic violated invariants.
                    // Per Microsoft Rust Guidelines M-PANIC-IS-STOP, fail fast rather
                    // than continue with potentially corrupted state.
                    anyhow::bail!(
                        "JobManager lock poisoned during execute_background. \
                         Cannot safely track background job due to potential state corruption."
                    );
                }
            };
            // Store original command (with &) for display
            mgr.add_job(command.trim().to_string(), pid, child)
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

        // sudo is handled via root mode wrapper, not as interactive command
        assert!(!CommandExecutor::requires_interactive("sudo"));

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
        // sudo is handled via root mode wrapper, not as interactive command
        assert!(!CommandExecutor::is_interactive_command("sudo"));

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
    fn test_is_interactive_subcommand() {
        // gcloud auth should be blocked
        let args = vec!["auth".to_string(), "login".to_string()];
        assert!(CommandExecutor::is_interactive_subcommand("gcloud", &args).is_some());

        // gcloud compute should NOT be blocked
        let args = vec!["compute".to_string(), "instances".to_string()];
        assert!(CommandExecutor::is_interactive_subcommand("gcloud", &args).is_none());

        // az login should be blocked
        let args = vec!["login".to_string()];
        assert!(CommandExecutor::is_interactive_subcommand("az", &args).is_some());

        // az vm should NOT be blocked
        let args = vec!["vm".to_string(), "list".to_string()];
        assert!(CommandExecutor::is_interactive_subcommand("az", &args).is_none());

        // gh auth should be blocked
        let args = vec!["auth".to_string(), "login".to_string()];
        assert!(CommandExecutor::is_interactive_subcommand("gh", &args).is_some());

        // Empty args should not match
        assert!(CommandExecutor::is_interactive_subcommand("gcloud", &[]).is_none());
    }

    #[test]
    fn test_shell_has_interactive_subcommand() {
        // Direct commands
        assert!(CommandExecutor::shell_has_interactive_subcommand("gcloud auth login").is_some());
        assert!(CommandExecutor::shell_has_interactive_subcommand("az login").is_some());
        assert!(CommandExecutor::shell_has_interactive_subcommand("gh auth login").is_some());

        // With extra args
        assert!(CommandExecutor::shell_has_interactive_subcommand(
            "gcloud auth login --no-launch-browser"
        )
        .is_some());

        // Non-interactive subcommands
        assert!(
            CommandExecutor::shell_has_interactive_subcommand("gcloud compute instances list")
                .is_none()
        );
        assert!(CommandExecutor::shell_has_interactive_subcommand("az vm list").is_none());

        // Partial matches should NOT trigger (word boundary)
        assert!(
            CommandExecutor::shell_has_interactive_subcommand("gcloud authorization").is_none()
        );
    }

    #[tokio::test]
    async fn test_gcloud_auth_blocked() {
        let output =
            CommandExecutor::execute("gcloud", &["auth".to_string(), "login".to_string()], None)
                .await
                .unwrap();

        assert!(!output.is_success());
        assert!(output.stderr.contains("not supported"));
        assert!(output.stderr.contains("opens a browser"));
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

    // ========== Glob Pattern Detection Tests ==========

    #[test]
    fn test_has_glob_patterns_asterisk() {
        assert!(has_glob_patterns(&["file*".to_string(), "-rf".to_string()]));
        assert!(has_glob_patterns(&["*.txt".to_string()]));
        assert!(has_glob_patterns(&["file*.log".to_string()]));
    }

    #[test]
    fn test_has_glob_patterns_question_mark() {
        assert!(has_glob_patterns(&["file?.txt".to_string()]));
        assert!(has_glob_patterns(&["test??.log".to_string()]));
    }

    #[test]
    fn test_has_glob_patterns_brackets() {
        assert!(has_glob_patterns(&["file[123].txt".to_string()]));
        assert!(has_glob_patterns(&["test[a-z].log".to_string()]));
    }

    #[test]
    fn test_has_glob_patterns_braces() {
        assert!(has_glob_patterns(&["file{1..3}".to_string()]));
        assert!(has_glob_patterns(&["test{a,b,c}.txt".to_string()]));
    }

    #[test]
    fn test_has_glob_patterns_no_patterns() {
        assert!(!has_glob_patterns(&["file.txt".to_string()]));
        assert!(!has_glob_patterns(&[
            "-rf".to_string(),
            "directory".to_string()
        ]));
        assert!(!has_glob_patterns(&["test123.log".to_string()]));
    }

    #[test]
    fn test_has_glob_patterns_empty() {
        assert!(!has_glob_patterns(&[]));
    }

    #[test]
    fn test_has_glob_patterns_mixed() {
        // One arg with pattern, others without
        assert!(has_glob_patterns(&[
            "-rf".to_string(),
            "file*".to_string(),
            "other".to_string()
        ]));
    }

    // ========== Output Truncation Tests ==========

    #[test]
    fn test_is_unlimited_output_command() {
        // Whitelisted commands
        assert!(is_unlimited_output_command("ls"));
        assert!(is_unlimited_output_command("cat"));
        assert!(is_unlimited_output_command("grep"));
        assert!(is_unlimited_output_command("git"));
        assert!(is_unlimited_output_command("docker"));
        assert!(is_unlimited_output_command("echo"));

        // Non-whitelisted commands (should be limited)
        assert!(!is_unlimited_output_command("yes"));
        assert!(!is_unlimited_output_command("seq"));
        assert!(!is_unlimited_output_command("unknown_command"));
    }

    #[tokio::test]
    async fn test_yes_truncated_at_limit() {
        // yes produces infinite output, should be truncated
        let output = CommandExecutor::execute("yes", &[], None).await.unwrap();
        assert!(output.stdout.contains("[Output truncated"));
        let line_count = output.stdout.lines().count();
        // Should have MAX_OUTPUT_LINES + truncation message (2 lines)
        assert!(line_count <= MAX_OUTPUT_LINES + 3);
    }

    #[tokio::test]
    async fn test_seq_truncated_when_exceeds_limit() {
        // seq is NOT whitelisted, so it should be truncated
        let output = CommandExecutor::execute("seq", &["1".to_string(), "2000".to_string()], None)
            .await
            .unwrap();
        assert!(output.stdout.contains("[Output truncated"));
    }

    #[tokio::test]
    async fn test_ls_not_truncated() {
        // ls IS whitelisted - should not be truncated for normal directories
        let output = CommandExecutor::execute("ls", &["-la".to_string()], None)
            .await
            .unwrap();
        assert!(!output.stdout.contains("[Output truncated"));
    }

    #[tokio::test]
    async fn test_echo_not_truncated() {
        // echo IS whitelisted
        let output = CommandExecutor::execute("echo", &["hello".to_string()], None)
            .await
            .unwrap();
        assert!(!output.stdout.contains("[Output truncated"));
        assert!(output.stdout.contains("hello"));
    }

    #[test]
    fn test_is_infinite_output_command() {
        assert!(is_infinite_output_command("yes"));
        assert!(!is_infinite_output_command("echo"));
        assert!(!is_infinite_output_command("ls"));
    }
}
