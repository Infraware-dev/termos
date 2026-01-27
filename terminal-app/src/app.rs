//! Main application struct implementing eframe::App with full terminal emulation.
//!
//! Supports split view via egui_tiles for multiple terminal panes.

use crate::auth::{AuthConfig, Authenticator, HttpAuthenticator};
use crate::config::{rendering, timing};
use crate::input::{
    InputClassifier, InputType, KeyboardAction, KeyboardHandler, TextSelection, ValidationResult,
    validate_command,
};
use crate::llm::{HttpLLMClient, LLMClientTrait, LLMQueryResult};
use crate::orchestrators::NaturalLanguageOrchestrator;
use crate::session::{SessionId, TerminalSession};
use crate::state::AppMode;
use crate::terminal::Color;
use crate::ui::scrollbar::{ScrollAction, Scrollbar};
use crate::ui::{
    SPINNER_FRAMES, Theme, render_backgrounds, render_cursor, render_decorations, render_scrollbar,
    render_text_runs_buffered,
};
use egui::{Color32, FontFamily, FontId, Pos2, Rect, Sense, Vec2, ViewportCommand};
use egui_tiles::{Tiles, Tree, UiResponse};
use smallvec::SmallVec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;
use tokio::runtime::Runtime;

/// Safely truncate a UTF-8 string to at most `max_bytes` bytes,
/// ensuring the result ends at a valid char boundary.
#[allow(dead_code)] // Used for debugging output capture
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

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
    // === Sessions (Split View Support) ===
    /// All terminal sessions, keyed by session ID (which is also the pane ID in egui_tiles)
    sessions: HashMap<SessionId, TerminalSession>,

    /// Currently focused session ID
    active_session_id: SessionId,

    /// Next session ID to assign (monotonically increasing)
    next_session_id: SessionId,

    /// Theme configuration
    theme: Theme,

    /// Flag to quit application
    should_quit: bool,

    /// Tokio runtime
    runtime: Runtime,

    /// Theme applied flag
    theme_applied: bool,

    /// Font metrics
    char_width: f32,
    char_height: f32,

    /// Startup time for delayed init
    startup_time: Instant,

    /// Cursor blink state
    cursor_blink_visible: bool,

    /// Last cursor blink toggle time
    last_cursor_blink: Instant,

    /// Last keyboard input time (for adaptive PTY throttling)
    last_keyboard_time: Instant,

    /// Track window focus state to detect focus gain
    had_window_focus: bool,

    /// Cached font for rendering (avoids per-frame allocation)
    font_id: FontId,

    /// Keyboard input handler (extracted for testability)
    keyboard_handler: KeyboardHandler,

    /// Input classifier for command vs natural language detection
    input_classifier: InputClassifier,

    /// Buffer for collecting user input during HITL interactions (AwaitingApproval/Answer)
    current_input_buffer: String,

    /// Buffer for tracking the current command line (for classification)
    current_command_buffer: String,

    // === LLM & Orchestration ===
    /// Orchestrator for natural language queries
    orchestrator: Arc<NaturalLanguageOrchestrator>,
    /// Channel for background events (sender)
    bg_event_tx: mpsc::Sender<AppBackgroundEvent>,
    /// Channel for background events (receiver)
    bg_event_rx: mpsc::Receiver<AppBackgroundEvent>,
    /// Cancellation token for active LLM queries (allows Ctrl+C to cancel)
    llm_cancel_token: Option<tokio_util::sync::CancellationToken>,

    // === PERFORMANCE: Reusable render buffers (avoid per-frame allocations) ===
    /// Background rectangles buffer (reused each frame with .clear())
    render_bg_rects: Vec<(f32, f32, egui::Color32)>,
    /// Text runs buffer - stores (x_offset, end_index_in_text_buffer, color)
    /// The actual text is in render_text_buffer
    render_text_runs: Vec<(f32, usize, egui::Color32)>,
    /// Single text buffer for all runs in a row (avoids per-run String allocation)
    render_text_buffer: String,
    /// Decorations buffer (reused each frame with .clear())
    render_decorations: Vec<(f32, bool, bool, egui::Color32)>,

    // === Clipboard (arboard for direct OS access) ===
    /// Clipboard instance for immediate copy operations (bypasses egui's delayed sync)
    clipboard: Option<arboard::Clipboard>,

    /// Scrollbar logic and state (shared across sessions)
    scrollbar: Scrollbar,

    /// Authentication status message (shown at startup)
    auth_status_message: Option<String>,

    // === Split View (egui_tiles) ===
    /// Tile tree for split view layout (pane ID is session ID)
    /// Wrapped in Option to allow temporary removal for borrow-checker compatibility
    tiles: Option<Tree<SessionId>>,

    /// Mapping from SessionId to TileId for pane removal
    session_tile_ids: HashMap<SessionId, egui_tiles::TileId>,
}

impl std::fmt::Debug for InfrawareApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareApp")
            .field("sessions", &self.sessions.len())
            .field("active_session_id", &self.active_session_id)
            .finish()
    }
}

impl InfrawareApp {
    /// Create a new application instance.
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        let theme = Theme::dark();

        // Create initial session
        let initial_session_id: SessionId = 0;
        let initial_session = TerminalSession::new(initial_session_id, runtime.handle());

        let mut sessions = HashMap::new();
        sessions.insert(initial_session_id, initial_session);

        // Initialize clipboard (arboard) once - keeping it alive avoids macOS issues
        let clipboard = arboard::Clipboard::new()
            .map_err(|e| log::error!("Failed to init clipboard: {}", e))
            .ok();

        // Initialize background event channel
        let (bg_event_tx, bg_event_rx) = mpsc::channel();

        // Initialize LLM Orchestrator with authentication
        // Reads from .env file (loaded in main.rs via dotenvy)
        let auth_config = AuthConfig::from_env();

        let (llm_client, auth_status_message): (Arc<dyn LLMClientTrait>, Option<String>) =
            if auth_config.is_configured() {
                // Safe: is_configured() guarantees these are Some
                let backend_url = auth_config
                    .backend_url
                    .clone()
                    .expect("backend_url must be Some when is_configured() returns true");
                let api_key = auth_config
                    .api_key
                    .clone()
                    .expect("api_key must be Some when is_configured() returns true");

                // Authenticate at startup
                let authenticator = HttpAuthenticator::new(backend_url.clone());
                let auth_result =
                    runtime.block_on(async { authenticator.authenticate(&api_key).await });

                match auth_result {
                    Ok(response) if response.success => {
                        log::info!("Authentication successful: {}", response.message);
                        (
                            Arc::new(HttpLLMClient::new(backend_url, api_key)),
                            Some(format!(
                                "\x1b[1;32mConnected to LLM Backend: {}\x1b[0m",
                                response.message
                            )),
                        )
                    }
                    Ok(response) => {
                        log::warn!("Authentication rejected: {}", response.message);
                        (
                            Arc::new(crate::llm::MockLLMClient::new()),
                            Some(format!(
                                "\x1b[1;31mAuth rejected: {} (Using Mock LLM)\x1b[0m",
                                response.message
                            )),
                        )
                    }
                    Err(e) => {
                        log::error!("Authentication failed: {}", e);
                        (
                            Arc::new(crate::llm::MockLLMClient::new()),
                            Some(format!(
                                "\x1b[1;31mAuth failed: {} (Using Mock LLM)\x1b[0m",
                                e
                            )),
                        )
                    }
                }
            } else {
                log::warn!("No API key configured, using Mock LLM Client");
                (
                    Arc::new(crate::llm::MockLLMClient::new()),
                    Some("\x1b[1;33mNo API key configured (Using Mock LLM - Set ANTHROPIC_API_KEY to connect)\x1b[0m".to_string())
                )
            };

        let orchestrator = Arc::new(NaturalLanguageOrchestrator::new(llm_client));

        // Create tiles and track the initial pane's tile ID
        let mut tiles = Tiles::default();
        let initial_tile_id = tiles.insert_pane(initial_session_id);
        let tree = Tree::new("terminal_tiles", initial_tile_id, tiles);

        // Initialize session to TileId mapping
        let mut session_tile_ids = HashMap::new();
        session_tile_ids.insert(initial_session_id, initial_tile_id);

        Self {
            // Sessions
            sessions,
            active_session_id: initial_session_id,
            next_session_id: 1, // Next ID after 0
            theme,
            should_quit: false,
            runtime,
            theme_applied: false,
            char_width: rendering::CHAR_WIDTH,
            char_height: rendering::CHAR_HEIGHT,
            startup_time: Instant::now(),
            cursor_blink_visible: true,
            last_cursor_blink: Instant::now(),
            // Initialize to past so we start in "idle" mode (higher PTY throughput)
            last_keyboard_time: Instant::now() - std::time::Duration::from_secs(1),
            had_window_focus: false,
            font_id: FontId::new(rendering::FONT_SIZE, FontFamily::Monospace),
            keyboard_handler: KeyboardHandler::new(),
            input_classifier: InputClassifier::new(),
            current_input_buffer: String::new(),
            current_command_buffer: String::new(),
            // LLM & Orchestration
            orchestrator,
            bg_event_tx,
            bg_event_rx,
            llm_cancel_token: None,
            // Pre-allocate render buffers (reused each frame to avoid allocations)
            render_bg_rects: Vec::with_capacity(32),
            render_text_runs: Vec::with_capacity(32),
            render_text_buffer: String::with_capacity(256),
            render_decorations: Vec::with_capacity(8),
            // Clipboard (kept alive for immediate OS access)
            clipboard,
            // Scrollbar logic
            scrollbar: Scrollbar::new(),
            // Authentication status
            auth_status_message,
            // Split view tiles
            tiles: Some(tree),
            // Session to TileId mapping (for pane removal)
            session_tile_ids,
        }
    }

    /// Get the active session (immutable).
    fn active_session(&self) -> Option<&TerminalSession> {
        self.sessions.get(&self.active_session_id)
    }

    /// Get the active session (mutable).
    fn active_session_mut(&mut self) -> Option<&mut TerminalSession> {
        self.sessions.get_mut(&self.active_session_id)
    }

    /// Create a new terminal session and return its ID.
    fn create_session(&mut self) -> SessionId {
        let id = self.next_session_id;
        self.next_session_id += 1;

        let session = TerminalSession::new(id, self.runtime.handle());
        self.sessions.insert(id, session);

        log::info!("Created new session {}", id);
        id
    }

    /// Close a session by ID.
    fn close_session(&mut self, session_id: SessionId) {
        if self.sessions.remove(&session_id).is_some() {
            log::info!("Closed session {}", session_id);

            // Remove the pane from egui_tiles using remove_recursively
            // IMPORTANT: tree.tiles.remove() only removes from storage but leaves
            // dangling references in the tree structure. remove_recursively() properly
            // cleans up parent-child relationships.
            if let Some(tile_id) = self.session_tile_ids.remove(&session_id)
                && let Some(ref mut tree) = self.tiles
            {
                tree.remove_recursively(tile_id);
                log::debug!("Removed tile {:?} for session {}", tile_id, session_id);
            }

            // If we closed the active session, switch to another one
            if self.active_session_id == session_id
                && let Some(&new_id) = self.sessions.keys().next()
            {
                self.active_session_id = new_id;
                log::info!("Switched to session {}", new_id);
            }

            // If no sessions left, quit
            if self.sessions.is_empty() {
                log::info!("All sessions closed, quitting application");
                self.should_quit = true;
            }

            // Mark remaining sessions for repaint after pane close
            for session in self.sessions.values_mut() {
                session.needs_repaint = true;
            }
        }
    }

    /// Initialize shell with custom prompt for a session.
    fn initialize_shell(&mut self, session_id: SessionId) {
        let session = match self.sessions.get_mut(&session_id) {
            Some(s) => s,
            None => return,
        };

        if session.shell_initialized {
            return;
        }

        // Wait for shell to fully initialize
        if session.startup_time.elapsed() < timing::SHELL_INIT_DELAY {
            return;
        }

        session.shell_initialized = true;

        // Set custom prompt with |~| prefix (#c6d0d6 = rgb 198,208,214)
        // Also inject command_not_found hooks to trigger LLM on error
        let init_commands = if std::env::var("SHELL").unwrap_or_default().contains("zsh") {
            "export PROMPT=$'%{\\e[38;2;198;208;214m%}|~| %n@%m:%~%# %{\\e[0m%}'\n\
             command_not_found_handler() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
             clear\n"
        } else {
            "export PS1=$'\\[\\e[38;2;198;208;214m\\]|~| \\u@\\h:\\w\\$ \\[\\e[0m\\]'\n\
             command_not_found_handle() { printf \"\\033]777;CommandNotFound;%s\\033\\\\\" \"$1\"; return 127; }\n\
             clear\n"
        };

        session.send_to_pty(init_commands.as_bytes());

        // Print welcome message with auth status (only for first session)
        if session_id == 0
            && let Some(ref status_msg) = self.auth_status_message
        {
            let msg = format!("\r\nInfraware Terminal Ready - {}\r\n", status_msg);
            session
                .vte_parser
                .advance(&mut session.terminal_handler, msg.as_bytes());
        }

        log::info!(
            "Session {}: Shell initialized with custom prompt",
            session_id
        );
    }

    /// Poll PTY output for all sessions.
    /// Rate-limited to allow Ctrl+C to work even with heavy output (cat /dev/zero).
    /// Uses adaptive throttling: lower limit during keyboard activity for Ctrl+C responsiveness,
    /// higher limit when idle for better burst throughput (cat large_file).
    /// Returns true if any output was processed (for smart repaint).
    fn poll_all_sessions(&mut self) -> bool {
        let mut any_output = false;
        // PERFORMANCE: SmallVec avoids heap allocation for common cases
        let mut sessions_to_close: SmallVec<[SessionId; 2]> = SmallVec::new();
        let mut completed_commands: SmallVec<[(SessionId, String, String); 1]> = SmallVec::new();

        // Adaptive PTY throttle: use lower limit during keyboard activity for Ctrl+C responsiveness
        // After 200ms of no keyboard input, switch to higher throughput mode
        let byte_limit = if self.last_keyboard_time.elapsed() < std::time::Duration::from_millis(200)
        {
            rendering::MAX_BYTES_PER_FRAME_ACTIVE
        } else {
            rendering::MAX_BYTES_PER_FRAME_IDLE
        };

        // Collect session IDs first to avoid borrow conflicts
        let session_ids: SmallVec<[SessionId; 4]> = self.sessions.keys().copied().collect();

        for session_id in session_ids {
            let session = match self.sessions.get_mut(&session_id) {
                Some(s) => s,
                None => continue,
            };

            // Poll this session's PTY output with adaptive byte limit
            // Returns (had_output, command_completed) tuple
            let (had_output, command_completed) = session.poll_pty_output(byte_limit);
            if had_output {
                any_output = true;
            }

            // Check if session should be closed
            if session.should_close {
                sessions_to_close.push(session_id);
                continue;
            }

            // Handle command completion for HITL flow
            // poll_pty_output() feeds output to OutputCapture and detects prompt
            if command_completed && let AppMode::ExecutingCommand { ref command } = session.mode {
                let cmd = command.clone();
                let output = session.output_capture.take_output();
                log::info!(
                    "Session {}: Command '{}' completed, output length: {} chars",
                    session_id,
                    cmd,
                    output.len()
                );
                completed_commands.push((session_id, cmd, output));
                session.mode = AppMode::WaitingLLM;
            }
        }

        // Close sessions that exited
        let sessions_closed = !sessions_to_close.is_empty();
        for session_id in sessions_to_close {
            self.close_session(session_id);
            // Note: egui_tiles pane removal is handled through the Behavior trait
        }

        // Resume LLM for completed commands
        for (session_id, cmd, output) in completed_commands {
            log::info!(
                "Session {}: Sending command output to backend for '{}'",
                session_id,
                cmd
            );
            self.resume_with_command_output(cmd, output);
        }

        // Return true if there was output OR sessions were closed (forces immediate repaint)
        any_output || sessions_closed
    }

    /// Send data to the active session's PTY.
    fn send_to_pty(&self, data: &[u8]) {
        if let Some(session) = self.active_session() {
            session.send_to_pty(data);
        } else {
            log::warn!("No active session!");
        }
    }

    /// Poll background events (LLM results, etc.)
    fn poll_background_events(&mut self) {
        while let Ok(event) = self.bg_event_rx.try_recv() {
            let session = match self.active_session_mut() {
                Some(s) => s,
                None => continue,
            };

            log::info!(
                "Received background event: {:?}, current mode: {:?}",
                event,
                session.mode.name()
            );
            match event {
                AppBackgroundEvent::LlmResult(result) => {
                    // Stream ended - we received a result
                    session.agent_state.end_stream();
                    match result {
                        LLMQueryResult::Complete(text) => {
                            log::info!(
                                "LLM query complete, response length: {} chars, transitioning to Normal",
                                text.len()
                            );

                            // Set mode FIRST to stop throbber immediately
                            session.mode = AppMode::Normal;

                            // If response is empty, just transition to Normal without any output
                            if text.is_empty() {
                                log::debug!("Empty response, no output to render");
                                continue;
                            }

                            // Then render response lines
                            let lines = self.orchestrator.render_response(&text);
                            log::debug!("Rendered {} lines to display", lines.len());

                            // Re-acquire mutable borrow after orchestrator call
                            let session = self.active_session_mut().unwrap();

                            // Start with newline to avoid overwriting current prompt
                            session
                                .vte_parser
                                .advance(&mut session.terminal_handler, b"\r\n");

                            let last_idx = lines.len().saturating_sub(1);
                            for (i, line) in lines.iter().enumerate() {
                                session
                                    .vte_parser
                                    .advance(&mut session.terminal_handler, line.as_bytes());
                                if i < last_idx {
                                    session
                                        .vte_parser
                                        .advance(&mut session.terminal_handler, b"\r\n");
                                }
                            }

                            // Clear shell buffer and trigger fresh prompt
                            session.send_to_pty(b"\x15\n");
                        }
                        LLMQueryResult::CommandApproval { command, message } => {
                            log::info!("LLM requested command approval: {}", command);

                            // Set mode FIRST to stop throbber immediately
                            session.mode = AppMode::AwaitingApproval {
                                command: command.clone(),
                                message: message.clone(),
                            };

                            // Then display the approval prompt with command highlighted
                            let message_formatted = message.replace('\n', "\r\n");
                            let prompt = format!(
                                "\r\n\x1b[1;33m{}\x1b[0m\r\n\r\n\x1b[1;36mCommand:\x1b[0m \x1b[1m{}\x1b[0m\r\n\r\n\x1b[90mType 'y' to approve, 'n' to reject:\x1b[0m ",
                                message_formatted, command
                            );
                            session
                                .vte_parser
                                .advance(&mut session.terminal_handler, prompt.as_bytes());
                        }
                        LLMQueryResult::Question { question, options } => {
                            log::info!("LLM asked a question: {}", question);

                            // Set mode FIRST to stop throbber immediately
                            session.mode = AppMode::AwaitingAnswer {
                                question: question.clone(),
                                options: options.clone(),
                            };

                            // Then display the question
                            let question_formatted = question.replace('\n', "\r\n");
                            let mut prompt = format!(
                                "\r\n\x1b[1;33mAgent Question:\x1b[0m\r\n  {}\r\n",
                                question_formatted
                            );

                            // Show options if provided
                            if let Some(ref opts) = options {
                                prompt.push_str("\x1b[90m  Options:\x1b[0m\r\n");
                                for (i, opt) in opts.iter().enumerate() {
                                    let opt_formatted = opt.replace('\n', "\r\n");
                                    prompt.push_str(&format!(
                                        "    {}. {}\r\n",
                                        i + 1,
                                        opt_formatted
                                    ));
                                }
                            }

                            prompt.push_str("\r\n\x1b[90mType your answer:\x1b[0m ");
                            session
                                .vte_parser
                                .advance(&mut session.terminal_handler, prompt.as_bytes());
                        }
                    }
                }
                AppBackgroundEvent::LlmError(err) => {
                    // Stream ended - we received an error
                    session.agent_state.end_stream();
                    log::error!("LLM query error: {}", err);
                    // No trailing newline - shell's echo of \n provides it
                    let error_msg = format!("\x1b[31mError: {}\x1b[0m", err);
                    session
                        .vte_parser
                        .advance(&mut session.terminal_handler, error_msg.as_bytes());
                    session.mode = AppMode::Normal;

                    // Clear shell buffer and trigger fresh prompt
                    session.send_to_pty(b"\x15\n");
                }
            }
        }
    }

    /// Start an LLM query in a background task
    fn start_llm_query(&mut self, query: String) {
        log::info!("Starting LLM query: {}", query);

        // Set mode on active session
        if let Some(session) = self.active_session_mut() {
            session.mode = AppMode::WaitingLLM;
            session.agent_state.start_stream();
        }

        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();

        // Create cancellation token and save it for Ctrl+C cancellation
        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.llm_cancel_token = Some(cancel_token.clone());

        self.runtime.spawn(async move {
            log::info!("Background task started for query: {}", query);

            match orchestrator.query(&query, cancel_token).await {
                Ok(result) => {
                    log::info!("LLM query succeeded, sending result to channel");
                    if let Err(e) = tx.send(AppBackgroundEvent::LlmResult(result)) {
                        log::error!("Failed to send LLM result to channel: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("LLM query failed: {}", e);
                    if let Err(send_err) = tx.send(AppBackgroundEvent::LlmError(e.to_string())) {
                        log::error!("Failed to send error to channel: {}", send_err);
                    }
                }
            }
            log::info!("Background task completed");
        });
    }

    /// Send SIGINT to the active session's foreground process group.
    fn send_sigint(&self) {
        if let Some(session) = self.active_session() {
            session.send_sigint();
        }
    }

    /// Resize a session's PTY to match pane size.
    /// Resize a session's PTY to match the pane size.
    /// Returns true if the resize was performed.
    fn resize_session_pty(&mut self, session_id: SessionId, cols: u16, rows: u16) -> bool {
        if let Some(session) = self.sessions.get_mut(&session_id) {
            // Per-session debounce is now handled inside session.resize_pty()
            session.resize_pty(cols, rows, self.runtime.handle())
        } else {
            false
        }
    }

    /// Handle keyboard input using the extracted KeyboardHandler.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Check if active session is in HITL mode
        let is_hitl = self.active_session().is_some_and(|s| {
            matches!(
                s.mode,
                AppMode::AwaitingApproval { .. } | AppMode::AwaitingAnswer { .. }
            )
        });

        if is_hitl {
            self.handle_hitl_keyboard(ctx);
            return;
        }

        // Process keyboard input and get actions
        let actions = self.keyboard_handler.process(ctx);

        // Track keyboard activity for adaptive PTY throttling
        if !actions.is_empty() {
            self.last_keyboard_time = Instant::now();
            if let Some(session) = self.active_session() {
                log::debug!(
                    "Keyboard actions: {} actions, mode: {:?}",
                    actions.len(),
                    session.mode.name()
                );
            }
        }

        // Execute each action
        for action in actions {
            // Clear prompt detection on any user input
            if matches!(action, KeyboardAction::SendBytes(_))
                && let Some(session) = self.active_session_mut()
            {
                session.prompt_detector.clear();
            }

            match action {
                KeyboardAction::SendBytes(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    let mut handled = false;
                    for c in text.chars() {
                        if c == '\r' || c == '\n' {
                            // Classify the input before sending
                            let input = self.current_command_buffer.trim().to_string();
                            match self.input_classifier.classify(&input) {
                                InputType::NaturalLanguage(query) => {
                                    log::info!("Input classified as NaturalLanguage: {}", query);
                                    self.current_command_buffer.clear();
                                    // Clear the shell's input buffer
                                    self.send_to_pty(b"\x15");
                                    // Visual feedback: move to next line
                                    if let Some(session) = self.active_session_mut() {
                                        session
                                            .vte_parser
                                            .advance(&mut session.terminal_handler, b"\r\n");
                                    }
                                    self.start_llm_query(query);
                                    handled = true;
                                    break;
                                }
                                InputType::Command(_) | InputType::Empty => {
                                    self.current_command_buffer.clear();
                                }
                            }
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

                    // Cancel LLM stream if active
                    if let Some(token) = self.llm_cancel_token.take() {
                        log::info!("Cancelling active LLM stream");
                        token.cancel();
                        if let Some(session) = self.active_session_mut() {
                            session.agent_state.end_stream();
                            session.mode = AppMode::Normal;
                        }
                    }

                    // Pause output reading briefly
                    if let Some(session) = self.active_session_mut() {
                        session.output_pause_until =
                            Some(Instant::now() + std::time::Duration::from_millis(200));
                    }
                }
                KeyboardAction::Copy => {
                    self.copy_selection_to_clipboard(ctx);
                }
                KeyboardAction::Paste => {
                    self.perform_paste();
                }
                KeyboardAction::SplitHorizontal => {
                    self.split_horizontal();
                }
                KeyboardAction::SplitVertical => {
                    self.split_vertical();
                }
            }
        }
    }

    /// Handle keyboard input specifically for Human-in-the-Loop interactions.
    fn handle_hitl_keyboard(&mut self, ctx: &egui::Context) {
        let actions = self.keyboard_handler.process(ctx);

        for action in actions {
            // Clear prompt detection on any user input
            if matches!(action, KeyboardAction::SendBytes(_))
                && let Some(session) = self.active_session_mut()
            {
                session.prompt_detector.clear();
            }

            match action {
                KeyboardAction::SendBytes(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    for c in text.chars() {
                        if c == '\r' || c == '\n' {
                            // Echo newline and submit
                            if let Some(session) = self.active_session_mut() {
                                session
                                    .vte_parser
                                    .advance(&mut session.terminal_handler, b"\r\n");
                            }
                            self.submit_hitl_input();
                        } else if c == '\x7f' || c == '\x08' {
                            // Backspace: remove from buffer and erase character on screen
                            if self.current_input_buffer.pop().is_some()
                                && let Some(session) = self.active_session_mut()
                            {
                                session
                                    .vte_parser
                                    .advance(&mut session.terminal_handler, b"\x08 \x08");
                            }
                        } else if !c.is_control() {
                            // Echo the character to terminal and add to buffer
                            self.current_input_buffer.push(c);
                            let char_bytes = c.to_string();
                            if let Some(session) = self.active_session_mut() {
                                session
                                    .vte_parser
                                    .advance(&mut session.terminal_handler, char_bytes.as_bytes());
                            }
                        }
                    }
                }
                KeyboardAction::SendSigInt => {
                    log::info!("Cancelling HITL interaction");
                    self.current_input_buffer.clear();
                    // Show cancellation message
                    if let Some(session) = self.active_session_mut() {
                        session.vte_parser.advance(
                            &mut session.terminal_handler,
                            b"\r\n\x1b[33m(cancelled)\x1b[0m\r\n",
                        );
                        session.mode = AppMode::Normal;
                    }
                }
                _ => {}
            }
        }
    }

    /// Submit current input buffer for the active HITL interaction.
    fn submit_hitl_input(&mut self) {
        let input = std::mem::take(&mut self.current_input_buffer);

        // Get mode from active session
        let mode = match self.active_session() {
            Some(s) => s.mode.clone(),
            None => return,
        };

        match mode {
            AppMode::AwaitingApproval { command, .. } => {
                let approved = crate::orchestrators::HitlOrchestrator::parse_approval(&input);
                if approved {
                    // SECURITY: Validate command before execution
                    let validation = validate_command(&command);

                    if validation.is_blocked() {
                        log::warn!("Blocked dangerous command: {}", command);
                        let warning = match &validation {
                            ValidationResult::Blocked { reason } => {
                                format!(
                                    "\r\n\x1b[91mBLOCKED: {}\x1b[0m\r\n\
                                     \x1b[33mCommand not executed for security reasons.\x1b[0m\r\n",
                                    reason
                                )
                            }
                            _ => unreachable!(),
                        };
                        if let Some(session) = self.active_session_mut() {
                            session
                                .vte_parser
                                .advance(&mut session.terminal_handler, warning.as_bytes());
                            session.mode = AppMode::Normal;
                            session.send_to_pty(b"\x15\n");
                        }
                        return;
                    }

                    // Show warning for risky commands but allow execution
                    if let ValidationResult::Warning { reason } = &validation {
                        log::info!("Warning for command {}: {}", command, reason);
                        let warning = format!("\x1b[33mWarning: {}\x1b[0m\r\n", reason);
                        if let Some(session) = self.active_session_mut() {
                            session
                                .vte_parser
                                .advance(&mut session.terminal_handler, warning.as_bytes());
                        }
                    }

                    log::info!("User approved command: {}", command);
                    // Echo approval to terminal
                    let echo = format!("Approved: {}\r\n", command);

                    if let Some(session) = self.active_session_mut() {
                        session
                            .vte_parser
                            .advance(&mut session.terminal_handler, echo.as_bytes());

                        // Start capturing output before sending command
                        session.output_capture.start(&command);

                        // Send command to PTY
                        let cmd_with_newline = format!("{}\n", command);
                        session.send_to_pty(cmd_with_newline.as_bytes());

                        // Enter ExecutingCommand mode
                        session.mode = AppMode::ExecutingCommand {
                            command: command.clone(),
                        };
                        log::debug!("Entered ExecutingCommand mode for: {}", command);
                    }
                } else {
                    log::info!("User rejected command: {}", command);
                    // Show rejection message
                    if let Some(session) = self.active_session_mut() {
                        session.vte_parser.advance(
                            &mut session.terminal_handler,
                            b"\r\n\x1b[33mCommand rejected.\x1b[0m\r\n",
                        );
                    }

                    // Notify backend that command was rejected
                    self.resume_llm_rejected();
                }
            }
            AppMode::AwaitingAnswer { .. } => {
                log::info!("User answered question: {}", input);
                self.resume_llm_with_answer(input);
            }
            _ => {
                if let Some(session) = self.active_session_mut() {
                    session.mode = AppMode::Normal;
                }
            }
        }
    }

    /// Resume LLM run after approval (sets WaitingLLM mode immediately).
    #[allow(dead_code)]
    fn resume_llm_run(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.mode = AppMode::WaitingLLM;
        }
        self.resume_llm_run_background();
    }

    /// Resume LLM run in background without changing mode.
    fn resume_llm_run_background(&mut self) {
        // Start agent stream tracking
        if let Some(session) = self.active_session_mut() {
            session.agent_state.start_stream();
        }
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.llm_cancel_token = Some(cancel_token.clone());

        self.runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM run cancelled");
                }
                result = orchestrator.resume_run() => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Resume LLM run with a text answer.
    fn resume_llm_with_answer(&mut self, answer: String) {
        if let Some(session) = self.active_session_mut() {
            session.mode = AppMode::WaitingLLM;
            session.agent_state.start_stream();
        }
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();

        // Create cancellation token for Ctrl+C cancellation
        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.llm_cancel_token = Some(cancel_token.clone());

        self.runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM answer run cancelled");
                }
                result = orchestrator.resume_with_answer(&answer) => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Resume LLM run with command output from PTY execution.
    fn resume_with_command_output(&mut self, command: String, output: String) {
        // Start agent stream tracking
        if let Some(session) = self.active_session_mut() {
            session.agent_state.start_stream();
        }
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.llm_cancel_token = Some(cancel_token.clone());

        self.runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM command output run cancelled");
                }
                result = orchestrator.resume_with_command_output(&command, &output) => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Resume LLM run with rejection (user rejected the command).
    fn resume_llm_rejected(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.mode = AppMode::WaitingLLM;
            session.agent_state.start_stream();
        }
        let orchestrator = self.orchestrator.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = tokio_util::sync::CancellationToken::new();
        self.llm_cancel_token = Some(cancel_token.clone());

        self.runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM rejected run cancelled");
                }
                result = orchestrator.resume_rejected() => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
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

    /// Convert screen coordinates to grid (row, col).
    /// Returns visible row index (0-based from top of visible area).
    fn screen_to_grid(&self, pos: Pos2, rect: Rect, session: &TerminalSession) -> (usize, usize) {
        let col = ((pos.x - rect.left()) / self.char_width).max(0.0) as usize;
        let row = ((pos.y - rect.top()) / self.char_height).max(0.0) as usize;

        // Clamp to valid range
        let max_col = session.terminal_size.0.saturating_sub(1) as usize;
        let max_row = session.terminal_size.1.saturating_sub(1) as usize;

        (row.min(max_row), col.min(max_col))
    }

    /// Copy selected text from active session to clipboard using arboard.
    fn copy_selection_to_clipboard(&mut self, ctx: &egui::Context) {
        let Some(session) = self.sessions.get(&self.active_session_id) else {
            log::info!("No active session for copy");
            return;
        };

        log::info!(
            "copy_selection_to_clipboard called, selection: {:?}",
            session.selection
        );

        let Some(ref sel) = session.selection else {
            log::info!("No selection exists");
            return;
        };

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

        let text = session
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
                let use_bracketed = self
                    .active_session()
                    .is_some_and(|s| s.terminal_handler.bracketed_paste_enabled());

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

    /// Render terminal grid for a specific session.
    /// `has_focus` is passed to avoid redundant ctx.input() calls.
    ///
    /// PERFORMANCE OPTIMIZATIONS:
    /// - Single session lookup at start, extract all needed data
    /// - Zero-allocation visible_row() iteration
    /// - Shared text buffer instead of per-row String allocations
    /// - Cached terminal_id per session
    /// - Optimized selection checks (early exit if no selection)
    fn render_terminal(&mut self, ui: &mut egui::Ui, session_id: SessionId, has_focus: bool) {
        // === PHASE 1: Extract all session data upfront (single HashMap lookup) ===
        let session = match self.sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };

        // Extract immutable data from session (avoids repeated HashMap lookups)
        let terminal_id = session.terminal_egui_id;
        // NOTE: column_x_coords is accessed via reference in Phase 5 (no clone needed)
        let show_throbber = session.should_show_throbber();
        let shell_initialized = session.shell_initialized;

        // Extract grid state
        let grid = session.terminal_handler.grid();
        let scroll_offset = grid.scroll_offset();
        let max_scroll = grid.max_scroll_offset();
        let visible_row_count = grid.visible_row_count();
        let (cursor_row, cursor_col) = grid.cursor_position();
        let cursor_visible = grid.cursor_visible();
        let cols = grid.size().1 as usize;

        let available = ui.available_size();
        let size = Vec2::new(available.x, available.y);

        // Check for layout desync: if the grid is smaller than the available space,
        // we might need a reactive repaint to let the resize logic catch up.
        // The 1-column tolerance (saturating_sub(1)) accounts for floating-point
        // rounding differences between egui's available_size and our char_width calc.
        let expected_cols = (available.x / self.char_width).floor() as u16;
        let (_, current_cols) = session.terminal_handler.grid().size();
        if current_cols < expected_cols.saturating_sub(1) {
            ui.ctx().request_repaint();
        }

        let (response, painter) = ui.allocate_painter(size, Sense::click().union(Sense::drag()));
        let rect = response.rect;
        let scrollbar_area = self.scrollbar.area(rect);

        // Request keyboard focus when terminal is clicked
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && !scrollbar_area.contains(pos)
        {
            ui.memory_mut(|mem| mem.request_focus(terminal_id));
            self.active_session_id = session_id;
            // Clear selection on this session
            if let Some(session) = self.sessions.get_mut(&session_id) {
                session.selection = None;
            }
        }

        // Handle mouse drag for text selection (per-session selection)
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
            && !scrollbar_area.contains(pos)
        {
            // Calculate grid position first (immutable borrow), then update selection (mutable)
            let grid_pos = self
                .sessions
                .get(&session_id)
                .map(|s| self.screen_to_grid(pos, rect, s));
            if let Some((row, col)) = grid_pos
                && let Some(session) = self.sessions.get_mut(&session_id)
            {
                session.selection = Some(TextSelection::new(row, col));
            }
        }

        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
            && !self.scrollbar.is_dragging()
        {
            // Calculate grid position first (immutable borrow), then update selection (mutable)
            let grid_pos = self
                .sessions
                .get(&session_id)
                .map(|s| self.screen_to_grid(pos, rect, s));
            if let Some((row, col)) = grid_pos
                && let Some(session) = self.sessions.get_mut(&session_id)
                && let Some(ref mut sel) = session.selection
            {
                sel.update_end(row, col);
            }
        }

        if response.drag_stopped()
            && let Some(session) = self.sessions.get_mut(&session_id)
            && let Some(ref mut sel) = session.selection
        {
            sel.active = false;
        }

        // Handle mouse wheel scrolling - ONLY if mouse is over THIS pane
        // IMPORTANT: smooth_scroll_delta is global, so we must check hovered() first
        if response.hovered() {
            let scroll_delta = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll_delta != 0.0
                && let Some(session) = self.sessions.get_mut(&session_id)
            {
                let lines = (scroll_delta / self.char_height).round() as i32;
                let grid = session.terminal_handler.grid_mut();
                if lines > 0 {
                    grid.scroll_view_up(lines as usize);
                } else if lines < 0 {
                    grid.scroll_view_down((-lines) as usize);
                }
            }
        }

        // === PHASE 3: Scrollbar handling ===
        if let Some(action) = self.scrollbar.show(
            ui,
            &painter,
            rect,
            scroll_offset,
            max_scroll,
            visible_row_count,
        ) && let Some(session) = self.sessions.get_mut(&session_id)
        {
            match action {
                ScrollAction::Up(n) => session.terminal_handler.grid_mut().scroll_view_up(n),
                ScrollAction::Down(n) => session.terminal_handler.grid_mut().scroll_view_down(n),
                ScrollAction::To(offset) => {
                    session.terminal_handler.grid_mut().scroll_to_offset(offset)
                }
            }
        }

        // === PHASE 4: Rendering (re-acquire session for grid access) ===
        let session = match self.sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };
        let grid = session.terminal_handler.grid();
        // PERFORMANCE: Use reference instead of clone (saves ~320 bytes/frame)
        let column_x_coords: &[f32] = &session.column_x_coords;

        // Fill background once
        painter.rect_filled(rect, 0.0, self.theme.background);

        // PERFORMANCE: Pre-compute selection bounds once if selection exists (per-session)
        let selection_bounds = session.selection.as_ref().map(|sel| sel.normalized());

        // Cache font_id reference
        let font_id = &self.font_id;
        let char_width = self.char_width;
        let char_height = self.char_height;
        let theme_background = self.theme.background;
        let theme_selection = self.theme.selection;

        // === PHASE 5: Row-by-row rendering (zero-allocation iteration) ===
        // PERFORMANCE: visible_rows_iter() calculates bounds once instead of per-row
        for (row_idx, row) in grid.visible_rows_iter().enumerate() {
            let y = rect.top() + row_idx as f32 * char_height;

            // Clear buffers (O(1) - reuses capacity)
            self.render_bg_rects.clear();
            self.render_text_runs.clear();
            self.render_text_buffer.clear();
            self.render_decorations.clear();

            // State for batching
            let mut bg_start: Option<(usize, Color32)> = None;
            let mut run_start: Option<(usize, Color32)> = None;

            let row_len = row.len().min(cols);

            // PERFORMANCE: Direct column lookup (bounds check removed)
            // Safety: column_x_coords has `cols` elements, idx is always < row_len <= cols
            #[inline(always)]
            fn col_x(idx: usize, column_x_coords: &[f32]) -> f32 {
                column_x_coords[idx]
            }

            for (col_idx, cell) in row.iter().take(row_len).enumerate() {
                // PERFORMANCE: Calculate colors once per cell (was 6 calls, now 2)
                // Handles reverse attribute by swapping fg/bg at conversion time
                let (cell_fg, cell_bg) = if cell.attrs.reverse() {
                    (self.color_to_egui(cell.bg), self.color_to_egui(cell.fg))
                } else {
                    (self.color_to_egui(cell.fg), self.color_to_egui(cell.bg))
                };

                // PERFORMANCE: Apply dim attribute once (was per-use)
                let cell_fg = if cell.attrs.dim() {
                    Color32::from_rgba_unmultiplied(cell_fg.r(), cell_fg.g(), cell_fg.b(), 128)
                } else {
                    cell_fg
                };

                // PERFORMANCE: Optimized selection check - only if selection exists
                let is_selected = selection_bounds.as_ref().is_some_and(|(start, end)| {
                    if row_idx < start.row || row_idx > end.row {
                        false
                    } else if row_idx == start.row && row_idx == end.row {
                        col_idx >= start.col && col_idx <= end.col
                    } else if row_idx == start.row {
                        col_idx >= start.col
                    } else if row_idx == end.row {
                        col_idx <= end.col
                    } else {
                        true
                    }
                });

                // --- Background batching ---
                let bg = if is_selected { theme_selection } else { cell_bg };

                if is_selected || bg != theme_background {
                    match bg_start {
                        Some((_start, color)) if color == bg => {}
                        Some((start, color)) => {
                            let width = (col_idx - start) as f32 * char_width;
                            self.render_bg_rects.push((
                                col_x(start, column_x_coords),
                                width,
                                color,
                            ));
                            bg_start = Some((col_idx, bg));
                        }
                        None => {
                            bg_start = Some((col_idx, bg));
                        }
                    }
                } else if let Some((start, color)) = bg_start.take() {
                    let width = (col_idx - start) as f32 * char_width;
                    self.render_bg_rects.push((
                        col_x(start, column_x_coords),
                        width,
                        color,
                    ));
                }

                // --- Text batching (using shared buffer) ---
                if cell.ch == ' ' || cell.attrs.hidden() {
                    if let Some((start, color)) = run_start.take() {
                        let end_idx = self.render_text_buffer.len();
                        if end_idx > 0 {
                            self.render_text_runs.push((
                                col_x(start, column_x_coords),
                                end_idx,
                                color,
                            ));
                        }
                    }
                } else {
                    match run_start {
                        Some((_start, color)) if color == cell_fg => {
                            self.render_text_buffer.push(cell.ch);
                        }
                        Some((start, color)) => {
                            let end_idx = self.render_text_buffer.len();
                            if end_idx > 0 {
                                self.render_text_runs.push((
                                    col_x(start, column_x_coords),
                                    end_idx,
                                    color,
                                ));
                            }
                            run_start = Some((col_idx, cell_fg));
                            self.render_text_buffer.push(cell.ch);
                        }
                        None => {
                            run_start = Some((col_idx, cell_fg));
                            self.render_text_buffer.push(cell.ch);
                        }
                    }
                }

                // --- Decorations ---
                if cell.attrs.underline() || cell.attrs.strikethrough() {
                    self.render_decorations.push((
                        col_x(col_idx, column_x_coords),
                        cell.attrs.underline(),
                        cell.attrs.strikethrough(),
                        cell_fg,
                    ));
                }
            }

            // Flush remaining background
            if let Some((start, color)) = bg_start {
                let width = (row_len - start) as f32 * char_width;
                self.render_bg_rects.push((
                    col_x(start, column_x_coords),
                    width,
                    color,
                ));
            }

            // Flush remaining text
            if run_start.is_some() {
                let end_idx = self.render_text_buffer.len();
                if let Some((start, color)) = run_start
                    && end_idx > 0
                {
                    self.render_text_runs.push((
                        col_x(start, column_x_coords),
                        end_idx,
                        color,
                    ));
                }
            }

            // === DRAW PHASE ===
            render_backgrounds(&painter, rect, y, char_height, &self.render_bg_rects);
            render_text_runs_buffered(
                &painter,
                rect,
                y,
                font_id,
                &self.render_text_buffer,
                &self.render_text_runs,
            );
            render_decorations(
                &painter,
                rect,
                y,
                char_width,
                char_height,
                &self.render_decorations,
            );
        }

        // === PHASE 6: Cursor/Throbber rendering ===
        if show_throbber && scroll_offset == 0 {
            let frame_idx =
                (self.startup_time.elapsed().as_millis() / 250) as usize % SPINNER_FRAMES.len();
            let frame = SPINNER_FRAMES[frame_idx];
            let row_y = rect.top() + cursor_row as f32 * char_height;
            let spinner_x = rect.left() + cursor_col as f32 * char_width;

            painter.text(
                Pos2::new(spinner_x, row_y),
                egui::Align2::LEFT_TOP,
                frame,
                font_id.clone(),
                Color32::from_rgb(0, 255, 255),
            );
        } else if cursor_visible
            && self.cursor_blink_visible
            && scroll_offset == 0
            && has_focus
            && shell_initialized
        {
            let cursor_col_idx = cursor_col as usize;
            let cursor_x = rect.left()
                + if cursor_col_idx < column_x_coords.len() {
                    column_x_coords[cursor_col_idx]
                } else {
                    cursor_col as f32 * char_width
                };
            let cursor_y = rect.top() + cursor_row as f32 * char_height;
            render_cursor(&painter, cursor_x, cursor_y, char_height, self.theme.cursor);
        }

        // === PHASE 7: Scrollbar rendering ===
        if max_scroll > 0 {
            render_scrollbar(&painter, rect, scroll_offset, max_scroll, visible_row_count);
        }
    }

    /// Split the active pane horizontally (new pane to the right).
    ///
    /// Uses a flatter tree structure by splitting the active pane rather than
    /// wrapping the entire root. This results in better performance with many splits:
    /// - Old: Container[Container[Container[P1, P2], P3], P4] (deep nesting)
    /// - New: Container[P1, Container[P2, Container[P3, P4]]] (flatter)
    pub fn split_horizontal(&mut self) {
        self.split_active_pane(egui_tiles::LinearDir::Horizontal);
    }

    /// Split the active pane vertically (new pane below).
    ///
    /// Uses a flatter tree structure by splitting the active pane rather than
    /// wrapping the entire root. See `split_horizontal` for details.
    pub fn split_vertical(&mut self) {
        self.split_active_pane(egui_tiles::LinearDir::Vertical);
    }

    /// Internal: Split the active pane in the specified direction.
    ///
    /// Creates a container holding [active_pane, new_pane] and replaces the
    /// active pane in the tree with this container.
    fn split_active_pane(&mut self, direction: egui_tiles::LinearDir) {
        let new_session_id = self.create_session();

        // Get the active pane's tile ID
        let Some(&active_tile_id) = self.session_tile_ids.get(&self.active_session_id) else {
            log::warn!(
                "Split failed: no tile found for active session {}",
                self.active_session_id
            );
            return;
        };

        let Some(ref mut tree) = self.tiles else {
            return;
        };

        // IMPORTANT: Get parent BEFORE inserting the new container.
        // After insertion, parent_of(active_tile_id) would return the new container
        // since it also contains active_tile_id as a child.
        let parent_id = tree.tiles.parent_of(active_tile_id);

        // Insert the new pane
        let new_pane_id = tree.tiles.insert_pane(new_session_id);
        self.session_tile_ids.insert(new_session_id, new_pane_id);

        // Create container holding [active_pane, new_pane]
        let container = egui_tiles::Linear::new(direction, vec![active_tile_id, new_pane_id]);
        let container_id = tree.tiles.insert_container(container);

        // Replace active pane with container in the tree
        if let Some(parent_id) = parent_id {
            // Replace active_tile_id with container_id in parent's children
            if let Some(egui_tiles::Tile::Container(parent_container)) =
                tree.tiles.get_mut(parent_id)
            {
                Self::replace_child_in_container(parent_container, active_tile_id, container_id);
            }
        } else {
            // Active pane is root - make container the new root
            *tree = Tree::new(
                "terminal_tiles",
                container_id,
                std::mem::take(&mut tree.tiles),
            );
        }

        let dir_name = match direction {
            egui_tiles::LinearDir::Horizontal => "horizontal",
            egui_tiles::LinearDir::Vertical => "vertical",
        };
        log::info!(
            "Split {}: session {} in pane {:?}, container {:?}",
            dir_name,
            new_session_id,
            new_pane_id,
            container_id
        );
    }

    /// Replace a child tile ID in a container's children list.
    ///
    /// Handles Linear and Tabs containers by directly modifying their
    /// public `children` fields. Grid containers use `replace_at` method.
    fn replace_child_in_container(
        container: &mut egui_tiles::Container,
        old_child: egui_tiles::TileId,
        new_child: egui_tiles::TileId,
    ) {
        match container {
            egui_tiles::Container::Linear(linear) => {
                for child in &mut linear.children {
                    if *child == old_child {
                        *child = new_child;
                        return;
                    }
                }
            }
            egui_tiles::Container::Tabs(tabs) => {
                for child in &mut tabs.children {
                    if *child == old_child {
                        *child = new_child;
                        return;
                    }
                }
            }
            egui_tiles::Container::Grid(grid) => {
                // Grid doesn't expose children directly; find index then use replace_at
                let idx = grid.children().position(|&c| c == old_child);
                if let Some(idx) = idx {
                    let _ = grid.replace_at(idx, new_child);
                    return;
                }
            }
        }
        log::warn!(
            "Failed to replace child {:?} with {:?} in container",
            old_child,
            new_child
        );
    }
}

/// Behavior implementation for egui_tiles.
/// Wraps InfrawareApp for rendering terminal panes.
struct TerminalBehavior<'a> {
    app: &'a mut InfrawareApp,
    has_focus: bool,
}

impl egui_tiles::Behavior<SessionId> for TerminalBehavior<'_> {
    fn tab_title_for_pane(&mut self, pane: &SessionId) -> egui::WidgetText {
        // PERFORMANCE: Use cached title from session to avoid format! allocation
        if let Some(session) = self.app.sessions.get(pane) {
            session.cached_title.as_str().into()
        } else {
            format!("Terminal {}", pane).into()
        }
    }

    fn pane_ui(
        &mut self,
        ui: &mut egui::Ui,
        _tile_id: egui_tiles::TileId,
        pane: &mut SessionId,
    ) -> UiResponse {
        let session_id = *pane;

        // Use a Frame with outer_margin to create the visible gap (split separator)
        // The gap reveals the CentralPanel background (Theme::split_separator)
        egui::Frame::NONE
            .outer_margin(2.0) // 2px margin per side = 4px total gap between panes
            .show(ui, |ui| {
                // Calculate terminal size based on pane size
                let available = ui.available_size();
                let cols = ((available.x / self.app.char_width) as u16).max(20);
                let rows = ((available.y / self.app.char_height) as u16).max(5);

                // Check if resize is needed (triggers repaint if size changed)
                let size_changed = self.app.resize_session_pty(session_id, cols, rows);
                if size_changed {
                    // Force repaint when size changes to ensure layout updates immediately
                    ui.ctx().request_repaint();
                }

                // Render the terminal for this session
                self.app.render_terminal(ui, session_id, self.has_focus);
            });

        UiResponse::None
    }

    fn gap_width(&self, _style: &egui::Style) -> f32 {
        4.0 // 4px separator width
    }

    fn simplification_options(&self) -> egui_tiles::SimplificationOptions {
        egui_tiles::SimplificationOptions {
            all_panes_must_have_tabs: false,
            ..Default::default()
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
        if crate::SIGINT_RECEIVED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            log::info!("System SIGINT received, sending to process group");
            self.send_sigint();
            // Pause output reading briefly for active session
            if let Some(session) = self.active_session_mut() {
                session.output_pause_until =
                    Some(Instant::now() + std::time::Duration::from_millis(200));
            }
        }

        // Poll background events (LLM results, etc.)
        self.poll_background_events();

        // Check for pending LLM query from shell hook (Command Not Found) - active session only
        let pending_llm_query = self
            .active_session_mut()
            .and_then(|s| s.terminal_handler.take_pending_llm_query());
        if let Some(failed_cmd) = pending_llm_query {
            log::info!("Triggering LLM for failed command: {}", failed_cmd);
            let query = format!(
                "I tried to run '{}' but got 'command not found'. What should I do?",
                failed_cmd
            );
            self.start_llm_query(query);
        }

        // Handle keyboard FIRST - ensures Ctrl+C works even during heavy output
        self.handle_keyboard(ctx);

        // Poll PTY output for all sessions
        let pty_had_data = self.poll_all_sessions();

        // Initialize shells for all sessions
        let session_ids: SmallVec<[SessionId; 4]> = self.sessions.keys().copied().collect();
        for session_id in session_ids {
            self.initialize_shell(session_id);
        }

        // Render UI using egui_tiles for split view support
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.split_separator))
            .show(ctx, |ui| {
                // Take the tree temporarily to avoid borrow conflicts
                if let Some(mut tree) = self.tiles.take() {
                    // Use egui_tiles to render the terminal pane(s)
                    let mut behavior = TerminalBehavior {
                        app: self,
                        has_focus,
                    };
                    tree.ui(&mut behavior, ui);

                    // Put the tree back
                    self.tiles = Some(tree);
                }
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

            // Check if we need to animate the throbber (4 FPS / 250ms)
            // Any session waiting for LLM response shows throbber
            let is_waiting_llm = self
                .sessions
                .values()
                .any(|s| matches!(s.mode, AppMode::WaitingLLM));

            if pty_had_data || cursor_needs_blink || had_user_input {
                // Something changed - repaint immediately
                ctx.request_repaint();
            } else if is_waiting_llm {
                // Animate throbber at 4 FPS (250ms per frame, 1s full cycle)
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
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
