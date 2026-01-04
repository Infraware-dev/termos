//! Main application struct implementing eframe::App with full terminal emulation.

use crate::config::{pty as pty_config, rendering, size, timing};
use crate::input::{KeyboardAction, KeyboardHandler, TextSelection};
use crate::llm::{HttpLLMClient, LLMQueryResult, LLMClientTrait};
use crate::orchestrators::NaturalLanguageOrchestrator;
use crate::pty::{PtyManager, PtyReader, PtyWrite, PtyWriter};
use crate::state::AppMode;
use crate::terminal::{Color, TerminalHandler};
use crate::ui::scrollbar::{Scrollbar, ScrollAction};
use crate::ui::{
    render_backgrounds, render_cursor, render_decorations, render_scrollbar, render_text_runs,
    Theme,
};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Vec2, ViewportCommand};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::Mutex as TokioMutex;

/// Events coming from background tasks (LLM, etc.)
#[derive(Debug)]
pub enum AppBackgroundEvent {
    /// LLM produced a chunk of output or completed
    LlmResult(LLMQueryResult),
    /// An error occurred during LLM query
    LlmError(String),
}

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

    /// Buffer for collecting user input during HITL interactions (AwaitingApproval/Answer)
    current_input_buffer: String,

    /// Buffer for tracking the current command line (for '?' prefix detection)
    current_command_buffer: String,

    // === LLM & Orchestration ===
    /// Orchestrator for natural language queries
    orchestrator: Arc<NaturalLanguageOrchestrator>,
    /// Channel for background events (sender)
    bg_event_tx: mpsc::Sender<AppBackgroundEvent>,
    /// Channel for background events (receiver)
    bg_event_rx: mpsc::Receiver<AppBackgroundEvent>,

    // === PERFORMANCE: Reusable render buffers (avoid per-frame allocations) ===
    /// Background rectangles buffer (reused each frame with .clear())
    render_bg_rects: Vec<(f32, f32, egui::Color32)>,
    /// Text runs buffer (reused each frame with .clear())
    render_text_runs: Vec<(f32, String, egui::Color32)>,
    /// Decorations buffer (reused each frame with .clear())
    render_decorations: Vec<(f32, bool, bool, egui::Color32)>,

    // === Text Selection ===
    /// Current text selection (None if no selection active)
    selection: Option<TextSelection>,

    // === Clipboard (arboard for direct OS access) ===
    /// Clipboard instance for immediate copy operations (bypasses egui's delayed sync)
    clipboard: Option<arboard::Clipboard>,

    /// Scrollbar logic and state
    scrollbar: Scrollbar,
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

        // Initialize clipboard (arboard) once - keeping it alive avoids macOS issues
        let clipboard = arboard::Clipboard::new()
            .map_err(|e| log::error!("Failed to init clipboard: {}", e))
            .ok();

        // Initialize background event channel
        let (bg_event_tx, bg_event_rx) = mpsc::channel();

        // Initialize LLM Orchestrator with fallback logic
        let backend_url = std::env::var("INFRAWARE_BACKEND_URL")
            .unwrap_or_else(|_| "http://localhost:8000".to_string());
        let api_key = std::env::var("BACKEND_API_KEY").unwrap_or_default();
        
        let llm_client: Arc<dyn LLMClientTrait> = if api_key.is_empty() {
            log::warn!("No BACKEND_API_KEY found, using Mock LLM Client");
            Arc::new(crate::llm::MockLLMClient::new())
        } else {
            log::info!("Using HTTP LLM Client at {}", backend_url);
            Arc::new(HttpLLMClient::new(backend_url, api_key))
        };

        let orchestrator = Arc::new(NaturalLanguageOrchestrator::new(llm_client));

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
            current_input_buffer: String::new(),
            current_command_buffer: String::new(),
            // LLM & Orchestration
            orchestrator,
            bg_event_tx,
            bg_event_rx,
            // Pre-allocate render buffers (reused each frame to avoid allocations)
            render_bg_rects: Vec::with_capacity(32),
            render_text_runs: Vec::with_capacity(32),
            render_decorations: Vec::with_capacity(8),
            // Text selection (initialized on first drag)
            selection: None,
            // Clipboard (kept alive for immediate OS access)
            clipboard,
            // Scrollbar logic
            scrollbar: Scrollbar::new(),
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
        // Also inject command_not_found hooks to trigger LLM on error
        let init_commands = if std::env::var("SHELL").unwrap_or_default().contains("zsh") {
            "export PROMPT='%F{green}|~| %n@%m:%~%# %f'\n\
             command_not_found_handler() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
             clear\n"
        } else {
            "export PS1=$'\\[\\e[32m\\]|~| \\u@\\h:\\w\\$ \\[\\e[0m\\]'\n\
             command_not_found_handle() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
             clear\n"
        };

        self.send_to_pty(init_commands.as_bytes());
        
        // Print welcome message with LLM status
        if std::env::var("BACKEND_API_KEY").is_ok() {
            let msg = "\r\n\x1b[1;32mInfraware Terminal Ready (Connected to LLM Backend)\x1b[0m\r\n";
            self.vte_parser.advance(&mut self.terminal_handler, msg.as_bytes());
        } else {
            let msg = "\r\n\x1b[1;33mInfraware Terminal Ready (Using Mock LLM - Set BACKEND_API_KEY to connect)\x1b[0m\r\n";
            self.vte_parser.advance(&mut self.terminal_handler, msg.as_bytes());
        }
        
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

    /// Poll background events (LLM results, etc.)
    fn poll_background_events(&mut self) {
        while let Ok(event) = self.bg_event_rx.try_recv() {
            match event {
                AppBackgroundEvent::LlmResult(result) => {
                    match result {
                        LLMQueryResult::Complete(text) => {
                            log::info!("LLM query complete");
                            let lines = self.orchestrator.render_response(&text);
                            for line in lines {
                                // Use VTE parser to handle ANSI formatting from renderer
                                self.vte_parser.advance(&mut self.terminal_handler, line.as_bytes());
                                self.vte_parser.advance(&mut self.terminal_handler, b"\r\n");
                            }
                            self.mode = AppMode::Normal;
                        }
                        LLMQueryResult::CommandApproval { command, message } => {
                            log::info!("LLM requested command approval: {}", command);
                            self.mode = AppMode::AwaitingApproval { command, message };
                        }
                        LLMQueryResult::Question { question, options } => {
                            log::info!("LLM asked a question: {}", question);
                            self.mode = AppMode::AwaitingAnswer { question, options };
                        }
                    }
                }
                AppBackgroundEvent::LlmError(err) => {
                    log::error!("LLM query error: {}", err);
                    let error_msg = format!("\x1b[31mError: {}\x1b[0m\r\n", err);
                    self.vte_parser.advance(&mut self.terminal_handler, error_msg.as_bytes());
                    self.mode = AppMode::Normal;
                }
            }
        }
    }

    /// Start an LLM query in a background task
    fn start_llm_query(&mut self, query: String) {
        log::info!("Starting LLM query: {}", query);
        self.mode = AppMode::WaitingLLM;
        
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();
        
        self.runtime.spawn(async move {
            // Using a new cancellation token for each query
            let cancel_token = tokio_util::sync::CancellationToken::new();
            
            match orchestrator.query(&query, cancel_token).await {
                Ok(result) => {
                    let _ = tx.send(AppBackgroundEvent::LlmResult(result));
                }
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string()));
                }
            }
        });
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
        // Intercept input if in HITL mode
        if !matches!(self.mode, AppMode::Normal | AppMode::WaitingLLM) {
            self.handle_hitl_keyboard(ctx);
            return;
        }

        // Process keyboard input and get actions (returns owned Vec to avoid borrow issues)
        let actions = self.keyboard_handler.process(ctx);

        // Execute each action
        for action in actions {
            match action {
                KeyboardAction::SendBytes(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut handled = false;
                    for c in text.chars() {
                        if c == '\r' || c == '\n' {
                            // Check for '?' prefix
                            if self.current_command_buffer.starts_with('?') {
                                let query = self.current_command_buffer[1..].trim().to_string();
                                if !query.is_empty() {
                                    self.current_command_buffer.clear();
                                    // Visual feedback: clear the line and move to next
                                    self.vte_parser.advance(&mut self.terminal_handler, b"\r\n");
                                    self.start_llm_query(query);
                                    handled = true;
                                    break;
                                }
                            }
                            self.current_command_buffer.clear();
                        } else if c == '\x7f' || c == '\x08' {
                            self.current_command_buffer.pop();
                        } else if !c.is_control() {
                            self.current_command_buffer.push(c);
                        }
                    }
                    
                    if !handled {
                        self.send_to_pty(&bytes);
                    }
                }
                KeyboardAction::SendSigInt => {
                    log::info!("Ctrl+C detected, sending ETX (0x03) to PTY");
                    self.send_to_pty(&[0x03]);
                    // Pause output reading briefly to let the kernel/shell process the signal
                    // and to give immediate visual feedback (stop scrolling)
                    self.output_pause_until =
                        Some(Instant::now() + std::time::Duration::from_millis(200));
                }
                KeyboardAction::Copy => {
                    self.copy_selection_to_clipboard(ctx);
                }
                KeyboardAction::Paste => {
                    self.perform_paste();
                }
            }
        }
    }

    /// Handle keyboard input specifically for Human-in-the-Loop interactions.
    fn handle_hitl_keyboard(&mut self, ctx: &egui::Context) {
        let actions = self.keyboard_handler.process(ctx);

        for action in actions {
            match action {
                KeyboardAction::SendBytes(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for c in text.chars() {
                        if c == '\r' || c == '\n' {
                            self.submit_hitl_input();
                        } else if c == '\x7f' || c == '\x08' {
                            self.current_input_buffer.pop();
                        } else if !c.is_control() {
                            self.current_input_buffer.push(c);
                        }
                    }
                }
                KeyboardAction::SendSigInt => {
                    log::info!("Cancelling HITL interaction");
                    self.current_input_buffer.clear();
                    self.mode = AppMode::Normal;
                }
                _ => {}
            }
        }
    }

    /// Submit current input buffer for the active HITL interaction.
    fn submit_hitl_input(&mut self) {
        let input = std::mem::take(&mut self.current_input_buffer);
        let mode = self.mode.clone();
        
        match mode {
            AppMode::AwaitingApproval { command, .. } => {
                let approved = crate::orchestrators::HitlOrchestrator::parse_approval(&input);
                if approved {
                    log::info!("User approved command: {}", command);
                    // Echo the command to the terminal
                    let echo = format!("Executing: {}\r\n", command);
                    self.vte_parser.advance(&mut self.terminal_handler, echo.as_bytes());
                    
                    // Option 1: Execute directly in PTY
                    let cmd_bytes = format!("{}\n", command);
                    self.send_to_pty(cmd_bytes.as_bytes());
                    
                    // Option 2: Resume LLM if it expects feedback (using main branch logic)
                    self.resume_llm_run();
                } else {
                    log::info!("User rejected command: {}", command);
                    self.mode = AppMode::Normal;
                }
            }
            AppMode::AwaitingAnswer { .. } => {
                log::info!("User answered question: {}", input);
                self.resume_llm_with_answer(input);
            }
            _ => {
                self.mode = AppMode::Normal;
            }
        }
    }

    /// Resume LLM run after approval.
    fn resume_llm_run(&mut self) {
        self.mode = AppMode::WaitingLLM;
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();
        
        self.runtime.spawn(async move {
            match orchestrator.resume_run().await {
                Ok(result) => {
                    let _ = tx.send(AppBackgroundEvent::LlmResult(result));
                }
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string()));
                }
            }
        });
    }

    /// Resume LLM run with a text answer.
    fn resume_llm_with_answer(&mut self, answer: String) {
        self.mode = AppMode::WaitingLLM;
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();
        
        self.runtime.spawn(async move {
            match orchestrator.resume_with_answer(&answer).await {
                Ok(result) => {
                    let _ = tx.send(AppBackgroundEvent::LlmResult(result));
                }
                Err(e) => {
                    let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string()));
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

    /// Convert screen coordinates to grid (row, col).
    /// Returns visible row index (0-based from top of visible area).
    fn screen_to_grid(&self, pos: Pos2, rect: Rect) -> (usize, usize) {
        let col = ((pos.x - rect.left()) / self.char_width).max(0.0) as usize;
        let row = ((pos.y - rect.top()) / self.char_height).max(0.0) as usize;

        // Clamp to valid range
        let max_col = self.terminal_size.0.saturating_sub(1) as usize;
        let max_row = self.terminal_size.1.saturating_sub(1) as usize;

        (row.min(max_row), col.min(max_col))
    }

    /// Check if a cell is within the current selection.
    fn is_cell_selected(&self, row: usize, col: usize) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|sel| sel.contains(row, col))
    }

    /// Copy selected text to clipboard using arboard for immediate OS access.
    ///
    /// Uses arboard directly instead of egui's `ctx.copy_text()` to avoid
    /// the delayed clipboard sync that causes "stale data" issues on macOS.
    fn copy_selection_to_clipboard(&mut self, ctx: &egui::Context) {
        log::info!(
            "copy_selection_to_clipboard called, selection: {:?}",
            self.selection
        );

        if let Some(ref sel) = self.selection {
            if sel.is_empty() {
                log::info!("Selection is empty, nothing to copy");
                return;
            }

            let (start, end) = sel.normalized();
            log::info!(
                "Extracting text from ({},{}) to ({},{})",
                start.row,
                start.col,
                end.row,
                end.col
            );

            let text = self
                .terminal_handler
                .grid()
                .extract_selection_text(start.row, start.col, end.row, end.col);

            log::info!("Extracted text: '{}' ({} chars)", text, text.len());

            if !text.is_empty() {
                // Use arboard for immediate OS clipboard write (bypasses egui's delayed sync)
                if let Some(ref mut cb) = self.clipboard {
                    match cb.set_text(&text) {
                        Ok(()) => {
                            log::info!(
                                "Text copied to OS clipboard via arboard ({} chars)",
                                text.len()
                            );
                        }
                        Err(e) => {
                            log::error!("Arboard copy error: {}", e);
                            // Fallback to egui if arboard fails
                            ctx.copy_text(text);
                        }
                    }
                } else {
                    // Fallback to egui if arboard init failed
                    log::warn!("Arboard not available, using egui fallback");
                    ctx.copy_text(text);
                }
            }
        } else {
            log::info!("No selection exists");
        }
    }

    /// Perform paste operation using arboard for direct OS clipboard access.
    ///
    /// Supports Bracketed Paste Mode: when enabled by the terminal application
    /// (via ESC[?2004h), wraps pasted content in escape sequences to prevent
    /// auto-execution and the "staircase effect" in editors like vim.
    fn perform_paste(&mut self) {
        log::info!("perform_paste() called");

        if self.clipboard.is_none() {
            log::error!("Clipboard (arboard) not initialized!");
            return;
        }

        let cb = self.clipboard.as_mut().expect("checked above");
        match cb.get_text() {
            Ok(text) if !text.is_empty() => {
                let use_bracketed = self.terminal_handler.bracketed_paste_enabled();

                let mut payload =
                    Vec::with_capacity(text.len() + if use_bracketed { 12 } else { 0 });

                if use_bracketed {
                    // Start bracketed paste sequence
                    payload.extend_from_slice(b"\x1b[200~");
                }

                payload.extend_from_slice(text.as_bytes());

                if use_bracketed {
                    // End bracketed paste sequence
                    payload.extend_from_slice(b"\x1b[201~");
                }

                log::info!(
                    "Pasting {} bytes to PTY (bracketed: {}, text: '{}')",
                    payload.len(),
                    use_bracketed,
                    if text.len() > 50 {
                        format!("{}...", &text[..50])
                    } else {
                        text.clone()
                    }
                );
                self.send_to_pty(&payload);
            }
            Ok(_) => {
                log::warn!("Clipboard is empty, nothing to paste");
            }
            Err(e) => {
                log::error!("Failed to read clipboard via arboard: {}", e);
            }
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

        // Calculate scrollbar area for exclusion (using Scrollbar helper)
        let scrollbar_area = self.scrollbar.area(rect);

        // Request keyboard focus when terminal is clicked (without drag)
        if response.clicked() {
            if let Some(pos) = response.interact_pointer_pos() {
                if !scrollbar_area.contains(pos) {
                    ui.memory_mut(|mem| mem.request_focus(terminal_id));
                    // Clear selection on simple click
                    self.selection = None;
                }
            }
        }

        // Handle mouse drag for text selection
        if response.drag_started() {
            if let Some(pos) = response.interact_pointer_pos() {
                if !scrollbar_area.contains(pos) {
                    let (row, col) = self.screen_to_grid(pos, rect);
                    self.selection = Some(TextSelection::new(row, col));
                    log::debug!("Selection started at ({}, {})", row, col);
                }
            }
        }

        if response.dragged() {
            if let Some(pos) = response.interact_pointer_pos() {
                // If we are dragging the scrollbar, don't update text selection
                if !self.scrollbar.is_dragging() {
                    let (row, col) = self.screen_to_grid(pos, rect);
                    if let Some(ref mut sel) = self.selection {
                        sel.update_end(row, col);
                    }
                }
            }
        }

        if response.drag_stopped() {
            if let Some(ref mut sel) = self.selection {
                sel.active = false;
                log::debug!("Selection ended");
            }
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

                // --- SCROLLBAR HANDLING ---
                // Get scroll state (copy values to avoid borrow issues)
                let (scroll_offset, max_scroll, visible_lines_count, cols) = {
                    let grid = self.terminal_handler.grid();
                    (grid.scroll_offset(), grid.max_scroll_offset(), grid.visible_rows().len(), grid.size().1)
                };
        
                // Render and handle scrollbar interaction
                if let Some(action) = self.scrollbar.show(
                    ui,
                    &painter,
                    rect,
                    scroll_offset,
                    max_scroll,
                    visible_lines_count,
                ) {
                    match action {
                        ScrollAction::ScrollUp(n) => self.terminal_handler.grid_mut().scroll_view_up(n),
                        ScrollAction::ScrollDown(n) => self.terminal_handler.grid_mut().scroll_view_down(n),
                        ScrollAction::ScrollTo(offset) => self.terminal_handler.grid_mut().scroll_to_offset(offset),
                    }
                }
        
                // --- RENDERING PHASE ---
                // Now re-acquire grid state for rendering (might have changed due to input)
                let grid = self.terminal_handler.grid();
                // Update these values for rendering
                let scroll_offset = grid.scroll_offset();
                let max_scroll = grid.max_scroll_offset();
                let visible_rows = grid.visible_rows();
                let (cursor_row, cursor_col) = grid.cursor_position();
                let cursor_visible = grid.cursor_visible();
        
                // Fill background
                painter.rect_filled(rect, 0.0, self.theme.background);
        // ... (rendering rows remains same) ...
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

                // --- Background batching (with selection support) ---
                let is_selected = self.is_cell_selected(row_idx, col_idx);
                let bg = if is_selected {
                    // Selection takes priority over other background colors
                    self.theme.selection
                } else if cell.attrs.reverse {
                    self.color_to_egui(cell.fg)
                } else {
                    self.color_to_egui(cell.bg)
                };

                // For selection, always draw background (even if it matches theme background)
                if is_selected || bg != self.theme.background {
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

        // Draw Braille Throbber when waiting for LLM
        if matches!(self.mode, AppMode::WaitingLLM) && scroll_offset == 0 {
            let braille_frames = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
            let frame_idx = (self.startup_time.elapsed().as_millis() / 100) as usize % braille_frames.len();
            let frame = braille_frames[frame_idx];
            
            // Draw at current cursor position
            let tx = rect.left() + cursor_col as f32 * self.char_width;
            let ty = rect.top() + cursor_row as f32 * self.char_height;
            
            painter.text(
                Pos2::new(tx, ty),
                egui::Align2::LEFT_TOP,
                frame,
                font_id.clone(),
                Color32::from_rgb(0, 255, 0), // Infraware Green
            );
        } else if cursor_visible
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
            // Pause output reading briefly to let the kernel/shell process the signal
            // and to give immediate visual feedback (stop scrolling)
            self.output_pause_until = Some(Instant::now() + std::time::Duration::from_millis(200));
        }

        // Poll background events (LLM results, etc.)
        self.poll_background_events();

        // Check for pending LLM query from shell hook (Command Not Found)
        if let Some(failed_cmd) = self.terminal_handler.take_pending_llm_query() {
            log::info!("Triggering LLM for failed command: {}", failed_cmd);
            let query = format!("I tried to run '{}' but got 'command not found'. What should I do?", failed_cmd);
            self.start_llm_query(query);
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
            
            // Check if we need to animate the throbber (10 FPS / 100ms)
            let is_waiting_llm = matches!(self.mode, AppMode::WaitingLLM);

            if pty_had_data || cursor_needs_blink || had_user_input {
                // Something changed - repaint immediately
                ctx.request_repaint();
            } else if is_waiting_llm {
                // Animate throbber at 10 FPS
                ctx.request_repaint_after(std::time::Duration::from_millis(100));
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
