//! Main application struct implementing eframe::App.

use crate::llm::{LLMClient, LLMQueryResult};
use crate::pty::{PtyManager, PtyWriter};
use crate::state::AppMode;
use crate::ui::{PromptConfig, Theme};
use egui::{FontId, Key, TextFormat, ViewportCommand};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;

/// A line in the terminal output.
#[derive(Debug, Clone)]
struct TerminalLine {
    /// The text content
    text: String,
    /// Line type for coloring
    line_type: LineType,
}

/// Type of terminal line for coloring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LineType {
    /// Prompt line (with command)
    Prompt,
    /// Command output
    Output,
    /// LLM response
    LlmResponse,
    /// Error message
    Error,
}

/// Main terminal application.
pub struct InfrawareApp {
    /// Current application mode
    mode: AppMode,

    /// Theme configuration
    theme: Theme,

    /// Prompt configuration
    prompt_config: PromptConfig,

    /// Terminal output lines
    lines: Vec<TerminalLine>,

    /// Current input text
    input_buffer: String,

    /// Command history
    history: Vec<String>,

    /// Position in history navigation
    history_position: Option<usize>,

    /// Current working directory (tracked separately from shell)
    cwd: String,

    /// Scroll to bottom flag
    scroll_to_bottom: bool,

    /// PTY writer for sending input
    pty_writer: Option<Arc<PtyWriter>>,

    /// PTY output receiver channel
    pty_output_rx: Option<mpsc::Receiver<Vec<u8>>>,

    /// PTY manager for resize
    pty_manager: Option<Arc<TokioMutex<PtyManager>>>,

    /// Current terminal size (cols, rows)
    terminal_size: (u16, u16),

    /// Last output timestamp
    last_output_time: Instant,

    /// Pending command for "command not found" detection
    pending_command: Option<String>,

    /// Last command sent (to filter echo)
    last_sent_command: Option<String>,

    /// Flag to quit application
    should_quit: bool,

    /// Tokio runtime
    runtime: Runtime,

    /// Raw output buffer for parsing
    output_buffer: String,

    /// Startup time (to skip initial shell output)
    startup_time: Instant,

    /// LLM client
    llm_client: Arc<LLMClient>,

    /// LLM response channel
    llm_response_rx: Option<mpsc::Receiver<Result<LLMQueryResult, String>>>,

    /// Pending approval command
    pending_approval: Option<String>,

    /// Waiting for PTY command to complete
    waiting_for_pty: bool,

    /// Password input mode (hide typed characters)
    password_input: bool,

    /// Interactive prompt from PTY (e.g., password prompt)
    interactive_prompt: Option<String>,

    /// Root mode (sudo su successful)
    is_root_mode: bool,

    /// Cursor blink state
    cursor_visible: bool,

    /// Last cursor blink time
    last_cursor_blink: Instant,

    /// Theme applied flag
    theme_applied: bool,
}

impl std::fmt::Debug for InfrawareApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareApp")
            .field("mode", &self.mode)
            .field("input_buffer", &self.input_buffer)
            .field("lines", &self.lines.len())
            .finish()
    }
}

impl InfrawareApp {
    /// Create a new application instance.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        let theme = Theme::dark();
        let prompt_config = PromptConfig::from_environment();

        // Get initial CWD
        let cwd = prompt_config.get_cwd();

        // Initialize PTY with no prompt (we render our own)
        let (pty_writer, pty_output_rx, pty_manager) = runtime.block_on(async {
            match PtyManager::new().await {
                Ok(mut manager) => {
                    log::info!("PTY initialized with shell: {}", manager.shell());

                    let writer = manager.take_writer().await.ok();
                    let reader = manager.take_reader().await.ok();

                    let (tx, rx) = mpsc::channel();

                    if let Some(mut pty_reader) = reader {
                        std::thread::spawn(move || {
                            let rt = Runtime::new().unwrap();
                            rt.block_on(async {
                                loop {
                                    match pty_reader
                                        .read_with_timeout(Duration::from_millis(50))
                                        .await
                                    {
                                        Ok(data) if !data.is_empty() => {
                                            if tx.send(data).is_err() {
                                                break;
                                            }
                                        }
                                        Ok(_) => {}
                                        Err(_) => break,
                                    }
                                }
                            });
                        });
                    }

                    let manager = Arc::new(TokioMutex::new(manager));
                    (writer, Some(rx), Some(manager))
                }
                Err(e) => {
                    log::error!("Failed to initialize PTY: {}", e);
                    (None, None, None)
                }
            }
        });

        let llm_client = Arc::new(LLMClient::new());

        Self {
            mode: AppMode::Normal,
            theme,
            prompt_config,
            lines: Vec::new(),
            input_buffer: String::new(),
            history: Vec::new(),
            history_position: None,
            cwd,
            scroll_to_bottom: true,
            pty_writer,
            pty_output_rx,
            pty_manager,
            terminal_size: (80, 24),
            last_output_time: Instant::now(),
            pending_command: None,
            last_sent_command: None,
            should_quit: false,
            runtime,
            output_buffer: String::new(),
            startup_time: Instant::now(),
            llm_client,
            llm_response_rx: None,
            pending_approval: None,
            waiting_for_pty: false,
            password_input: false,
            interactive_prompt: None,
            is_root_mode: false,
            cursor_visible: true,
            last_cursor_blink: Instant::now(),
            theme_applied: false,
        }
    }

    /// Get current prompt string.
    fn get_prompt(&self) -> String {
        let (username, symbol) = if self.is_root_mode || self.prompt_config.is_root {
            ("root", "#")
        } else {
            (self.prompt_config.username.as_str(), "$")
        };
        format!(
            "{} {}@{}:{}{}",
            self.prompt_config.prefix,
            username,
            self.prompt_config.hostname,
            self.cwd,
            symbol
        )
    }

    /// Add a prompt line with command.
    fn add_prompt_line(&mut self, command: &str) {
        let prompt = self.get_prompt();
        self.lines.push(TerminalLine {
            text: format!("{} {}", prompt, command),
            line_type: LineType::Prompt,
        });
    }

    /// Add output lines.
    fn add_output(&mut self, text: &str) {
        for line in text.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            let lower = trimmed.to_lowercase();

            // Filter out echo of the command we just sent
            if let Some(ref last_cmd) = self.last_sent_command {
                if trimmed == last_cmd {
                    self.last_sent_command = None; // Only filter once
                    continue;
                }
            }

            // Filter shell prompts (user@host:path$ or user@host:path#)
            // This also detects command completion
            if self.is_shell_prompt(trimmed) {
                // Command completed - exit waiting mode
                self.waiting_for_pty = false;
                self.password_input = false;
                self.interactive_prompt = None;

                // Detect root mode from prompt
                if trimmed.ends_with('#') || trimmed.ends_with("# ") {
                    if trimmed.contains("root@") {
                        self.is_root_mode = true;
                    }
                } else if trimmed.ends_with('$') || trimmed.ends_with("$ ") {
                    self.is_root_mode = false;
                }
                continue; // Don't show shell prompt in output
            }

            // Detect password/passphrase prompts - store as interactive prompt
            if lower.contains("password") || lower.contains("passphrase") {
                self.password_input = true;
                self.interactive_prompt = Some(trimmed.to_string());
                continue; // Don't add to lines, will be rendered inline with cursor
            }

            // Filter common shell noise
            if trimmed.starts_with("export ")
                || trimmed.starts_with("PS1=")
                || trimmed.starts_with("stty ")
            {
                continue;
            }

            self.lines.push(TerminalLine {
                text: line.to_string(),
                line_type: LineType::Output,
            });
        }
        self.scroll_to_bottom = true;
    }

    /// Check if a line is a root shell prompt.
    fn is_root_shell_prompt(&self, line: &str) -> bool {
        let trimmed = line.trim();
        // Root prompts end with # and typically have user@host pattern
        (trimmed.ends_with("#") || trimmed.ends_with("# "))
            && (trimmed.contains("@") || trimmed.starts_with("root@") || trimmed == "#")
    }

    /// Check if a line looks like a shell prompt.
    fn is_shell_prompt(&self, line: &str) -> bool {
        let trimmed = line.trim();

        // Skip export commands that might leak through
        if trimmed.starts_with("export ") || trimmed.starts_with("PS1=") || trimmed.starts_with("PS2=") {
            return true;
        }

        // A shell prompt must have user@host pattern with $ or #
        // This is more precise than just checking endings
        if trimmed.contains('@') {
            // Pattern: user@host:path$ or user@host:path#
            let ends_with_prompt = trimmed.ends_with('$')
                || trimmed.ends_with("$ ")
                || trimmed.ends_with('#')
                || trimmed.ends_with("# ");

            // Must also have : for path separator (typical prompt format)
            if ends_with_prompt && trimmed.contains(':') {
                return true;
            }
        }

        false
    }

    /// Add LLM response line.
    fn add_llm_line(&mut self, text: &str) {
        let prefix = &self.prompt_config.prefix;
        for line in text.lines() {
            self.lines.push(TerminalLine {
                text: format!("{} {}", prefix, line),
                line_type: LineType::LlmResponse,
            });
        }
        self.scroll_to_bottom = true;
    }

    /// Poll PTY output.
    fn poll_pty_output(&mut self) {
        // Skip output during first 500ms (shell startup noise)
        let skip_startup = self.startup_time.elapsed() < Duration::from_millis(500);

        if let Some(ref rx) = self.pty_output_rx {
            while let Ok(bytes) = rx.try_recv() {
                if !skip_startup {
                    if let Ok(text) = String::from_utf8(bytes.clone()) {
                        self.output_buffer.push_str(&text);
                    }
                }
                self.last_output_time = Instant::now();
            }
        }

        // Process output buffer when stable
        if !self.output_buffer.is_empty()
            && self.last_output_time.elapsed() > Duration::from_millis(50)
        {
            let output = std::mem::take(&mut self.output_buffer);
            // Strip ANSI escape codes for now (simple implementation)
            let clean_output = strip_ansi(&output);
            if !clean_output.trim().is_empty() {
                self.add_output(&clean_output);
            }

            // Check for "command not found"
            if self.pending_command.is_some() {
                if output.contains(": command not found")
                    || output.contains(": not found")
                    || output.contains("No such file or directory")
                {
                    let cmd = self.pending_command.take().unwrap();
                    self.query_llm(cmd);
                } else {
                    self.pending_command = None;
                }
            }

            // Update CWD if cd command was used
            self.cwd = self.prompt_config.get_cwd();
        }
    }

    /// Query LLM for failed command.
    fn query_llm(&mut self, command: String) {
        let client = self.llm_client.clone();
        let (tx, rx) = mpsc::channel();

        self.llm_response_rx = Some(rx);
        self.mode = AppMode::WaitingLLM;

        self.runtime.spawn(async move {
            let result = client.query_failed_command(&command).await;
            let _ = tx.send(result.map_err(|e| e.to_string()));
        });
    }

    /// Poll LLM response.
    fn poll_llm_response(&mut self) {
        if let Some(ref rx) = self.llm_response_rx {
            if let Ok(result) = rx.try_recv() {
                self.llm_response_rx = None;

                match result {
                    Ok(LLMQueryResult::Complete(response)) => {
                        self.add_llm_line(&response);
                        self.mode = AppMode::Normal;
                    }
                    Ok(LLMQueryResult::CommandApproval { command, message }) => {
                        self.add_llm_line(&message);
                        self.add_llm_line(&format!("    command: {}", command));
                        self.add_llm_line("    Do you want to execute this command (y/n)?");
                        self.pending_approval = Some(command.clone());
                        self.mode = AppMode::AwaitingApproval { command, message };
                    }
                    Ok(LLMQueryResult::Question { question, options }) => {
                        self.add_llm_line(&question);
                        if let Some(opts) = options {
                            for (i, opt) in opts.iter().enumerate() {
                                self.add_llm_line(&format!("  {}: {}", i + 1, opt));
                            }
                        }
                        self.mode = AppMode::AwaitingAnswer {
                            question,
                            options: None,
                        };
                    }
                    Err(e) => {
                        self.lines.push(TerminalLine {
                            text: format!("Error: {}", e),
                            line_type: LineType::Error,
                        });
                        self.mode = AppMode::Normal;
                    }
                }
            }
        }
    }

    /// Handle approval.
    fn handle_approval(&mut self, approved: bool) {
        if let Some(command) = self.pending_approval.take() {
            if approved {
                self.add_prompt_line(&command);
                self.send_to_pty(&command);
            } else {
                self.add_llm_line("Command cancelled.");
            }
        }
        self.mode = AppMode::Normal;
    }

    /// Send input to PTY.
    fn send_to_pty(&mut self, input: &str) {
        // Track command to filter echo
        if !input.is_empty() {
            self.last_sent_command = Some(input.to_string());
        }

        if let Some(ref writer) = self.pty_writer {
            let data = format!("{}\n", input);
            let writer = writer.clone();
            self.runtime.spawn(async move {
                if let Err(e) = writer.write(data.as_bytes()).await {
                    log::error!("Failed to write to PTY: {}", e);
                }
            });
        }
    }

    /// Resize PTY to match window size.
    fn resize_pty(&mut self, cols: u16, rows: u16) {
        if self.terminal_size != (cols, rows) {
            self.terminal_size = (cols, rows);
            if let Some(ref manager) = self.pty_manager {
                let manager = manager.clone();
                self.runtime.spawn(async move {
                    let mut mgr = manager.lock().await;
                    if let Err(e) = mgr.resize(rows, cols).await {
                        log::error!("Failed to resize PTY: {}", e);
                    }
                });
            }
        }
    }

    /// Send Ctrl+C to PTY.
    fn send_interrupt(&self) {
        if let Some(ref writer) = self.pty_writer {
            let writer = writer.clone();
            self.runtime.spawn(async move {
                if let Err(e) = writer.send_interrupt().await {
                    log::error!("Failed to send interrupt: {}", e);
                }
            });
        }
    }

    /// Submit input.
    fn submit_input(&mut self) {
        let input = self.input_buffer.trim().to_string();
        self.input_buffer.clear();

        // If waiting for PTY (command is running), send directly without showing prompt
        if self.waiting_for_pty {
            // Send input directly to PTY (could be password, interactive input, etc.)
            self.send_to_pty(&input);
            // After sending password, clear the interactive prompt display
            if self.password_input {
                self.interactive_prompt = None;
            }
            return;
        }

        if input.is_empty() {
            // Just show new prompt
            self.add_prompt_line("");
            return;
        }

        // Check for exit
        if input == "exit" {
            // If in root mode, exit root first
            if self.is_root_mode {
                self.add_prompt_line(&input);
                self.send_to_pty("exit");
                self.waiting_for_pty = true;
                return;
            }
            self.should_quit = true;
            return;
        }

        // Handle clear command locally
        if input == "clear" {
            self.lines.clear();
            return;
        }

        // Handle cd command - also change our process directory
        if input == "cd" || input.starts_with("cd ") {
            let path_str = if input == "cd" {
                "~".to_string()
            } else {
                input.strip_prefix("cd ").unwrap().trim().to_string()
            };

            // Handle special cases
            if path_str == "-" {
                // cd - not supported locally, just send to PTY
                self.add_prompt_line(&input);
                self.pending_command = Some(input.clone());
                self.waiting_for_pty = true;
                self.send_to_pty(&input);
                return;
            }

            // Expand ~ to home directory
            let expanded = if path_str == "~" {
                dirs::home_dir().unwrap_or_default()
            } else if path_str.starts_with("~/") {
                dirs::home_dir().unwrap_or_default().join(&path_str[2..])
            } else if path_str.starts_with('/') {
                std::path::PathBuf::from(&path_str)
            } else {
                // Relative path - join with current dir
                std::env::current_dir().unwrap_or_default().join(&path_str)
            };

            // Canonicalize to resolve .. and symlinks
            let target = expanded.canonicalize().unwrap_or(expanded);

            // Try to change directory
            if target.is_dir() && std::env::set_current_dir(&target).is_ok() {
                self.cwd = self.prompt_config.get_cwd();
                self.add_prompt_line(&input);
                // Also send to PTY so it's in sync
                self.send_to_pty(&input);
                self.waiting_for_pty = true;
            } else {
                self.add_prompt_line(&input);
                self.lines.push(TerminalLine {
                    text: format!("cd: {}: No such file or directory", path_str),
                    line_type: LineType::Error,
                });
            }
            return;
        }

        // Add to history
        self.history.push(input.clone());
        self.history_position = None;

        // Add prompt line with command
        self.add_prompt_line(&input);

        // Track for "command not found"
        self.pending_command = Some(input.clone());

        // Enter waiting for PTY mode
        self.waiting_for_pty = true;

        // Send to PTY
        self.send_to_pty(&input);
    }

    /// Handle keyboard input.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        ctx.input(|i| {
            // Handle special keys first
            if i.modifiers.ctrl {
                if i.key_pressed(Key::C) {
                    self.send_interrupt();
                    self.input_buffer.clear();
                    return;
                }
                if i.key_pressed(Key::D) && self.input_buffer.is_empty() {
                    self.should_quit = true;
                    return;
                }
                if i.key_pressed(Key::L) {
                    // Clear screen
                    self.lines.clear();
                    return;
                }
            }

            // Handle Enter
            if i.key_pressed(Key::Enter) {
                match &self.mode {
                    AppMode::Normal => self.submit_input(),
                    AppMode::AwaitingApproval { .. } => {
                        let input = self.input_buffer.trim().to_lowercase();
                        self.input_buffer.clear();
                        if input == "y" || input == "yes" {
                            self.handle_approval(true);
                        } else if input == "n" || input == "no" {
                            self.handle_approval(false);
                        }
                    }
                    AppMode::AwaitingAnswer { .. } => {
                        let answer = self.input_buffer.trim().to_string();
                        if !answer.is_empty() {
                            self.input_buffer.clear();
                            // Send answer to LLM
                            let client = self.llm_client.clone();
                            let (tx, rx) = mpsc::channel();
                            self.llm_response_rx = Some(rx);
                            self.mode = AppMode::WaitingLLM;
                            self.runtime.spawn(async move {
                                let result = client.resume_with_answer(&answer).await;
                                let _ = tx.send(result.map_err(|e| e.to_string()));
                            });
                        }
                    }
                    AppMode::WaitingLLM => {}
                }
                self.scroll_to_bottom = true;
                return;
            }

            // History navigation
            if i.key_pressed(Key::ArrowUp) && !self.history.is_empty() {
                match self.history_position {
                    None => self.history_position = Some(self.history.len() - 1),
                    Some(pos) if pos > 0 => self.history_position = Some(pos - 1),
                    _ => {}
                }
                if let Some(pos) = self.history_position {
                    self.input_buffer = self.history[pos].clone();
                }
                self.scroll_to_bottom = true;
                return;
            }

            if i.key_pressed(Key::ArrowDown) {
                if let Some(pos) = self.history_position {
                    if pos + 1 < self.history.len() {
                        self.history_position = Some(pos + 1);
                        self.input_buffer = self.history[pos + 1].clone();
                    } else {
                        self.history_position = None;
                        self.input_buffer.clear();
                    }
                }
                self.scroll_to_bottom = true;
                return;
            }

            // Backspace
            if i.key_pressed(Key::Backspace) {
                self.input_buffer.pop();
                self.scroll_to_bottom = true;
                return;
            }

            // Space
            if i.key_pressed(Key::Space) {
                self.input_buffer.push(' ');
                self.scroll_to_bottom = true;
                return;
            }

            // Tab (insert spaces)
            if i.key_pressed(Key::Tab) {
                self.input_buffer.push_str("    ");
                self.scroll_to_bottom = true;
                return;
            }

            // Handle printable characters via Key events
            // This is a workaround because egui doesn't generate Text events without a focused widget
            let shift = i.modifiers.shift;
            for key in &[
                Key::A, Key::B, Key::C, Key::D, Key::E, Key::F, Key::G, Key::H, Key::I,
                Key::J, Key::K, Key::L, Key::M, Key::N, Key::O, Key::P, Key::Q, Key::R,
                Key::S, Key::T, Key::U, Key::V, Key::W, Key::X, Key::Y, Key::Z,
            ] {
                if i.key_pressed(*key) {
                    let c = format!("{:?}", key).chars().last().unwrap();
                    let c = if shift { c } else { c.to_ascii_lowercase() };
                    self.input_buffer.push(c);
                    self.scroll_to_bottom = true;
                }
            }

            // Numbers
            for (key, normal, shifted) in &[
                (Key::Num0, '0', ')'), (Key::Num1, '1', '!'), (Key::Num2, '2', '@'),
                (Key::Num3, '3', '#'), (Key::Num4, '4', '$'), (Key::Num5, '5', '%'),
                (Key::Num6, '6', '^'), (Key::Num7, '7', '&'), (Key::Num8, '8', '*'),
                (Key::Num9, '9', '('),
            ] {
                if i.key_pressed(*key) {
                    let c = if shift { *shifted } else { *normal };
                    self.input_buffer.push(c);
                    self.scroll_to_bottom = true;
                }
            }

            // Common symbols
            for (key, normal, shifted) in &[
                (Key::Minus, '-', '_'),
                (Key::Plus, '=', '+'),
                (Key::OpenBracket, '[', '{'),
                (Key::CloseBracket, ']', '}'),
                (Key::Backslash, '\\', '|'),
                (Key::Semicolon, ';', ':'),
                (Key::Quote, '\'', '"'),
                (Key::Comma, ',', '<'),
                (Key::Period, '.', '>'),
                (Key::Slash, '/', '?'),
                (Key::Backtick, '`', '~'),
            ] {
                if i.key_pressed(*key) {
                    let c = if shift { *shifted } else { *normal };
                    self.input_buffer.push(c);
                    self.scroll_to_bottom = true;
                }
            }
        });
    }

    /// Render terminal content.
    fn render_terminal(&mut self, ui: &mut egui::Ui) {
        let font_id = FontId::monospace(14.0);
        let should_scroll = self.scroll_to_bottom;

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                ui.set_min_width(ui.available_width());

                // Render all lines
                for line in &self.lines {
                    let mut job = egui::text::LayoutJob::default();

                    let color = match line.line_type {
                        LineType::Prompt => self.theme.prompt_path,
                        LineType::Output => self.theme.text,
                        LineType::LlmResponse => self.theme.llm_response,
                        LineType::Error => self.theme.error,
                    };

                    // For prompt lines, color the prompt part differently
                    if line.line_type == LineType::Prompt {
                        // Split at the prompt end ($ or #)
                        if let Some(idx) = line.text.find('$').or_else(|| line.text.find('#')) {
                            let prompt_part = &line.text[..=idx];
                            let cmd_part = &line.text[idx + 1..];

                            // Prompt in green
                            job.append(
                                prompt_part,
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    color: self.theme.prompt_path,
                                    ..Default::default()
                                },
                            );

                            // Command in white
                            job.append(
                                cmd_part,
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    color: self.theme.text,
                                    ..Default::default()
                                },
                            );
                        } else {
                            job.append(
                                &line.text,
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    color,
                                    ..Default::default()
                                },
                            );
                        }
                    } else {
                        job.append(
                            &line.text,
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color,
                                ..Default::default()
                            },
                        );
                    }

                    ui.label(job);
                }

                // Render current input line (prompt + input + cursor)
                // When waiting for PTY (command running), show interactive prompt with inline cursor
                let response = if self.waiting_for_pty {
                    let mut job = egui::text::LayoutJob::default();

                    // If there's an interactive prompt (e.g., password prompt), show it
                    if let Some(ref prompt) = self.interactive_prompt {
                        job.append(
                            prompt,
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color: self.theme.text,
                                ..Default::default()
                            },
                        );
                        // Add space after prompt if it doesn't end with space
                        if !prompt.ends_with(' ') {
                            job.append(
                                " ",
                                0.0,
                                TextFormat {
                                    font_id: font_id.clone(),
                                    color: self.theme.text,
                                    ..Default::default()
                                },
                            );
                        }
                    }

                    // Only show input if not in password mode
                    if !self.password_input && !self.input_buffer.is_empty() {
                        job.append(
                            &self.input_buffer,
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color: self.theme.text,
                                ..Default::default()
                            },
                        );
                    }

                    // Show thin line cursor | instead of block
                    if self.cursor_visible {
                        job.append(
                            "|",
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color: self.theme.cursor,
                                ..Default::default()
                            },
                        );
                    }
                    ui.label(job)
                } else if self.mode != AppMode::WaitingLLM {
                    let mut job = egui::text::LayoutJob::default();

                    // Prompt
                    let prompt = self.get_prompt();
                    job.append(
                        &format!("{} ", prompt),
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color: self.theme.prompt_path,
                            ..Default::default()
                        },
                    );

                    // Input text
                    job.append(
                        &self.input_buffer,
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color: self.theme.text,
                            ..Default::default()
                        },
                    );

                    // Cursor
                    if self.cursor_visible {
                        job.append(
                            "█",
                            0.0,
                            TextFormat {
                                font_id: font_id.clone(),
                                color: self.theme.cursor,
                                ..Default::default()
                            },
                        );
                    }

                    ui.label(job)
                } else {
                    // Waiting for LLM - show spinner
                    let mut job = egui::text::LayoutJob::default();
                    let prefix = &self.prompt_config.prefix;
                    job.append(
                        &format!("{} ", prefix),
                        0.0,
                        TextFormat {
                            font_id: font_id.clone(),
                            color: self.theme.prompt_prefix,
                            ..Default::default()
                        },
                    );

                    ui.horizontal(|ui| {
                        ui.label(job);
                        ui.spinner();
                    }).response
                };

                // Always scroll to the input line
                if should_scroll {
                    response.scroll_to_me(Some(egui::Align::BOTTOM));
                }
            });

        self.scroll_to_bottom = false;
    }
}

impl eframe::App for InfrawareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme once
        if !self.theme_applied {
            self.theme.apply(ctx);
            self.theme_applied = true;
        }

        // Check for quit
        if self.should_quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        // Update cursor blink
        if self.last_cursor_blink.elapsed() > Duration::from_millis(530) {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_blink = Instant::now();
        }

        // Poll PTY output (always, to detect command completion)
        self.poll_pty_output();

        // Poll LLM response
        if self.mode == AppMode::WaitingLLM {
            self.poll_llm_response();
        }

        // Handle keyboard
        self.handle_keyboard(ctx);

        // Render UI
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(self.theme.background))
            .show(ctx, |ui| {
                ui.set_min_size(ui.available_size());

                // Calculate terminal size based on available space and font size
                let char_width = 8.4; // Approximate monospace char width at 14pt
                let char_height = 18.0; // Approximate line height at 14pt
                let available = ui.available_size();
                let cols = ((available.x / char_width) as u16).max(20);
                let rows = ((available.y / char_height) as u16).max(5);
                self.resize_pty(cols, rows);

                self.render_terminal(ui);
            });

        // Request repaint
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}

/// Strip ANSI escape codes from text.
fn strip_ansi(text: &str) -> String {
    let mut result = String::new();
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip escape sequence
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // Skip until we hit a letter
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else if c == '\r' {
            // Skip carriage return
        } else {
            result.push(c);
        }
    }

    result
}
