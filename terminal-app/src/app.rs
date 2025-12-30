//! Main application struct implementing eframe::App with full terminal emulation.

use crate::llm::{LLMClient, LLMQueryResult};
use crate::pty::{PtyManager, PtyWriter};
use crate::state::AppMode;
use crate::terminal::{Color, TerminalHandler};
use crate::ui::{PromptConfig, Theme};
use egui::{Color32, FontFamily, FontId, Key, Pos2, Rect, Sense, Stroke, Vec2, ViewportCommand};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;

/// Main terminal application with VTE-based terminal emulation.
pub struct InfrawareApp {
    /// Current application mode
    mode: AppMode,

    /// Theme configuration
    theme: Theme,

    /// Prompt configuration
    prompt_config: PromptConfig,

    /// VTE parser for escape sequences
    vte_parser: vte::Parser,

    /// Terminal handler with grid state
    terminal_handler: TerminalHandler,

    /// PTY writer for sending input
    pty_writer: Option<Arc<PtyWriter>>,

    /// PTY output receiver channel
    pty_output_rx: Option<mpsc::Receiver<Vec<u8>>>,

    /// PTY manager for resize
    pty_manager: Option<Arc<TokioMutex<PtyManager>>>,

    /// Current terminal size (cols, rows)
    terminal_size: (u16, u16),

    /// Flag to quit application
    should_quit: bool,

    /// Tokio runtime
    runtime: Runtime,

    /// LLM client
    llm_client: Arc<LLMClient>,

    /// LLM response channel
    llm_response_rx: Option<mpsc::Receiver<Result<LLMQueryResult, String>>>,

    /// Pending approval command
    pending_approval: Option<String>,

    /// Theme applied flag
    theme_applied: bool,

    /// Font metrics
    char_width: f32,
    char_height: f32,

    /// Last resize time for debouncing
    last_resize: Instant,

    /// Shell initialization done
    shell_initialized: bool,

    /// Startup time for delayed init
    startup_time: Instant,

    /// Cursor blink state
    cursor_blink_visible: bool,

    /// Last cursor blink toggle time
    last_cursor_blink: Instant,

}

impl std::fmt::Debug for InfrawareApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareApp")
            .field("mode", &self.mode)
            .field("terminal_size", &self.terminal_size)
            .finish()
    }
}

impl InfrawareApp {
    /// Create a new application instance.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        let theme = Theme::dark();
        let prompt_config = PromptConfig::from_environment();

        // Default terminal size
        let (rows, cols) = (24_u16, 80_u16);

        // Initialize PTY
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
                                        .read_with_timeout(Duration::from_millis(16))
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
            vte_parser: vte::Parser::new(),
            terminal_handler: TerminalHandler::new(rows, cols),
            pty_writer,
            pty_output_rx,
            pty_manager,
            terminal_size: (cols, rows),
            should_quit: false,
            runtime,
            llm_client,
            llm_response_rx: None,
            pending_approval: None,
            theme_applied: false,
            char_width: 8.4,
            char_height: 16.0,
            last_resize: Instant::now(),
            shell_initialized: false,
            startup_time: Instant::now(),
            cursor_blink_visible: true,
            last_cursor_blink: Instant::now(),
        }
    }

    /// Initialize shell with custom prompt after startup.
    fn initialize_shell(&mut self) {
        if self.shell_initialized {
            return;
        }

        // Wait for shell to fully initialize (500ms)
        if self.startup_time.elapsed() < Duration::from_millis(500) {
            return;
        }

        self.shell_initialized = true;

        // Clear the terminal and set custom PS1 with colors
        // \e[32m = green, \e[0m = reset
        // Format: |~| (green) user@host:path$ (reset)
        let init_commands = concat!(
            "export PS1='\\[\\e[32m\\]|~| \\u@\\h:\\w\\$\\[\\e[0m\\] '\n",
            "clear\n"
        );

        self.send_to_pty(init_commands.as_bytes());
        log::info!("Shell initialized with custom prompt");
    }

    /// Poll PTY output and feed to VTE parser.
    /// Limited to MAX_BYTES_PER_FRAME to keep UI responsive during heavy output.
    fn poll_pty_output(&mut self) {
        // Limit bytes per frame to ensure keyboard input remains responsive
        // With cat /dev/zero, unlimited processing would starve input handling
        const MAX_BYTES_PER_FRAME: usize = 16384; // 16KB per frame (~1MB/s at 60fps)
        let mut bytes_processed = 0;

        if let Some(ref rx) = self.pty_output_rx {
            loop {
                // Stop if we've processed enough this frame
                if bytes_processed >= MAX_BYTES_PER_FRAME {
                    break;
                }

                match rx.try_recv() {
                    Ok(bytes) => {
                        bytes_processed += bytes.len();
                        // Feed each byte to the VTE parser
                        for byte in bytes {
                            self.vte_parser.advance(&mut self.terminal_handler, byte);
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => {
                        // No more data available right now
                        break;
                    }
                    Err(mpsc::TryRecvError::Disconnected) => {
                        // Shell exited - quit the application
                        log::info!("Shell exited, quitting application");
                        self.should_quit = true;
                        break;
                    }
                }
            }
        }
    }

    /// Send data to PTY synchronously (ensures immediate delivery).
    fn send_to_pty(&self, data: &[u8]) {
        if let Some(ref writer) = self.pty_writer {
            log::debug!("Writing {} bytes to PTY: {:?}", data.len(), data);
            match writer.write_sync(data) {
                Ok(n) => log::debug!("Wrote {} bytes to PTY", n),
                Err(e) => log::error!("Failed to write to PTY: {}", e),
            }
        } else {
            log::warn!("No PTY writer available!");
        }
    }

    /// Send string to PTY.
    fn send_string_to_pty(&self, s: &str) {
        self.send_to_pty(s.as_bytes());
    }

    /// Send SIGINT to the foreground process group (non-blocking).
    /// This bypasses PTY buffers and directly signals the process.
    fn send_sigint(&self) {
        if let Some(ref manager) = self.pty_manager {
            // Use try_lock to avoid blocking - if locked, skip this frame
            if let Ok(mgr) = manager.try_lock() {
                if let Err(e) = mgr.send_sigint() {
                    log::error!("Failed to send SIGINT: {}", e);
                }
            } else {
                log::warn!("Could not lock PTY manager for SIGINT, will retry next frame");
            }
        }
    }

    /// Resize PTY to match window size.
    fn resize_pty(&mut self, cols: u16, rows: u16) {
        if self.terminal_size != (cols, rows) && self.last_resize.elapsed() > Duration::from_millis(100) {
            self.terminal_size = (cols, rows);
            self.last_resize = Instant::now();

            // Resize terminal handler
            self.terminal_handler.resize(rows, cols);

            // Resize PTY
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

    /// Query LLM for natural language input.
    fn query_llm(&mut self, query: String) {
        let client = self.llm_client.clone();
        let (tx, rx) = mpsc::channel();

        self.llm_response_rx = Some(rx);
        self.mode = AppMode::WaitingLLM;

        self.runtime.spawn(async move {
            let result = client.query_failed_command(&query).await;
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
                        // Send LLM response to terminal as output
                        let output = format!("\r\n{}\r\n", response);
                        for byte in output.bytes() {
                            self.vte_parser.advance(&mut self.terminal_handler, byte);
                        }
                        self.mode = AppMode::Normal;
                    }
                    Ok(LLMQueryResult::CommandApproval { command, message }) => {
                        let output = format!(
                            "\r\n{}\r\n    command: {}\r\n    Execute? (y/n): ",
                            message, command
                        );
                        for byte in output.bytes() {
                            self.vte_parser.advance(&mut self.terminal_handler, byte);
                        }
                        self.pending_approval = Some(command.clone());
                        self.mode = AppMode::AwaitingApproval { command, message };
                    }
                    Ok(LLMQueryResult::Question { question, options }) => {
                        let mut output = format!("\r\n{}\r\n", question);
                        if let Some(opts) = &options {
                            for (i, opt) in opts.iter().enumerate() {
                                output.push_str(&format!("  {}: {}\r\n", i + 1, opt));
                            }
                        }
                        for byte in output.bytes() {
                            self.vte_parser.advance(&mut self.terminal_handler, byte);
                        }
                        self.mode = AppMode::AwaitingAnswer { question, options };
                    }
                    Err(e) => {
                        let output = format!("\r\nError: {}\r\n", e);
                        for byte in output.bytes() {
                            self.vte_parser.advance(&mut self.terminal_handler, byte);
                        }
                        self.mode = AppMode::Normal;
                    }
                }
            }
        }
    }

    /// Handle keyboard input.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Collect Ctrl+key bytes to send (to avoid borrow issues)
        let mut ctrl_bytes: Option<u8> = None;

        // Check for Ctrl key combinations by iterating events directly
        // This is more reliable on Linux than using modifiers + key_pressed
        ctx.input(|i| {
            for event in &i.events {
                // Log ALL key events to debug
                if let egui::Event::Key { key, pressed, modifiers, .. } = event {
                    log::debug!("Key event: {:?} pressed={} ctrl={} alt={} shift={}",
                        key, pressed, modifiers.ctrl, modifiers.alt, modifiers.shift);
                }

                // Handle Ctrl combinations - Ctrl+C accepts EITHER press OR release
                // because Linux/X11/Wayland often only sends release for Ctrl+C
                if let egui::Event::Key { key, pressed, modifiers, .. } = event {
                    if modifiers.ctrl && ctrl_bytes.is_none() {
                        // Ctrl+C: accept either press or release (Linux quirk)
                        // Other Ctrl keys: only accept press to avoid double-fire
                        let is_ctrl_c = *key == Key::C;
                        if is_ctrl_c || *pressed {
                            log::info!("Ctrl+{:?} detected (pressed={})", key, pressed);
                            ctrl_bytes = match key {
                                Key::C => Some(0xFF), // Special marker for SIGINT
                                Key::D => Some(0x04), // EOF
                                Key::L => Some(0x0C), // Clear screen
                                Key::A => Some(0x01), // Beginning of line
                                Key::E => Some(0x05), // End of line
                                Key::K => Some(0x0B), // Kill to end of line
                                Key::U => Some(0x15), // Kill to beginning of line
                                Key::W => Some(0x17), // Delete word backward
                                Key::R => Some(0x12), // Reverse search
                                Key::Z => Some(0x1A), // Suspend
                                _ => None,
                            };
                        }
                    }
                }
            }
        });

        // Handle Ctrl key if detected
        if let Some(byte) = ctrl_bytes {
            if byte == 0xFF {
                // Ctrl+C: just send ETX to PTY like a real terminal
                log::info!("Ctrl+C detected, sending ETX (0x03) to PTY");
                self.send_to_pty(&[0x03]);
            } else {
                log::info!("Sending Ctrl byte 0x{:02X} to PTY", byte);
                self.send_to_pty(&[byte]);
            }
            return;
        }

        // Fallback: also try the old method in case events don't work
        ctx.input(|i| {
            // Handle Ctrl combinations first
            if i.modifiers.ctrl {
                if i.key_pressed(Key::C) {
                    // Send SIGINT directly to process (bypasses blocked PTY)
                    log::info!("Ctrl+C via key_pressed fallback, sending SIGINT");
                    self.send_sigint();
                    return;
                }
                if i.key_pressed(Key::D) {
                    self.send_to_pty(&[0x04]);
                    return;
                }
                if i.key_pressed(Key::L) {
                    self.send_to_pty(&[0x0C]);
                    return;
                }
                if i.key_pressed(Key::A) {
                    self.send_to_pty(&[0x01]);
                    return;
                }
                if i.key_pressed(Key::E) {
                    self.send_to_pty(&[0x05]);
                    return;
                }
                if i.key_pressed(Key::K) {
                    self.send_to_pty(&[0x0B]);
                    return;
                }
                if i.key_pressed(Key::U) {
                    self.send_to_pty(&[0x15]);
                    return;
                }
                if i.key_pressed(Key::W) {
                    self.send_to_pty(&[0x17]);
                    return;
                }
                if i.key_pressed(Key::R) {
                    self.send_to_pty(&[0x12]);
                    return;
                }
                if i.key_pressed(Key::Z) {
                    self.send_to_pty(&[0x1A]); // Suspend
                    return;
                }
            }

            // Handle Alt combinations (Meta)
            if i.modifiers.alt {
                if i.key_pressed(Key::B) {
                    // Word backward
                    self.send_to_pty(b"\x1bb");
                    return;
                }
                if i.key_pressed(Key::F) {
                    // Word forward
                    self.send_to_pty(b"\x1bf");
                    return;
                }
                if i.key_pressed(Key::D) {
                    // Delete word forward
                    self.send_to_pty(b"\x1bd");
                    return;
                }
            }

            // Special keys - send escape sequences
            if i.key_pressed(Key::Enter) {
                self.send_to_pty(b"\r");
                return;
            }

            if i.key_pressed(Key::Backspace) {
                self.send_to_pty(&[0x7F]);
                return;
            }

            if i.key_pressed(Key::Tab) {
                self.send_to_pty(&[0x09]);
                return;
            }

            if i.key_pressed(Key::Escape) {
                self.send_to_pty(&[0x1B]);
                return;
            }

            // Arrow keys
            if i.key_pressed(Key::ArrowUp) {
                self.send_to_pty(b"\x1b[A");
                return;
            }
            if i.key_pressed(Key::ArrowDown) {
                self.send_to_pty(b"\x1b[B");
                return;
            }
            if i.key_pressed(Key::ArrowRight) {
                self.send_to_pty(b"\x1b[C");
                return;
            }
            if i.key_pressed(Key::ArrowLeft) {
                self.send_to_pty(b"\x1b[D");
                return;
            }

            // Home/End/PageUp/PageDown
            if i.key_pressed(Key::Home) {
                self.send_to_pty(b"\x1b[H");
                return;
            }
            if i.key_pressed(Key::End) {
                self.send_to_pty(b"\x1b[F");
                return;
            }
            if i.key_pressed(Key::PageUp) {
                self.send_to_pty(b"\x1b[5~");
                return;
            }
            if i.key_pressed(Key::PageDown) {
                self.send_to_pty(b"\x1b[6~");
                return;
            }

            // Insert/Delete
            if i.key_pressed(Key::Insert) {
                self.send_to_pty(b"\x1b[2~");
                return;
            }
            if i.key_pressed(Key::Delete) {
                self.send_to_pty(b"\x1b[3~");
                return;
            }

            // Function keys
            if i.key_pressed(Key::F1) {
                self.send_to_pty(b"\x1bOP");
                return;
            }
            if i.key_pressed(Key::F2) {
                self.send_to_pty(b"\x1bOQ");
                return;
            }
            if i.key_pressed(Key::F3) {
                self.send_to_pty(b"\x1bOR");
                return;
            }
            if i.key_pressed(Key::F4) {
                self.send_to_pty(b"\x1bOS");
                return;
            }
            if i.key_pressed(Key::F5) {
                self.send_to_pty(b"\x1b[15~");
                return;
            }
            if i.key_pressed(Key::F6) {
                self.send_to_pty(b"\x1b[17~");
                return;
            }
            if i.key_pressed(Key::F7) {
                self.send_to_pty(b"\x1b[18~");
                return;
            }
            if i.key_pressed(Key::F8) {
                self.send_to_pty(b"\x1b[19~");
                return;
            }
            if i.key_pressed(Key::F9) {
                self.send_to_pty(b"\x1b[20~");
                return;
            }
            if i.key_pressed(Key::F10) {
                self.send_to_pty(b"\x1b[21~");
                return;
            }
            if i.key_pressed(Key::F11) {
                self.send_to_pty(b"\x1b[23~");
                return;
            }
            if i.key_pressed(Key::F12) {
                self.send_to_pty(b"\x1b[24~");
                return;
            }

            // Space
            if i.key_pressed(Key::Space) {
                self.send_to_pty(b" ");
                return;
            }
        });

        // Handle text input events for printable characters
        // This is more reliable than mapping individual keys
        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Text(text) = event {
                    // Send each character to PTY
                    for c in text.chars() {
                        if c.is_ascii() {
                            self.send_to_pty(&[c as u8]);
                        } else {
                            // Send UTF-8 bytes for non-ASCII
                            self.send_to_pty(c.to_string().as_bytes());
                        }
                    }
                }
            }
        });
    }

    /// Convert terminal color to egui color.
    fn color_to_egui(&self, color: Color) -> Color32 {
        match color {
            Color::Named(named) => named.to_egui(),
            Color::Indexed(_) => color.to_egui(true),
            Color::Rgb(r, g, b) => Color32::from_rgb(r, g, b),
        }
    }

    /// Render terminal grid using custom paint.
    fn render_terminal(&mut self, ui: &mut egui::Ui) {
        let available = ui.available_size();
        let grid = self.terminal_handler.grid();
        let (rows, cols) = grid.size();
        let cells = grid.cells();
        let (cursor_row, cursor_col) = grid.cursor_position();
        let cursor_visible = grid.cursor_visible();

        // Calculate character dimensions
        let font_id = FontId::new(14.0, FontFamily::Monospace);

        // Allocate space for painting
        let size = Vec2::new(available.x, available.y);
        let (response, painter) = ui.allocate_painter(size, Sense::click());
        let rect = response.rect;

        // Fill background
        painter.rect_filled(rect, 0.0, self.theme.background);

        // Render each cell
        for (row_idx, row) in cells.iter().enumerate() {
            if row_idx >= rows as usize {
                break;
            }

            for (col_idx, cell) in row.iter().enumerate() {
                if col_idx >= cols as usize {
                    break;
                }

                let x = rect.left() + col_idx as f32 * self.char_width;
                let y = rect.top() + row_idx as f32 * self.char_height;

                // Cell bounds
                let cell_rect = Rect::from_min_size(
                    Pos2::new(x, y),
                    Vec2::new(self.char_width, self.char_height),
                );

                // Get colors (handle reverse attribute)
                let (fg, bg) = if cell.attrs.reverse {
                    (
                        self.color_to_egui(cell.bg),
                        self.color_to_egui(cell.fg),
                    )
                } else {
                    (
                        self.color_to_egui(cell.fg),
                        self.color_to_egui(cell.bg),
                    )
                };

                // Draw background if not default
                if bg != self.theme.background {
                    painter.rect_filled(cell_rect, 0.0, bg);
                }

                // Draw character if not space
                if cell.ch != ' ' && !cell.attrs.hidden {
                    let mut text_color = fg;

                    // Apply dim attribute
                    if cell.attrs.dim {
                        text_color = Color32::from_rgba_unmultiplied(
                            text_color.r(),
                            text_color.g(),
                            text_color.b(),
                            128,
                        );
                    }

                    // Create text galley
                    let text = cell.ch.to_string();
                    let galley = painter.layout_no_wrap(text, font_id.clone(), text_color);

                    // Center text in cell
                    let text_pos = Pos2::new(
                        x + (self.char_width - galley.size().x) / 2.0,
                        y + (self.char_height - galley.size().y) / 2.0,
                    );

                    painter.galley(text_pos, galley, text_color);

                    // Draw underline
                    if cell.attrs.underline {
                        let y_line = y + self.char_height - 2.0;
                        painter.line_segment(
                            [Pos2::new(x, y_line), Pos2::new(x + self.char_width, y_line)],
                            Stroke::new(1.0, text_color),
                        );
                    }

                    // Draw strikethrough
                    if cell.attrs.strikethrough {
                        let y_line = y + self.char_height / 2.0;
                        painter.line_segment(
                            [Pos2::new(x, y_line), Pos2::new(x + self.char_width, y_line)],
                            Stroke::new(1.0, text_color),
                        );
                    }
                }
            }
        }

        // Draw cursor (only after shell is initialized, with blink)
        if cursor_visible && self.shell_initialized && self.cursor_blink_visible {
            let cursor_x = rect.left() + cursor_col as f32 * self.char_width;
            let cursor_y = rect.top() + cursor_row as f32 * self.char_height;

            // Thin vertical bar cursor (like Linux terminal)
            let bar_rect = Rect::from_min_size(
                Pos2::new(cursor_x, cursor_y),
                Vec2::new(2.0, self.char_height),
            );
            painter.rect_filled(bar_rect, 0.0, self.theme.cursor);
        }
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

        // Update cursor blink (530ms interval like typical terminals)
        if self.last_cursor_blink.elapsed() > Duration::from_millis(530) {
            self.cursor_blink_visible = !self.cursor_blink_visible;
            self.last_cursor_blink = Instant::now();
        }

        // Check for SIGINT (Ctrl+C) from system signal handler
        // This works even when egui doesn't receive the key event
        if crate::SIGINT_RECEIVED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            log::info!("System SIGINT received, sending to process group");
            self.send_sigint();
        }

        // Handle keyboard FIRST - ensures Ctrl+C works even during heavy output
        self.handle_keyboard(ctx);

        // Poll PTY output and feed to VTE parser (limited per frame)
        self.poll_pty_output();

        // Initialize shell with custom prompt after startup delay
        self.initialize_shell();

        // Poll LLM response if waiting
        if self.mode == AppMode::WaitingLLM {
            self.poll_llm_response();
        }

        // Render UI
        egui::CentralPanel::default()
            .frame(egui::Frame::none().fill(self.theme.background))
            .show(ctx, |ui| {
                // Calculate terminal size based on available space
                let available = ui.available_size();
                let cols = ((available.x / self.char_width) as u16).max(20);
                let rows = ((available.y / self.char_height) as u16).max(5);
                self.resize_pty(cols, rows);

                self.render_terminal(ui);
            });

        // Request continuous repaint for smooth updates
        ctx.request_repaint_after(Duration::from_millis(16));
    }
}
