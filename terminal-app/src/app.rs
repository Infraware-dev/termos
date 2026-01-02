//! Main application struct implementing eframe::App with full terminal emulation.

use crate::config::{pty as pty_config, rendering, size, timing};
use crate::input::{KeyboardAction, KeyboardHandler};
use crate::pty::{PtyManager, PtyReader, PtyWrite, PtyWriter};
use crate::state::AppMode;
use crate::terminal::{Color, TerminalHandler};
use crate::ui::{
    render_backgrounds, render_cursor, render_decorations, render_scrollbar, render_text_runs,
    Theme,
};
use egui::{Color32, FontFamily, FontId, Sense, Vec2, ViewportCommand};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;

/// Main terminal application with VTE-based terminal emulation.
pub struct InfrawareApp {
    /// Current application mode
    mode: AppMode,

    /// Theme configuration
    theme: Theme,

    /// VTE parser for escape sequences
    vte_parser: vte::Parser,

    /// Terminal handler with grid state
    terminal_handler: TerminalHandler,

    /// PTY writer for sending input
    pty_writer: Option<Arc<PtyWriter>>,

    /// PTY output receiver channel
    pty_output_rx: Option<mpsc::Receiver<Vec<u8>>>,

    /// PTY reader (must be kept alive to keep reader thread running)
    #[expect(dead_code, reason = "Held to keep reader thread alive via Drop")]
    pty_reader: Option<PtyReader>,

    /// PTY manager for resize
    pty_manager: Option<Arc<TokioMutex<PtyManager>>>,

    /// Current terminal size (cols, rows)
    terminal_size: (u16, u16),

    /// Flag to quit application
    should_quit: bool,

    /// Tokio runtime
    runtime: Runtime,

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

    /// When set, pause output reading to let kernel process Ctrl+C
    output_pause_until: Option<Instant>,

    /// Track window focus state to detect focus gain
    had_window_focus: bool,

    /// Cached font for rendering (avoids per-frame allocation)
    font_id: FontId,

    /// Pre-calculated X coordinates for each column (avoids per-cell multiplication)
    column_x_coords: Vec<f32>,

    /// Keyboard input handler (extracted for testability)
    keyboard_handler: KeyboardHandler,

    // === PERFORMANCE: Reusable render buffers (avoid per-frame allocations) ===
    /// Background rectangles buffer (reused each frame with .clear())
    render_bg_rects: Vec<(f32, f32, egui::Color32)>,
    /// Text runs buffer (reused each frame with .clear())
    render_text_runs: Vec<(f32, String, egui::Color32)>,
    /// Decorations buffer (reused each frame with .clear())
    render_decorations: Vec<(f32, bool, bool, egui::Color32)>,
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

        // Default terminal size from config
        let (rows, cols) = (size::DEFAULT_ROWS, size::DEFAULT_COLS);

        // Initialize PTY with consolidated channel (FASE 2 optimization)
        // Single sync_channel instead of double channel eliminates async overhead
        let (pty_writer, pty_output_rx, pty_reader, pty_manager) = runtime.block_on(async {
            match PtyManager::new().await {
                Ok(mut manager) => {
                    log::info!("PTY initialized with shell: {}", manager.shell());

                    // Create sync_channel with limited capacity for BACKPRESSURE
                    // When channel is full, reader thread blocks -> kernel buffer fills ->
                    // cat blocks on write() -> kernel can process Ctrl+C
                    let (tx, rx) = mpsc::sync_channel(pty_config::CHANNEL_CAPACITY);

                    let writer = manager.take_writer().await.ok();
                    // PERFORMANCE: Reader sends directly to sync_channel (no bridge thread)
                    // IMPORTANT: Reader must be kept alive to keep reader thread running
                    let reader = manager.take_reader(tx).await.ok();

                    let manager = Arc::new(TokioMutex::new(manager));
                    (writer, Some(rx), reader, Some(manager))
                }
                Err(e) => {
                    log::error!("Failed to initialize PTY: {}", e);
                    (None, None, None, None)
                }
            }
        });

        Self {
            mode: AppMode::Normal,
            theme,
            vte_parser: vte::Parser::new(),
            terminal_handler: TerminalHandler::new(rows, cols),
            pty_writer,
            pty_output_rx,
            pty_reader,
            pty_manager,
            terminal_size: (cols, rows),
            should_quit: false,
            runtime,
            theme_applied: false,
            char_width: rendering::CHAR_WIDTH,
            char_height: rendering::CHAR_HEIGHT,
            last_resize: Instant::now(),
            shell_initialized: false,
            startup_time: Instant::now(),
            cursor_blink_visible: true,
            last_cursor_blink: Instant::now(),
            output_pause_until: None,
            had_window_focus: false,
            font_id: FontId::new(rendering::FONT_SIZE, FontFamily::Monospace),
            column_x_coords: (0..cols)
                .map(|c| c as f32 * rendering::CHAR_WIDTH)
                .collect(),
            keyboard_handler: KeyboardHandler::new(),
            // Pre-allocate render buffers (reused each frame to avoid allocations)
            render_bg_rects: Vec::with_capacity(32),
            render_text_runs: Vec::with_capacity(32),
            render_decorations: Vec::with_capacity(8),
        }
    }

    /// Initialize shell with custom prompt after startup.
    fn initialize_shell(&mut self) {
        if self.shell_initialized {
            return;
        }

        // Wait for shell to fully initialize
        if self.startup_time.elapsed() < timing::SHELL_INIT_DELAY {
            return;
        }

        self.shell_initialized = true;

        // Set custom prompt with |~| prefix (green)
        // Use $'...' for bash escape interpretation and %{...%} for zsh
        // The shell-specific syntax is handled by detecting which shell is running
        let init_commands = if std::env::var("SHELL").unwrap_or_default().contains("zsh") {
            // zsh: use %{ %} for non-printing chars and %F/%f for colors
            "export PROMPT='%F{green}|~| %n@%m:%~%# %f'\nclear\n"
        } else {
            // bash: use $'...' for escape interpretation, \[...\] for non-printing
            "export PS1=$'\\[\\e[32m\\]|~| \\u@\\h:\\w\\$ \\[\\e[0m\\]'\nclear\n"
        };

        self.send_to_pty(init_commands.as_bytes());
        log::info!("Shell initialized with custom prompt");
    }

    /// Poll PTY output and feed to VTE parser.
    /// Rate-limited to allow Ctrl+C to work even with heavy output (cat /dev/zero).
    /// Returns true if any output was processed (for smart repaint).
    fn poll_pty_output(&mut self) -> bool {
        // If paused (after Ctrl+C), skip reading to let kernel process input
        if let Some(until) = self.output_pause_until {
            if Instant::now() < until {
                return false;
            }
            self.output_pause_until = None;
        }

        let mut bytes_processed = 0;

        if let Some(ref rx) = self.pty_output_rx {
            loop {
                if bytes_processed >= rendering::MAX_BYTES_PER_FRAME {
                    break;
                }

                match rx.try_recv() {
                    Ok(bytes) => {
                        bytes_processed += bytes.len();
                        // VTE 0.15+ takes &[u8] slice instead of single byte
                        self.vte_parser.advance(&mut self.terminal_handler, &bytes);
                        // Auto-scroll to bottom when new output arrives
                        self.terminal_handler.grid_mut().scroll_to_bottom();
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        log::info!("Shell exited, quitting application");
                        self.should_quit = true;
                        break;
                    }
                }
            }
        }

        bytes_processed > 0
    }

    /// Send data to PTY synchronously (ensures immediate delivery).
    ///
    /// Uses the `PtyWrite` trait for dependency injection support.
    fn send_to_pty(&self, data: &[u8]) {
        if let Some(ref writer) = self.pty_writer {
            log::debug!("Writing {} bytes to PTY: {:?}", data.len(), data);
            // Use trait method for DI compatibility
            match writer.write_bytes(data) {
                Ok(n) => log::debug!("Wrote {} bytes to PTY", n),
                Err(e) => log::error!("Failed to write to PTY: {}", e),
            }
        } else {
            log::warn!("No PTY writer available!");
        }
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
        if self.terminal_size != (cols, rows)
            && self.last_resize.elapsed() > timing::RESIZE_DEBOUNCE
        {
            self.terminal_size = (cols, rows);
            self.last_resize = Instant::now();

            // Pre-calculate column X coordinates (avoids per-cell multiplication in render loop)
            self.column_x_coords = (0..cols).map(|c| c as f32 * self.char_width).collect();

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

    /// Handle keyboard input using the extracted KeyboardHandler.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Process keyboard input and get actions (returns owned Vec to avoid borrow issues)
        let actions = self.keyboard_handler.process(ctx);

        // Execute each action
        for action in actions {
            match action {
                KeyboardAction::SendBytes(bytes) => {
                    self.send_to_pty(&bytes);
                }
                KeyboardAction::SendSigInt => {
                    log::info!("Ctrl+C detected, sending ETX (0x03) to PTY");
                    self.send_to_pty(&[0x03]);
                }
            }
        }
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
    /// `has_focus` is passed to avoid redundant ctx.input() calls.
    fn render_terminal(&mut self, ui: &mut egui::Ui, has_focus: bool) {
        let available = ui.available_size();

        // Use cached font (avoids per-frame allocation)
        let font_id = &self.font_id;

        // Use fixed ID for terminal to enable focus from update()
        let terminal_id = egui::Id::new("terminal_main_area");

        // Allocate space for painting with scroll support
        let size = Vec2::new(available.x, available.y);
        let (response, painter) = ui.allocate_painter(size, Sense::click().union(Sense::drag()));
        let rect = response.rect;

        // Request keyboard focus when terminal is clicked
        if response.clicked() {
            ui.memory_mut(|mem| mem.request_focus(terminal_id));
        }

        // Handle mouse wheel scrolling
        let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
        if scroll_delta != 0.0 {
            let lines = (scroll_delta / self.char_height).round() as i32;
            let grid = self.terminal_handler.grid_mut();
            if lines > 0 {
                grid.scroll_view_up(lines as usize);
            } else if lines < 0 {
                grid.scroll_view_down((-lines) as usize);
            }
        }

        // Get grid info for rendering
        let grid = self.terminal_handler.grid();
        let (_rows, cols) = grid.size();
        let visible_rows = grid.visible_rows();
        let (cursor_row, cursor_col) = grid.cursor_position();
        let cursor_visible = grid.cursor_visible();
        let scroll_offset = grid.scroll_offset();
        let max_scroll = grid.max_scroll_offset();

        // Fill background
        painter.rect_filled(rect, 0.0, self.theme.background);

        // SINGLE-PASS RENDERING: Iterate each row once, collecting all render data,
        // then draw in correct z-order (backgrounds → text → decorations).
        // This reduces from 3 iterations to 1 iteration per row.

        // PERFORMANCE: Use struct-level buffers to avoid per-frame allocations.
        // These are cleared per-row but memory is reused across frames.
        // (Vec::clear() is O(1) and doesn't deallocate)

        for (row_idx, row) in visible_rows.iter().enumerate() {
            let y = rect.top() + row_idx as f32 * self.char_height;

            // Clear buffers for this row (O(1), reuses allocations)
            self.render_bg_rects.clear();
            self.render_text_runs.clear();
            self.render_decorations.clear();

            // State for batching
            let mut bg_start: Option<(usize, Color32)> = None;
            let mut text_run = String::new();
            let mut run_start: Option<(usize, Color32)> = None;

            // SINGLE PASS: iterate cells once, collect all render data
            // PERFORMANCE: row_len is bounded by cols, and column_x_coords has cols entries
            let row_len = row.len().min(cols as usize);
            let cols_usize = cols as usize;

            // PERFORMANCE: Direct indexing helper - all indices in loop are < row_len <= cols
            let col_x = |idx: usize| -> f32 {
                if idx < cols_usize {
                    self.column_x_coords[idx]
                } else {
                    idx as f32 * self.char_width
                }
            };

            for (col_idx, cell) in row.iter().take(row_len).enumerate() {
                let x = col_x(col_idx);

                // --- Background batching ---
                let bg = if cell.attrs.reverse {
                    self.color_to_egui(cell.fg)
                } else {
                    self.color_to_egui(cell.bg)
                };

                if bg != self.theme.background {
                    match bg_start {
                        Some((_start, color)) if color == bg => {
                            // Continue current background run
                        }
                        Some((start, color)) => {
                            // Flush previous background run
                            let width = (col_idx - start) as f32 * self.char_width;
                            self.render_bg_rects.push((col_x(start), width, color));
                            bg_start = Some((col_idx, bg));
                        }
                        None => {
                            bg_start = Some((col_idx, bg));
                        }
                    }
                } else if let Some((start, color)) = bg_start.take() {
                    let width = (col_idx - start) as f32 * self.char_width;
                    self.render_bg_rects.push((col_x(start), width, color));
                }

                // --- Text batching ---
                if cell.ch == ' ' || cell.attrs.hidden {
                    if let Some((start, color)) = run_start.take() {
                        if !text_run.is_empty() {
                            self.render_text_runs.push((
                                col_x(start),
                                std::mem::take(&mut text_run),
                                color,
                            ));
                        }
                    }
                } else {
                    let mut fg = if cell.attrs.reverse {
                        self.color_to_egui(cell.bg)
                    } else {
                        self.color_to_egui(cell.fg)
                    };

                    if cell.attrs.dim {
                        fg = Color32::from_rgba_unmultiplied(fg.r(), fg.g(), fg.b(), 128);
                    }

                    match run_start {
                        Some((_start, color)) if color == fg => {
                            text_run.push(cell.ch);
                        }
                        Some((start, color)) => {
                            if !text_run.is_empty() {
                                self.render_text_runs.push((
                                    col_x(start),
                                    std::mem::take(&mut text_run),
                                    color,
                                ));
                            }
                            run_start = Some((col_idx, fg));
                            text_run.push(cell.ch);
                        }
                        None => {
                            run_start = Some((col_idx, fg));
                            text_run.push(cell.ch);
                        }
                    }
                }

                // --- Decorations (collect, don't batch - they're rare) ---
                if cell.attrs.underline || cell.attrs.strikethrough {
                    let fg = if cell.attrs.reverse {
                        self.color_to_egui(cell.bg)
                    } else {
                        self.color_to_egui(cell.fg)
                    };
                    self.render_decorations.push((
                        x,
                        cell.attrs.underline,
                        cell.attrs.strikethrough,
                        fg,
                    ));
                }
            }

            // Flush remaining background
            if let Some((start, color)) = bg_start {
                let width = (row_len - start) as f32 * self.char_width;
                self.render_bg_rects.push((col_x(start), width, color));
            }

            // Flush remaining text (use std::mem::take to avoid clone)
            if let Some((start, color)) = run_start {
                if !text_run.is_empty() {
                    self.render_text_runs.push((
                        col_x(start),
                        std::mem::take(&mut text_run),
                        color,
                    ));
                }
            }

            // --- DRAW PHASE: Render in correct z-order using helper functions ---
            render_backgrounds(&painter, rect, y, self.char_height, &self.render_bg_rects);
            render_text_runs(&painter, rect, y, font_id, &self.render_text_runs);
            render_decorations(
                &painter,
                rect,
                y,
                self.char_width,
                self.char_height,
                &self.render_decorations,
            );
        }

        // Draw cursor (only when at bottom/live view, after shell init, with blink, and focused)
        if cursor_visible
            && self.shell_initialized
            && self.cursor_blink_visible
            && scroll_offset == 0
            && has_focus
        {
            // Direct indexing with bounds check (cursor_col comes from grid, should be < cols)
            let cursor_col_idx = cursor_col as usize;
            let cursor_x = rect.left()
                + if cursor_col_idx < self.column_x_coords.len() {
                    self.column_x_coords[cursor_col_idx]
                } else {
                    cursor_col as f32 * self.char_width
                };
            let cursor_y = rect.top() + cursor_row as f32 * self.char_height;
            render_cursor(
                &painter,
                cursor_x,
                cursor_y,
                self.char_height,
                self.theme.cursor,
            );
        }

        // Draw scrollbar if there's scrollback content
        if max_scroll > 0 {
            render_scrollbar(
                &painter,
                rect,
                scroll_offset,
                max_scroll,
                visible_rows.len(),
            );
        }
    }
}

impl eframe::App for InfrawareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme once on startup
        if !self.theme_applied {
            self.theme.apply(ctx);
            self.theme_applied = true;
        }

        // Handle window focus - request terminal focus when window becomes active
        let has_focus = ctx.input(|i| i.focused);
        let terminal_id = egui::Id::new("terminal_main_area");

        if has_focus && !self.had_window_focus {
            // Window just gained focus - force egui to focus the terminal widget
            ctx.memory_mut(|mem| mem.request_focus(terminal_id));
        }

        // If window has focus but terminal doesn't, claim it
        // This ensures keyboard input always goes to the terminal
        if has_focus && !ctx.memory(|mem| mem.has_focus(terminal_id)) {
            ctx.memory_mut(|mem| mem.request_focus(terminal_id));
        }

        self.had_window_focus = has_focus;

        // Check for quit
        if self.should_quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        // PERFORMANCE: Check if window is minimized (skip heavy work)
        let is_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));

        // Update cursor blink only when window has focus AND not minimized (530ms interval)
        if has_focus && !is_minimized {
            if self.last_cursor_blink.elapsed() > timing::CURSOR_BLINK_INTERVAL {
                self.cursor_blink_visible = !self.cursor_blink_visible;
                self.last_cursor_blink = Instant::now();
            }
        } else {
            // When unfocused or minimized, keep cursor visible but static
            // Reset timer so blink starts fresh when focus returns
            self.cursor_blink_visible = true;
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
        let pty_had_data = self.poll_pty_output();

        // Initialize shell with custom prompt after startup delay
        self.initialize_shell();

        // Render UI (pass has_focus to avoid redundant ctx.input() calls)
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.background))
            .show(ctx, |ui| {
                // Calculate terminal size based on available space
                let available = ui.available_size();
                let cols = ((available.x / self.char_width) as u16).max(20);
                let rows = ((available.y / self.char_height) as u16).max(5);
                self.resize_pty(cols, rows);

                self.render_terminal(ui, has_focus);
            });

        // REACTIVE REPAINT: Only request repaint when something actually changed
        // This dramatically reduces CPU usage when idle (from ~50% to <5%)
        if is_minimized {
            // PERFORMANCE: Window minimized - very rare repaint (just check PTY)
            ctx.request_repaint_after(timing::BACKGROUND_REPAINT * 2);
        } else if has_focus {
            // Check if there was any user interaction (keyboard/mouse)
            let had_user_input = ctx.input(|i| {
                !i.events.is_empty() || i.pointer.any_down() || i.pointer.any_released()
            });

            // Calculate time until next cursor blink
            let blink_interval = timing::CURSOR_BLINK_INTERVAL;
            let time_since_blink = self.last_cursor_blink.elapsed();
            let cursor_needs_blink = time_since_blink >= blink_interval;

            if pty_had_data || cursor_needs_blink || had_user_input {
                // Something changed - repaint immediately
                ctx.request_repaint();
            } else {
                // Idle - schedule repaint only for next cursor blink
                let time_to_next_blink = blink_interval.saturating_sub(time_since_blink);
                ctx.request_repaint_after(time_to_next_blink);
            }
        } else {
            // Window in background: very low FPS to save CPU
            ctx.request_repaint_after(timing::BACKGROUND_REPAINT);
        }
    }
}
