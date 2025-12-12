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
        // Handle "enter root mode" commands (sudo su, su, sudo -i, etc.)
        log::debug!(
            "handle_command: cmd='{}', args={:?}, is_enter_root={}",
            cmd,
            args,
            CommandExecutor::is_enter_root_command(cmd, args)
        );
        if CommandExecutor::is_enter_root_command(cmd, args) {
            log::info!("Entering root mode for command: {} {:?}", cmd, args);
            return self.handle_enter_root_mode(state, ui).await;
        }

        // NOTE: "exit" command is handled in main.rs before calling this function
        // This is intentional to allow early exit handling at the application level

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

        // Handle history built-in command
        if cmd == "history" {
            return self.handle_history_command(state, args);
        }

        // ==================== Command Confirmation Checks ====================
        // Priority for rm: -i (per-file) > -I (bulk) > write-protected

        if cmd == "rm" {
            // rm -i: per-file confirmation
            if let Some(files) = Self::needs_rm_interactive_confirmation(args) {
                return self.handle_rm_interactive(files, original_input, args, state);
            }
            // rm -I: bulk confirmation (>3 files or recursive)
            if let Some((count, recursive)) = Self::needs_rm_bulk_confirmation(args) {
                return self.handle_rm_bulk(count, recursive, original_input, args, state);
            }
            // rm on write-protected files
            if let Some(protected_files) = Self::needs_rm_confirmation(args) {
                return self.handle_rm_confirmation(protected_files, original_input, args, state);
            }
        }

        // cp -i: overwrite confirmation
        if cmd == "cp" {
            if let Some(dest) = Self::needs_cp_mv_confirmation(args) {
                return self.handle_cp_confirmation(dest, original_input, args, state);
            }
        }

        // mv -i: overwrite confirmation
        if cmd == "mv" {
            if let Some(dest) = Self::needs_cp_mv_confirmation(args) {
                return self.handle_mv_confirmation(dest, original_input, args, state);
            }
        }

        // ln -i: replace destination confirmation
        if cmd == "ln" {
            if let Some(dest) = Self::needs_ln_confirmation(args) {
                return self.handle_ln_confirmation(dest, original_input, args, state);
            }
        }

        // ==================== End Confirmation Checks ====================

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
            // Per Microsoft Rust Guidelines M-PANIC-IS-STOP: Lock poisoning indicates
            // a previous panic violated invariants. Fail fast rather than continue
            // with potentially corrupted state.
            let mgr = match job_manager.read() {
                Ok(guard) => guard,
                Err(_poisoned) => {
                    anyhow::bail!(
                        "JobManager lock poisoned during handle_jobs_command. \
                         Cannot safely list jobs due to potential state corruption."
                    );
                }
            };
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

    /// Handle the built-in "history" command
    ///
    /// Shows the command history, optionally limited to the last N entries.
    /// Usage: `history` (show all) or `history N` (show last N entries)
    fn handle_history_command(&self, state: &mut TerminalState, args: &[String]) -> Result<()> {
        // Clone history to avoid borrow checker issues with state.add_output()
        let all_history: Vec<String> = state.history.all().to_vec();

        if all_history.is_empty() {
            state.add_output(MessageFormatter::info("No command history"));
            return Ok(());
        }

        // Parse optional limit argument (e.g., "history 10")
        let limit = if let Some(arg) = args.first() {
            match arg.parse::<usize>() {
                Ok(n) if n > 0 => Some(n),
                Ok(_) => {
                    state.add_output(MessageFormatter::error(
                        "history: limit must be a positive number",
                    ));
                    return Ok(());
                }
                Err(_) => {
                    state.add_output(MessageFormatter::error(format!(
                        "history: invalid number: {}",
                        arg
                    )));
                    return Ok(());
                }
            }
        } else {
            None
        };

        // Determine which entries to show
        let entries: Vec<(usize, String)> = if let Some(n) = limit {
            // Show last N entries
            let start_idx = all_history.len().saturating_sub(n);
            all_history
                .iter()
                .enumerate()
                .skip(start_idx)
                .map(|(i, cmd)| (i + 1, cmd.clone()))
                .collect()
        } else {
            // Show all entries
            all_history
                .iter()
                .enumerate()
                .map(|(i, cmd)| (i + 1, cmd.clone()))
                .collect()
        };

        // Display each command with its number
        for (num, cmd) in entries {
            state.add_output(format!("  {} {}", num, cmd));
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
        // In root mode, prefix commands with "sudo -n" (non-interactive)
        let output_result = if state.is_root_mode() {
            // Build the full command with sudo prefix
            let sudo_cmd = if let Some(input) = original_input {
                format!("sudo -n {}", input)
            } else {
                let args_str = args.join(" ");
                if args_str.is_empty() {
                    format!("sudo -n {}", cmd)
                } else {
                    format!("sudo -n {} {}", cmd, args_str)
                }
            };
            // Execute via shell to handle the sudo prefix
            CommandExecutor::execute("sh", &["-c".to_string(), sudo_cmd], None).await
        } else {
            CommandExecutor::execute(cmd, args, original_input).await
        };

        match output_result {
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

    /// Handle entering root mode (sudo su, su, sudo -i, etc.)
    ///
    /// This prompts for password in TUI and validates via `sudo -S`.
    /// On success, enters root mode where all commands are prefixed with `sudo -n`.
    async fn handle_enter_root_mode(
        &self,
        state: &mut TerminalState,
        _ui: &mut TerminalUI,
    ) -> Result<()> {
        log::info!(
            "handle_enter_root_mode called, current is_root_mode={}",
            state.is_root_mode()
        );

        // Already in root mode? Just inform and return
        if state.is_root_mode() {
            state.add_output(MessageFormatter::info("Already in root mode."));
            return Ok(());
        }

        // Check if sudo credentials are already cached (no password needed)
        // Use -n (non-interactive) to avoid any prompts, suppress all output
        let cached = tokio::task::spawn_blocking(|| {
            use std::process::Stdio;
            std::process::Command::new("sudo")
                .args(["-n", "true"])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        })
        .await
        .unwrap_or(false);

        if cached {
            log::info!("Sudo credentials already cached, entering root mode directly");
            state.enter_root_mode();
            state.add_output(MessageFormatter::success(
                "Entered root mode. Type 'exit' to return to normal user.",
            ));
            return Ok(());
        }

        // Need password - set up password prompt mode (simple prompt like real sudo)
        state.pending_interaction = Some(crate::terminal::PendingInteraction::Question {
            question: "[sudo] password: ".to_string(),
            options: None,
        });
        state.mode = crate::terminal::TerminalMode::AwaitingAnswer;

        Ok(())
    }

    /// Validate sudo password and enter root mode if successful
    ///
    /// Called when user submits password in AwaitingAnswer mode for root authentication.
    pub async fn validate_sudo_password(
        &self,
        password: String,
        state: &mut TerminalState,
    ) -> Result<bool> {
        log::info!("Validating sudo password");

        // Use sudo -S to read password from stdin
        // Run "sudo -S true" to validate credentials (runs /bin/true with sudo)
        let result = tokio::task::spawn_blocking(move || {
            use std::io::Write;
            use std::process::{Command, Stdio};

            let mut child = Command::new("sudo")
                .args(["-S", "true"]) // -S reads from stdin, "true" is a no-op command
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null()) // Suppress "Password:" prompt from sudo
                .spawn()?;

            // Write password to stdin followed by newline
            if let Some(mut stdin) = child.stdin.take() {
                // sudo expects password followed by newline
                writeln!(stdin, "{}", password)?;
                // Ensure it's flushed before closing
                stdin.flush()?;
            }

            child.wait()
        })
        .await
        .map_err(|e| anyhow::anyhow!("sudo validation task panicked: {}", e))?;

        match result {
            Ok(status) if status.success() => {
                log::info!("Sudo authentication successful");
                state.enter_root_mode();
                state.add_output(MessageFormatter::success(
                    "Entered root mode. Type 'exit' to return to normal user.",
                ));
                Ok(true)
            }
            Ok(_) => {
                log::warn!("Sudo authentication failed");
                state.add_output(MessageFormatter::error(
                    "Authentication failed. Incorrect password.",
                ));
                Ok(false)
            }
            Err(e) => {
                log::error!("Sudo command error: {}", e);
                state.add_output(MessageFormatter::error(format!(
                    "Failed to authenticate: {}",
                    e
                )));
                Ok(false)
            }
        }
    }

    /// Check if we're waiting for sudo password
    pub fn is_waiting_for_sudo_password(state: &TerminalState) -> bool {
        if let Some(crate::terminal::PendingInteraction::Question { question, .. }) =
            &state.pending_interaction
        {
            question.contains("[sudo] password")
        } else {
            false
        }
    }

    // ==================== Flag Detection Helpers ====================

    /// Generic flag detection helper
    ///
    /// Checks if args contain a flag with:
    /// - Exact short form match (e.g., "-f")
    /// - Combined short flags containing the character (e.g., "-rf" contains 'f')
    /// - Optional long form match (e.g., "--force")
    /// - Optional exclusion character (e.g., 'i' but not 'I')
    fn has_flag(args: &[String], short: char, long: Option<&str>, exclude: Option<char>) -> bool {
        args.iter().any(|a| {
            // Long form match
            if let Some(l) = long {
                if a == l {
                    return true;
                }
            }
            // Short form: -X or combined flags containing X
            if a.starts_with('-') && !a.starts_with("--") && a.contains(short) {
                // Check exclusion (e.g., 'i' but not if 'I' present)
                if let Some(exc) = exclude {
                    return !a.contains(exc);
                }
                return true;
            }
            false
        })
    }

    /// Check for -i flag (handles -iv, -ri, etc. but NOT -I)
    fn has_interactive_flag(args: &[String]) -> bool {
        Self::has_flag(args, 'i', None, Some('I'))
    }

    /// Check for -I flag (bulk interactive)
    fn has_bulk_interactive_flag(args: &[String]) -> bool {
        Self::has_flag(args, 'I', None, None)
    }

    /// Check for -f flag (force - disables confirmation)
    fn has_force_flag(args: &[String]) -> bool {
        Self::has_flag(args, 'f', Some("--force"), None)
    }

    /// Check for recursive flags (-r, -R, --recursive)
    fn has_recursive_flag(args: &[String]) -> bool {
        Self::has_flag(args, 'r', Some("--recursive"), None)
            || Self::has_flag(args, 'R', None, None)
    }

    /// Extract file/path arguments from args (filters out flags)
    fn get_file_args(args: &[String]) -> Vec<&String> {
        args.iter().filter(|a| !a.starts_with('-')).collect()
    }

    /// Build command string from original input or reconstruct from parts
    fn build_command_string(original_input: Option<&str>, cmd: &str, args: &[String]) -> String {
        original_input.map(|s| s.to_string()).unwrap_or_else(|| {
            if args.is_empty() {
                cmd.to_string()
            } else {
                format!("{} {}", cmd, args.join(" "))
            }
        })
    }

    /// Add -f (force) flag to a command string
    fn add_force_flag(command: &str, cmd_name: &str) -> String {
        command.replacen(&format!("{} ", cmd_name), &format!("{} -f ", cmd_name), 1)
    }

    // ==================== Detection Functions ====================

    /// Check if rm -i needs per-file confirmation
    /// Returns Some(files) if -i is present, None otherwise
    fn needs_rm_interactive_confirmation(args: &[String]) -> Option<Vec<String>> {
        if Self::has_force_flag(args) || !Self::has_interactive_flag(args) {
            return None;
        }

        let files: Vec<String> = Self::get_file_args(args).into_iter().cloned().collect();

        if files.is_empty() {
            None
        } else {
            Some(files)
        }
    }

    /// Check if rm -I needs bulk confirmation
    /// -I prompts once if: >3 files OR recursive deletion
    fn needs_rm_bulk_confirmation(args: &[String]) -> Option<(usize, bool)> {
        if Self::has_force_flag(args) || !Self::has_bulk_interactive_flag(args) {
            return None;
        }

        let file_count = Self::get_file_args(args).len();
        let is_recursive = Self::has_recursive_flag(args);

        // Only prompt if >3 files or recursive
        if file_count > 3 || is_recursive {
            Some((file_count, is_recursive))
        } else {
            None
        }
    }

    /// Check if cp/mv -i needs confirmation for overwrite
    /// Returns Some(destination) if -i is present and destination exists
    fn needs_cp_mv_confirmation(args: &[String]) -> Option<String> {
        if Self::has_force_flag(args) || !Self::has_interactive_flag(args) {
            return None;
        }

        let paths = Self::get_file_args(args);

        if paths.len() >= 2 {
            let dest = paths.last()?;
            let dest_path = std::path::Path::new(dest);

            if paths.len() == 2 {
                // Single file copy: check if dest file exists
                if dest_path.exists() && dest_path.is_file() {
                    return Some(dest.to_string());
                }
            } else {
                // Multiple sources to directory: check if any target exists
                if dest_path.is_dir() {
                    for source in &paths[..paths.len() - 1] {
                        if let Some(name) = std::path::Path::new(source).file_name() {
                            let target = dest_path.join(name);
                            if target.exists() {
                                return Some(target.display().to_string());
                            }
                        }
                    }
                }
            }
        }

        None
    }

    /// Check if ln -i needs confirmation for removing destination
    fn needs_ln_confirmation(args: &[String]) -> Option<String> {
        if Self::has_force_flag(args) || !Self::has_interactive_flag(args) {
            return None;
        }

        let paths = Self::get_file_args(args);

        if paths.len() >= 2 {
            let dest = paths.last()?;
            if std::path::Path::new(dest).exists() {
                return Some(dest.to_string());
            }
        }

        None
    }

    // ==================== Write-Protected Detection ====================

    /// Check if user can write to a file (Unix: uses access() syscall)
    ///
    /// This checks the effective permissions for the current user, which is
    /// exactly what `rm` does to decide whether to prompt for confirmation.
    #[cfg(unix)]
    fn user_can_write_file(path: &std::path::Path) -> bool {
        use std::ffi::CString;
        use std::os::unix::ffi::OsStrExt;

        if let Ok(c_path) = CString::new(path.as_os_str().as_bytes()) {
            // SAFETY: FFI to libc::access (M-UNSAFE: FFI and platform interactions)
            // Preconditions satisfied:
            // - c_path is a valid null-terminated C string (guaranteed by CString::new Ok)
            // - libc::W_OK is a valid constant defined by libc (value 2 on Unix)
            // - access() is thread-safe and only reads filesystem metadata
            // This call cannot cause UB as c_path remains valid for the duration of the call.
            unsafe { libc::access(c_path.as_ptr(), libc::W_OK) == 0 }
        } else {
            // If path conversion fails (contains null bytes), assume writable (don't block)
            true
        }
    }

    #[cfg(not(unix))]
    fn user_can_write_file(path: &std::path::Path) -> bool {
        // On Windows, fall back to readonly check
        std::fs::metadata(path)
            .map(|m| !m.permissions().readonly())
            .unwrap_or(true)
    }

    /// Check if rm command needs confirmation for write-protected files
    ///
    /// Returns Some(list of protected files) if confirmation is needed,
    /// None if the command can proceed without confirmation.
    ///
    /// A file is considered "write-protected" if the current user cannot write to it.
    /// This matches `rm` behavior which checks effective permissions, not just the
    /// readonly bit.
    fn needs_rm_confirmation(args: &[String]) -> Option<Vec<String>> {
        // Skip if -f flag is present (user explicitly wants force)
        if Self::has_force_flag(args) {
            return None;
        }

        // Get file paths from args (skip flags)
        let files = Self::get_file_args(args);

        // Check which files the user cannot write to
        let mut protected_files = Vec::new();
        for file in files {
            let path = std::path::Path::new(file);
            // Only check existing files - rm will handle non-existent files
            if path.exists() && !Self::user_can_write_file(path) {
                protected_files.push(file.clone());
            }
        }

        if protected_files.is_empty() {
            None
        } else {
            Some(protected_files)
        }
    }

    /// Handle rm confirmation for protected files
    fn handle_rm_confirmation(
        &self,
        protected_files: Vec<String>,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        let files_list = protected_files.join(", ");
        let command = Self::build_command_string(original_input, "rm", args);

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            message: format!("rm: remove write-protected file(s): {}?", files_list),
            confirmation_type: Some(crate::terminal::ConfirmationType::RmWriteProtected),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    // ==================== Confirmation Handlers ====================

    /// Get file type description for rm prompt (matches Linux output)
    fn get_file_type_description(path: &str) -> &'static str {
        let p = std::path::Path::new(path);
        if let Ok(meta) = p.symlink_metadata() {
            if meta.is_dir() {
                "directory"
            } else if meta.file_type().is_symlink() {
                "symbolic link"
            } else {
                "regular file"
            }
        } else {
            "file"
        }
    }

    /// Handle rm -i confirmation for individual files
    fn handle_rm_interactive(
        &self,
        files: Vec<String>,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        if files.is_empty() {
            return Ok(());
        }

        // SAFETY: files.is_empty() check above guarantees first() returns Some.
        // Using expect() instead of unwrap_or_default() for clearer intent.
        let first_file = files
            .first()
            .cloned()
            .expect("files vec verified non-empty above");
        let command = Self::build_command_string(original_input, "rm", args);

        // Linux rm format: "rm: remove regular file 'X'?"
        let file_type = Self::get_file_type_description(&first_file);
        let message = format!("rm: remove {} '{}'?", file_type, first_file);

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command: command.clone(),
            message,
            confirmation_type: Some(crate::terminal::ConfirmationType::RmInteractive {
                files,
                current_index: 0,
                command,
            }),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    /// Handle rm -I bulk confirmation
    fn handle_rm_bulk(
        &self,
        file_count: usize,
        is_recursive: bool,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        let command = Self::build_command_string(original_input, "rm", args);

        // Linux rm -I format: "rm: remove N arguments?" or "rm: remove N arguments recursively?"
        let message = if is_recursive {
            format!("rm: remove {} arguments recursively?", file_count)
        } else {
            format!("rm: remove {} arguments?", file_count)
        };

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            message,
            confirmation_type: Some(crate::terminal::ConfirmationType::RmInteractiveBulk {
                file_count,
                is_recursive,
            }),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    /// Handle cp -i confirmation for overwrite
    fn handle_cp_confirmation(
        &self,
        destination: String,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        let command = Self::build_command_string(original_input, "cp", args);

        // Linux cp format: "cp: overwrite 'X'?"
        let message = format!("cp: overwrite '{}'?", destination);

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            message,
            confirmation_type: Some(crate::terminal::ConfirmationType::CpInteractive {
                destination,
            }),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    /// Handle mv -i confirmation for overwrite
    fn handle_mv_confirmation(
        &self,
        destination: String,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        let command = Self::build_command_string(original_input, "mv", args);

        // Linux mv format: "mv: overwrite 'X'?"
        let message = format!("mv: overwrite '{}'?", destination);

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            message,
            confirmation_type: Some(crate::terminal::ConfirmationType::MvInteractive {
                destination,
            }),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    /// Handle ln -i confirmation for removing destination
    fn handle_ln_confirmation(
        &self,
        destination: String,
        original_input: Option<&str>,
        args: &[String],
        state: &mut TerminalState,
    ) -> Result<()> {
        let command = Self::build_command_string(original_input, "ln", args);

        // Linux ln format: "ln: replace 'X'?"
        let message = format!("ln: replace '{}'?", destination);

        state.pending_interaction = Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            message,
            confirmation_type: Some(crate::terminal::ConfirmationType::LnInteractive {
                destination,
            }),
        });
        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;

        Ok(())
    }

    /// Helper to remove a specific flag from command string
    fn remove_flag_from_command(cmd: &str, flag: &str) -> String {
        cmd.split_whitespace()
            .filter(|part| *part != flag)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Helper to execute a shell command and display output
    async fn execute_shell_command(&self, cmd: &str, state: &mut TerminalState) -> Result<()> {
        let output = crate::executor::CommandExecutor::execute(
            "sh",
            &["-c".to_string(), cmd.to_string()],
            None,
        )
        .await;

        match output {
            Ok(out) => {
                if !out.stdout.is_empty() {
                    for line in out.stdout.lines() {
                        state.add_output(line.to_string());
                    }
                }
                if !out.stderr.is_empty() {
                    for line in out.stderr.lines() {
                        if out.is_success() {
                            state.add_output(line.to_string());
                        } else {
                            state.add_output(MessageFormatter::stderr_error(line));
                        }
                    }
                }
                if !out.is_success() {
                    state.add_output(MessageFormatter::command_failed(out.exit_code));
                }
            }
            Err(e) => {
                state.add_output(MessageFormatter::error(format!("Failed to execute: {}", e)));
            }
        }

        Ok(())
    }

    /// Handle shell confirmation approval (rm on write-protected files, etc.)
    ///
    /// This is called from main.rs when user responds y/n to a shell confirmation.
    /// Business logic is kept here in the orchestrator, main.rs only delegates.
    pub async fn handle_shell_confirmation(
        &self,
        approved: bool,
        state: &mut TerminalState,
    ) -> Result<()> {
        // Take the pending interaction
        let pending = state.pending_interaction.take();
        state.mode = crate::terminal::TerminalMode::Normal;

        // Special handling for rm -i: 'n' skips to next file, doesn't cancel all
        if !approved {
            if let Some(crate::terminal::PendingInteraction::CommandApproval {
                confirmation_type:
                    Some(crate::terminal::ConfirmationType::RmInteractive {
                        files,
                        current_index,
                        command,
                    }),
                ..
            }) = pending
            {
                // Skip this file, move to next
                if current_index + 1 < files.len() {
                    let next_index = current_index + 1;
                    // SAFETY: bounds check above guarantees get() returns Some
                    let next_file = files
                        .get(next_index)
                        .cloned()
                        .expect("next_index verified in bounds above");
                    let file_type = Self::get_file_type_description(&next_file);
                    let message = format!("rm: remove {} '{}'?", file_type, next_file);

                    state.pending_interaction =
                        Some(crate::terminal::PendingInteraction::CommandApproval {
                            command: command.clone(),
                            message,
                            confirmation_type: Some(
                                crate::terminal::ConfirmationType::RmInteractive {
                                    files,
                                    current_index: next_index,
                                    command,
                                },
                            ),
                        });
                    state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;
                    return Ok(());
                }
                // All files processed (skipped)
                return Ok(());
            }

            // For other confirmation types, just cancel
            state.add_output(MessageFormatter::info("Cancelled."));
            return Ok(());
        }

        // Handle based on confirmation type
        if let Some(crate::terminal::PendingInteraction::CommandApproval {
            command,
            confirmation_type: Some(confirm_type),
            ..
        }) = pending
        {
            match confirm_type {
                crate::terminal::ConfirmationType::RmWriteProtected => {
                    // Execute rm -f to force removal of write-protected files
                    let forced_cmd = Self::add_force_flag(&command, "rm");
                    self.execute_shell_command(&forced_cmd, state).await?;
                }

                crate::terminal::ConfirmationType::RmInteractive {
                    files,
                    current_index,
                    command: orig_cmd,
                } => {
                    // Remove just this one file (without -i to avoid subprocess prompt)
                    if let Some(file) = files.get(current_index) {
                        let single_cmd = format!("rm '{}'", file);
                        self.execute_shell_command(&single_cmd, state).await?;
                    }

                    // If more files remain, prompt for next
                    if current_index + 1 < files.len() {
                        let next_index = current_index + 1;
                        // SAFETY: bounds check above guarantees get() returns Some
                        let next_file = files
                            .get(next_index)
                            .cloned()
                            .expect("next_index verified in bounds above");
                        let file_type = Self::get_file_type_description(&next_file);
                        let message = format!("rm: remove {} '{}'?", file_type, next_file);

                        state.pending_interaction =
                            Some(crate::terminal::PendingInteraction::CommandApproval {
                                command: orig_cmd.clone(),
                                message,
                                confirmation_type: Some(
                                    crate::terminal::ConfirmationType::RmInteractive {
                                        files,
                                        current_index: next_index,
                                        command: orig_cmd,
                                    },
                                ),
                            });
                        state.mode = crate::terminal::TerminalMode::AwaitingCommandApproval;
                    }
                }

                crate::terminal::ConfirmationType::RmInteractiveBulk { .. } => {
                    // Remove -I flag and execute the full command
                    let exec_cmd = Self::remove_flag_from_command(&command, "-I");
                    self.execute_shell_command(&exec_cmd, state).await?;
                }

                crate::terminal::ConfirmationType::CpInteractive { .. } => {
                    // Execute cp with -f flag to force overwrite
                    let exec_cmd = Self::add_force_flag(&command, "cp");
                    self.execute_shell_command(&exec_cmd, state).await?;
                }

                crate::terminal::ConfirmationType::MvInteractive { .. } => {
                    // Execute mv with -f flag to force overwrite
                    let exec_cmd = Self::add_force_flag(&command, "mv");
                    self.execute_shell_command(&exec_cmd, state).await?;
                }

                crate::terminal::ConfirmationType::LnInteractive { .. } => {
                    // Execute ln with -f flag to force (removes existing destination)
                    let exec_cmd = Self::add_force_flag(&command, "ln");
                    self.execute_shell_command(&exec_cmd, state).await?;
                }
            }
        }

        Ok(())
    }

    /// Check if a pending interaction is a shell confirmation (not LLM)
    pub fn is_shell_confirmation(state: &TerminalState) -> bool {
        matches!(
            &state.pending_interaction,
            Some(crate::terminal::PendingInteraction::CommandApproval {
                confirmation_type: Some(_),
                ..
            })
        )
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

    // ==================== Flag Detection Tests ====================

    #[test]
    fn test_has_interactive_flag() {
        // Standalone -i
        assert!(CommandOrchestrator::has_interactive_flag(&[
            "-i".to_string()
        ]));
        // Combined flags like -iv
        assert!(CommandOrchestrator::has_interactive_flag(&[
            "-iv".to_string()
        ]));
        assert!(CommandOrchestrator::has_interactive_flag(&[
            "-ri".to_string()
        ]));
        // Should NOT match -I (capital I)
        assert!(!CommandOrchestrator::has_interactive_flag(&[
            "-I".to_string()
        ]));
        assert!(!CommandOrchestrator::has_interactive_flag(&[
            "-rI".to_string()
        ]));
        // Should NOT match non-i flags
        assert!(!CommandOrchestrator::has_interactive_flag(&[
            "-f".to_string()
        ]));
        assert!(!CommandOrchestrator::has_interactive_flag(&[
            "-r".to_string()
        ]));
    }

    #[test]
    fn test_has_bulk_interactive_flag() {
        // Standalone -I
        assert!(CommandOrchestrator::has_bulk_interactive_flag(&[
            "-I".to_string()
        ]));
        // Combined flags like -rI
        assert!(CommandOrchestrator::has_bulk_interactive_flag(&[
            "-rI".to_string()
        ]));
        // Should NOT match -i (lowercase)
        assert!(!CommandOrchestrator::has_bulk_interactive_flag(&[
            "-i".to_string()
        ]));
    }

    #[test]
    fn test_has_force_flag() {
        assert!(CommandOrchestrator::has_force_flag(&["-f".to_string()]));
        assert!(CommandOrchestrator::has_force_flag(
            &["--force".to_string()]
        ));
        assert!(CommandOrchestrator::has_force_flag(&["-rf".to_string()]));
        assert!(!CommandOrchestrator::has_force_flag(&["-i".to_string()]));
        assert!(!CommandOrchestrator::has_force_flag(&["-r".to_string()]));
    }

    #[test]
    fn test_has_recursive_flag() {
        assert!(CommandOrchestrator::has_recursive_flag(&["-r".to_string()]));
        assert!(CommandOrchestrator::has_recursive_flag(&["-R".to_string()]));
        assert!(CommandOrchestrator::has_recursive_flag(&[
            "--recursive".to_string()
        ]));
        assert!(CommandOrchestrator::has_recursive_flag(
            &["-rf".to_string()]
        ));
        assert!(!CommandOrchestrator::has_recursive_flag(
            &["-i".to_string()]
        ));
    }

    // ==================== Detection Function Tests ====================

    #[test]
    fn test_needs_rm_interactive_confirmation() {
        // With -i flag and files
        let args = vec![
            "-i".to_string(),
            "file1.txt".to_string(),
            "file2.txt".to_string(),
        ];
        let result = CommandOrchestrator::needs_rm_interactive_confirmation(&args);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), vec!["file1.txt", "file2.txt"]);

        // Without -i flag
        let args = vec!["file1.txt".to_string()];
        assert!(CommandOrchestrator::needs_rm_interactive_confirmation(&args).is_none());

        // With -f flag (should skip)
        let args = vec!["-if".to_string(), "file1.txt".to_string()];
        assert!(CommandOrchestrator::needs_rm_interactive_confirmation(&args).is_none());
    }

    #[test]
    fn test_needs_rm_bulk_confirmation() {
        // With -I and >3 files
        let args = vec![
            "-I".to_string(),
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let result = CommandOrchestrator::needs_rm_bulk_confirmation(&args);
        assert!(result.is_some());
        let (count, recursive) = result.unwrap();
        assert_eq!(count, 4);
        assert!(!recursive);

        // With -I and recursive
        let args = vec!["-rI".to_string(), "dir".to_string()];
        let result = CommandOrchestrator::needs_rm_bulk_confirmation(&args);
        assert!(result.is_some());
        let (_, recursive) = result.unwrap();
        assert!(recursive);

        // With -I but <=3 files and not recursive (no prompt needed)
        let args = vec!["-I".to_string(), "a".to_string(), "b".to_string()];
        assert!(CommandOrchestrator::needs_rm_bulk_confirmation(&args).is_none());

        // With -f flag (should skip)
        let args = vec!["-If".to_string(), "a".to_string(), "b".to_string()];
        assert!(CommandOrchestrator::needs_rm_bulk_confirmation(&args).is_none());
    }

    #[test]
    fn test_get_file_type_description() {
        // For non-existent files, returns "file"
        assert_eq!(
            CommandOrchestrator::get_file_type_description("/nonexistent/path"),
            "file"
        );
        // For existing directory
        assert_eq!(
            CommandOrchestrator::get_file_type_description("/tmp"),
            "directory"
        );
    }

    #[test]
    fn test_remove_flag_from_command() {
        // Remove -I from command
        let cmd = "rm -rI file1 file2";
        let result = CommandOrchestrator::remove_flag_from_command(cmd, "-rI");
        assert_eq!(result, "rm file1 file2");

        // Remove standalone flag
        let cmd = "rm -I -r file1";
        let result = CommandOrchestrator::remove_flag_from_command(cmd, "-I");
        assert_eq!(result, "rm -r file1");
    }
}
