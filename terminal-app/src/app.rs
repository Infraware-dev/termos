//! Application module containing the main terminal application.
//!
//! This module provides the `InfrawareApp` struct which implements the eframe::App trait
//! for the terminal emulator. The module is organized into focused submodules:
//!
//! - [`state`] - Core application state (sessions, buffers, flags)
//! - [`llm_controller`] - LLM query management and background events
//! - [`session_manager`] - Session lifecycle (create, close, initialize)
//! - [`input_handler`] - Keyboard input processing and classification
//! - [`hitl_handler`] - Human-in-the-loop interaction handling
//! - [`tiles_manager`] - Split view and tab management
//! - [`clipboard`] - Clipboard operations (copy/paste)
//! - [`render`] - Terminal rendering state and helpers
//! - [`behavior`] - egui_tiles Behavior implementation

mod behavior;
mod clipboard;
mod hitl_handler;
mod input_handler;
mod llm_controller;
mod llm_event_handler;
mod render;
mod session_manager;
mod state;
mod terminal_renderer;
mod tiles_manager;

use std::collections::HashMap;
use std::time::Instant;

pub use behavior::TerminalBehavior;
pub use clipboard::ClipboardManager;
use egui::{Pos2, Rect, Sense, Vec2, ViewportCommand};
use egui_tiles::Tree;
pub use hitl_handler::{HitlAction, HitlHandler, HitlSubmission};
pub use input_handler::{InputAction, InputHandler};
pub use llm_controller::LlmController;
pub use llm_event_handler::LlmEventHandler;
pub use render::RenderState;
pub use session_manager::{CloseResult, SessionManager};
use smallvec::SmallVec;
pub use state::AppState;
pub use terminal_renderer::TerminalRenderer;
pub use tiles_manager::TilesManager;
use tokio::runtime::Runtime;

use crate::config::{rendering, timing};
use crate::input::{KeyboardHandler, TextSelection};
use crate::session::{SessionId, TerminalSession};
use crate::state::AppMode;
use crate::ui::scrollbar::ScrollAction;
use crate::ui::{Scrollbar, Theme};

/// Events coming from background tasks (LLM, etc.)
#[derive(Debug)]
#[expect(
    clippy::enum_variant_names,
    reason = "All events are LLM-related, prefix is meaningful"
)]
pub enum AppBackgroundEvent {
    /// LLM produced a chunk of output or completed (non-streaming)
    LlmResult(crate::llm::LLMQueryResult),
    /// LLM produced a streaming text chunk
    LlmChunk(String),
    /// LLM streaming completed successfully
    LlmStreamComplete,
    /// LLM interrupted for command approval (HITL)
    LlmCommandApproval {
        /// The command to execute
        command: String,
        /// Message describing why
        message: String,
    },
    /// LLM interrupted with a question (HITL)
    LlmQuestion {
        /// The question being asked
        question: String,
        /// Optional predefined choices
        options: Option<Vec<String>>,
    },
    /// An error occurred during LLM query
    LlmError(String),
}

/// Cursor blink and timing state.
#[derive(Debug)]
pub struct TimingState {
    /// Startup time for delayed init
    pub startup_time: Instant,
    /// Cursor blink state
    pub cursor_blink_visible: bool,
    /// Last cursor blink toggle time
    pub last_cursor_blink: Instant,
    /// Last keyboard input time (for adaptive PTY throttling)
    pub last_keyboard_time: Instant,
}

impl TimingState {
    /// Creates a new timing state with current time.
    pub fn new() -> Self {
        Self {
            startup_time: Instant::now(),
            cursor_blink_visible: true,
            last_cursor_blink: Instant::now(),
            // Initialize to past so we start in "idle" mode (higher PTY throughput)
            last_keyboard_time: Instant::now() - std::time::Duration::from_secs(1),
        }
    }

    /// Updates cursor blink state based on elapsed time.
    ///
    /// Returns true if cursor visibility changed.
    pub fn update_cursor_blink(&mut self, has_focus: bool) -> bool {
        if has_focus {
            if self.last_cursor_blink.elapsed() > timing::CURSOR_BLINK_INTERVAL {
                self.cursor_blink_visible = !self.cursor_blink_visible;
                self.last_cursor_blink = Instant::now();
                return true;
            }
        } else {
            // When unfocused, keep cursor visible but static
            self.cursor_blink_visible = true;
            self.last_cursor_blink = Instant::now();
        }
        false
    }

    /// Records keyboard activity timestamp for adaptive PTY throttling.
    pub fn record_keyboard_activity(&mut self) {
        self.last_keyboard_time = Instant::now();
    }

    /// Returns the PTY byte limit based on recent keyboard activity.
    ///
    /// Uses lower limit during keyboard activity for Ctrl+C responsiveness,
    /// higher limit when idle for better burst throughput.
    pub fn pty_byte_limit(&self) -> usize {
        if self.last_keyboard_time.elapsed() < std::time::Duration::from_millis(100) {
            rendering::MAX_BYTES_PER_FRAME_ACTIVE
        } else {
            rendering::MAX_BYTES_PER_FRAME_IDLE
        }
    }
}

impl Default for TimingState {
    fn default() -> Self {
        Self::new()
    }
}

/// Main terminal application with VTE-based terminal emulation.
pub struct InfrawareApp {
    /// Core application state (sessions, buffers, flags)
    pub state: AppState,

    /// Theme configuration
    theme: Theme,

    /// Tokio runtime
    runtime: Runtime,

    /// Theme applied flag
    theme_applied: bool,

    /// Track window focus state to detect focus gain
    had_window_focus: bool,

    /// Keyboard input handler (extracted for testability)
    keyboard_handler: KeyboardHandler,

    /// Input handler for processing keyboard actions
    input_handler: InputHandler,

    /// Render buffers and font metrics
    pub render: RenderState,

    /// LLM client and event channels
    pub llm: LlmController,

    /// Cursor blink and timing
    pub timing: TimingState,

    /// Clipboard manager for copy/paste operations
    clipboard: ClipboardManager,

    /// Scrollbar logic and state (shared across sessions)
    pub scrollbar: Scrollbar,

    /// Tile tree for split view layout (pane ID is session ID)
    pub tiles: Option<Tree<SessionId>>,

    /// Mapping from SessionId to TileId for pane removal
    pub session_tile_ids: HashMap<SessionId, egui_tiles::TileId>,

    /// Session that needs focus in the next frame (after tab creation)
    pending_focus_session: Option<SessionId>,

    /// Tab tile that should be activated in the next frame
    pending_active_tab: Option<egui_tiles::TileId>,

    /// Flag indicating a tab was selected and we need to sync focus
    tab_selection_pending: bool,

    /// Cached logo texture for tab icons
    logo_texture: Option<egui::TextureHandle>,
}

impl std::fmt::Debug for InfrawareApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareApp")
            .field("sessions", &self.state.sessions.len())
            .field("active_session_id", &self.state.active_session_id)
            .finish()
    }
}

impl InfrawareApp {
    /// Creates a new application instance.
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let runtime = Runtime::new().expect("Failed to create tokio runtime");
        let theme = Theme::dark();

        // Load logo texture
        let logo_texture = Self::load_logo_texture(cc);

        // Create initial session
        let initial_session_id: SessionId = 0;
        let initial_session = TerminalSession::new(initial_session_id, runtime.handle());

        let mut sessions = HashMap::new();
        sessions.insert(initial_session_id, initial_session);

        // Initialize LLM controller
        let llm = LlmController::new(&runtime);

        // Create tiles and track the initial pane's tile ID
        let mut tiles = egui_tiles::Tiles::default();
        let initial_tile_id = tiles.insert_pane(initial_session_id);
        let tree = Tree::new("terminal_tiles", initial_tile_id, tiles);

        // Initialize session to TileId mapping
        let mut session_tile_ids = HashMap::new();
        session_tile_ids.insert(initial_session_id, initial_tile_id);

        Self {
            state: AppState::new(sessions, initial_session_id),
            theme,
            runtime,
            theme_applied: false,
            had_window_focus: false,
            keyboard_handler: KeyboardHandler::new(),
            input_handler: InputHandler::new(),
            render: RenderState::new(),
            llm,
            timing: TimingState::new(),
            clipboard: ClipboardManager::new(),
            scrollbar: Scrollbar::new(),
            tiles: Some(tree),
            session_tile_ids,
            pending_focus_session: None,
            pending_active_tab: None,
            tab_selection_pending: false,
            logo_texture,
        }
    }

    /// Loads the logo texture from embedded resources.
    fn load_logo_texture(cc: &eframe::CreationContext<'_>) -> Option<egui::TextureHandle> {
        let image_data = include_bytes!("../resources/logo-corner.png");
        match image::load_from_memory(image_data) {
            Ok(image) => {
                let size = [image.width() as usize, image.height() as usize];
                let image_buffer = image.to_rgba8();
                let pixels = image_buffer.as_flat_samples();
                let color_image = egui::ColorImage::from_rgba_unmultiplied(size, pixels.as_slice());
                Some(cc.egui_ctx.load_texture(
                    "logo-corner",
                    color_image,
                    egui::TextureOptions::LINEAR,
                ))
            }
            Err(e) => {
                tracing::warn!("Failed to load logo-corner.png: {}", e);
                None
            }
        }
    }

    /// Sends data to the active session's PTY.
    pub fn send_to_pty(&self, data: &[u8]) {
        if let Some(session) = self.state.active_session() {
            session.send_to_pty(data);
        } else {
            tracing::warn!("No active session!");
        }
    }

    /// Sends SIGINT to the active session's foreground process group.
    pub fn send_sigint(&self) {
        if let Some(session) = self.state.active_session() {
            session.send_sigint();
        }
    }

    /// Polls PTY output for all sessions.
    ///
    /// Rate-limited to allow Ctrl+C to work even with heavy output.
    /// Uses adaptive throttling based on keyboard activity.
    /// Returns true if any output was processed.
    fn poll_all_sessions(&mut self) -> bool {
        let byte_limit = self.timing.pty_byte_limit();
        let mut any_output = false;
        let mut sessions_to_close: SmallVec<[SessionId; 2]> = SmallVec::new();
        let mut completed_commands: SmallVec<[(SessionId, String, String); 1]> = SmallVec::new();

        let session_ids: SmallVec<[SessionId; 4]> = self.state.sessions.keys().copied().collect();

        for session_id in session_ids {
            let session = match self.state.sessions.get_mut(&session_id) {
                Some(s) => s,
                None => continue,
            };

            let (had_output, command_completed) = session.poll_pty_output(byte_limit);
            if had_output {
                any_output = true;
            }

            if session.should_close {
                sessions_to_close.push(session_id);
                continue;
            }

            // Handle command completion for HITL flow
            if command_completed && let AppMode::ExecutingCommand { ref command } = session.mode {
                let cmd = command.clone();
                let output = session.output_capture.take_output();
                tracing::info!(
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
            let result = SessionManager::close(
                &mut self.state,
                &mut self.tiles,
                &mut self.session_tile_ids,
                session_id,
            );
            if matches!(result, CloseResult::LastSessionClosed) {
                self.state.should_quit = true;
            }
        }

        // Resume LLM for completed commands
        for (session_id, cmd, output) in completed_commands {
            tracing::info!(
                "Session {}: Sending command output to backend for '{}'",
                session_id,
                cmd
            );
            self.llm
                .resume_with_command_output(&self.runtime, cmd, output);
            if let Some(session) = self.state.sessions.get_mut(&session_id) {
                session.agent_state.start_stream();
            }
        }

        any_output || sessions_closed
    }

    /// Processes LLM background events.
    fn process_llm_events(&mut self) {
        let events = self.llm.poll_events();
        for event in events {
            let mut handler = LlmEventHandler::new(&mut self.state, &mut self.llm);
            handler.handle_event(event);
        }
    }

    /// Handles keyboard input.
    fn handle_keyboard(&mut self, ctx: &egui::Context) {
        // Check if active session is in HITL mode
        let is_hitl = self.state.active_session().is_some_and(|s| {
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
        let keyboard_actions = self.keyboard_handler.process(ctx);

        // Track keyboard activity
        if !keyboard_actions.is_empty() {
            self.timing.record_keyboard_activity();
            if let Some(session) = self.state.active_session() {
                tracing::debug!(
                    "Keyboard actions: {} actions, mode: {:?}",
                    keyboard_actions.len(),
                    session.mode.name()
                );
            }
        }

        // Convert keyboard actions to input actions
        let input_actions = self
            .input_handler
            .process_actions(keyboard_actions, &mut self.state.current_command_buffer);

        // Execute each action
        for action in input_actions {
            self.execute_input_action(action, ctx);
        }
    }

    /// Executes a single input action.
    fn execute_input_action(&mut self, action: InputAction, ctx: &egui::Context) {
        // Clear prompt detection on PTY send
        if matches!(action, InputAction::SendToPty(_))
            && let Some(session) = self.state.active_session_mut()
        {
            session.prompt_detector.clear();
        }

        match action {
            InputAction::SendToPty(bytes) => {
                self.send_to_pty(&bytes);
            }
            InputAction::StartLlmQuery(query) => {
                // Clear the shell's input buffer
                self.send_to_pty(b"\x15");
                // Visual feedback: move to next line
                if let Some(session) = self.state.active_session_mut() {
                    session
                        .vte_parser
                        .advance(&mut session.terminal_handler, b"\r\n");
                }
                self.llm.start_query(&self.runtime, query);
                if let Some(session) = self.state.active_session_mut() {
                    session.mode = AppMode::WaitingLLM;
                    session.agent_state.start_stream();
                }
            }
            InputAction::CancelLlm => {
                tracing::info!("Ctrl+C detected, sending ETX (0x03) to PTY");
                self.send_to_pty(&[0x03]);
                self.llm.cancel();
                if let Some(session) = self.state.active_session_mut() {
                    session.agent_state.end_stream();
                    session.mode = AppMode::Normal;
                    session.output_pause_until =
                        Some(Instant::now() + std::time::Duration::from_millis(200));
                }
            }
            InputAction::Copy => {
                self.clipboard.copy_selection(ctx, &self.state);
            }
            InputAction::Paste => {
                if let Some((text, payload)) = self.clipboard.get_paste_data(&self.state) {
                    // Update command buffer with pasted text for proper classification
                    self.input_handler.update_buffer_with_pasted_text(
                        &text,
                        &mut self.state.current_command_buffer,
                    );
                    self.send_to_pty(&payload);
                }
            }
            InputAction::SplitHorizontal => {
                let new_id = SessionManager::create(&mut self.state, self.runtime.handle());
                TilesManager::split(
                    &mut self.tiles,
                    &mut self.session_tile_ids,
                    self.state.active_session_id,
                    new_id,
                    egui_tiles::LinearDir::Horizontal,
                );
            }
            InputAction::SplitVertical => {
                let new_id = SessionManager::create(&mut self.state, self.runtime.handle());
                TilesManager::split(
                    &mut self.tiles,
                    &mut self.session_tile_ids,
                    self.state.active_session_id,
                    new_id,
                    egui_tiles::LinearDir::Vertical,
                );
            }
            InputAction::NewTab => {
                let new_id = SessionManager::create(&mut self.state, self.runtime.handle());
                if let Some(tile_id) =
                    TilesManager::create_tab(&mut self.tiles, &mut self.session_tile_ids, new_id)
                {
                    self.state.active_session_id = new_id;
                    self.pending_focus_session = Some(new_id);
                    self.pending_active_tab = Some(tile_id);
                }
            }
            InputAction::CloseTab => {
                let session_to_close = self.state.active_session_id;
                let result = SessionManager::close(
                    &mut self.state,
                    &mut self.tiles,
                    &mut self.session_tile_ids,
                    session_to_close,
                );
                if let CloseResult::Closed {
                    next_active: Some(next_id),
                } = result
                {
                    self.pending_focus_session = Some(next_id);
                } else if matches!(result, CloseResult::LastSessionClosed) {
                    self.state.should_quit = true;
                }
            }
            InputAction::NextTab => {
                if let Some(session_id) = TilesManager::next_tab(&mut self.tiles) {
                    self.state.active_session_id = session_id;
                    self.pending_focus_session = Some(session_id);
                }
            }
            InputAction::PrevTab => {
                if let Some(session_id) = TilesManager::prev_tab(&mut self.tiles) {
                    self.state.active_session_id = session_id;
                    self.pending_focus_session = Some(session_id);
                }
            }
            InputAction::EnterLlmMode => {
                self.send_to_pty(b"\x15");
                self.state.current_command_buffer.clear();
                self.state.current_command_buffer.push_str("? ");
                self.send_to_pty(b"? ");
                tracing::info!("Entered LLM mode via Ctrl+?");
            }
        }
    }

    /// Handles keyboard input in HITL mode.
    fn handle_hitl_keyboard(&mut self, ctx: &egui::Context) {
        let keyboard_actions = self.keyboard_handler.process(ctx);

        for kb_action in keyboard_actions {
            // Clear prompt detection on user input
            if matches!(kb_action, crate::input::KeyboardAction::SendBytes(_))
                && let Some(session) = self.state.active_session_mut()
            {
                session.prompt_detector.clear();
            }

            let mode = self
                .state
                .active_session()
                .map(|s| s.mode.clone())
                .unwrap_or(AppMode::Normal);

            let hitl_action = HitlHandler::process_keyboard_action(
                kb_action,
                &mut self.state.current_input_buffer,
                &mode,
            );

            self.execute_hitl_action(hitl_action);
        }
    }

    /// Executes a HITL action.
    fn execute_hitl_action(&mut self, action: HitlAction) {
        match action {
            HitlAction::Echo(bytes) => {
                if let Some(session) = self.state.active_session_mut() {
                    session
                        .vte_parser
                        .advance(&mut session.terminal_handler, &bytes);
                }
            }
            HitlAction::Submit(submission) => {
                self.handle_hitl_submission(submission);
            }
            HitlAction::Cancel => {
                if let Some(session) = self.state.active_session_mut() {
                    session.vte_parser.advance(
                        &mut session.terminal_handler,
                        b"\r\n\x1b[33m(cancelled)\x1b[0m\r\n",
                    );
                    session.mode = AppMode::Normal;
                }
            }
            HitlAction::Backspace => {
                if let Some(session) = self.state.active_session_mut() {
                    session
                        .vte_parser
                        .advance(&mut session.terminal_handler, b"\x08 \x08");
                }
            }
            HitlAction::None => {}
        }
    }

    /// Handles a HITL submission (approval or answer).
    fn handle_hitl_submission(&mut self, submission: HitlSubmission) {
        match submission {
            HitlSubmission::Approval { command, approved } => {
                if approved {
                    self.execute_approved_command(command);
                } else {
                    self.reject_command(command);
                }
            }
            HitlSubmission::Answer { answer } => {
                tracing::info!("User answered question: {}", answer);
                self.llm.resume_with_answer(&self.runtime, answer);
                if let Some(session) = self.state.active_session_mut() {
                    session.mode = AppMode::WaitingLLM;
                    session.agent_state.start_stream();
                }
            }
        }
    }

    /// Executes an approved command.
    fn execute_approved_command(&mut self, command: String) {
        use crate::input::validate_command;

        let validation = validate_command(&command);

        if validation.is_blocked() {
            tracing::warn!("Blocked dangerous command: {}", command);
            if let crate::input::ValidationResult::Blocked { reason } = &validation {
                let warning = format!(
                    "\r\n\x1b[91mBLOCKED: {}\x1b[0m\r\n\x1b[33mCommand not executed for security reasons.\x1b[0m\r\n",
                    reason
                );
                if let Some(session) = self.state.active_session_mut() {
                    session
                        .vte_parser
                        .advance(&mut session.terminal_handler, warning.as_bytes());
                    session.mode = AppMode::Normal;
                    session.send_to_pty(b"\x15\n");
                }
            }
            return;
        }

        if let crate::input::ValidationResult::Warning { reason } = &validation {
            tracing::info!("Warning for command {}: {}", command, reason);
            let warning = format!("\x1b[33mWarning: {}\x1b[0m\r\n", reason);
            if let Some(session) = self.state.active_session_mut() {
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, warning.as_bytes());
            }
        }

        tracing::info!("User approved command: {}", command);
        let echo = format!("Approved: {}\r\n", command);

        if let Some(session) = self.state.active_session_mut() {
            session
                .vte_parser
                .advance(&mut session.terminal_handler, echo.as_bytes());

            session.output_capture.start(&command);

            let cmd_with_newline = format!("{}\n", command);
            session.send_to_pty(cmd_with_newline.as_bytes());

            session.mode = AppMode::ExecutingCommand {
                command: command.clone(),
            };
            tracing::debug!("Entered ExecutingCommand mode for: {}", command);
        }
    }

    /// Rejects a command.
    fn reject_command(&mut self, command: String) {
        tracing::info!("User rejected command: {}", command);
        if let Some(session) = self.state.active_session_mut() {
            session.vte_parser.advance(
                &mut session.terminal_handler,
                b"\r\n\x1b[33mCommand rejected.\x1b[0m\r\n",
            );
        }
        self.llm.resume_rejected(&self.runtime);
        if let Some(session) = self.state.active_session_mut() {
            session.mode = AppMode::WaitingLLM;
            session.agent_state.start_stream();
        }
    }

    /// Resizes a session's PTY to match pane size.
    ///
    /// Returns `true` if the pane was resized.
    pub fn resize_session_pty(&mut self, session_id: SessionId, cols: u16, rows: u16) -> bool {
        if let Some(session) = self.state.sessions.get_mut(&session_id) {
            session.resize_pty(cols, rows, self.runtime.handle())
        } else {
            false
        }
    }

    /// Renders terminal grid for a specific session.
    ///
    /// This method handles both input events (selection, scrolling) and
    /// delegates pure rendering to `TerminalRenderer`.
    pub fn render_terminal(&mut self, ui: &mut egui::Ui, session_id: SessionId, has_focus: bool) {
        let session = match self.state.sessions.get(&session_id) {
            Some(s) => s,
            None => return,
        };

        let terminal_id = session.terminal_egui_id;
        let grid = session.terminal_handler.grid();
        let scroll_offset = grid.scroll_offset();
        let max_scroll = grid.max_scroll_offset();
        let visible_row_count = grid.visible_row_count();

        let available = ui.available_size();
        let size = Vec2::new(available.x, available.y);

        let expected_cols = (available.x / self.render.char_width).floor() as u16;
        let (_, current_cols) = session.terminal_handler.grid().size();
        if current_cols < expected_cols.saturating_sub(1) {
            ui.ctx().request_repaint();
        }

        let (response, painter) = ui.allocate_painter(size, Sense::click().union(Sense::drag()));
        let rect = response.rect;
        let scrollbar_area = self.scrollbar.area(rect);

        // Handle click for focus
        if response.clicked()
            && let Some(pos) = response.interact_pointer_pos()
            && !scrollbar_area.contains(pos)
        {
            ui.memory_mut(|mem| mem.request_focus(terminal_id));
            self.state.active_session_id = session_id;
            if let Some(session) = self.state.sessions.get_mut(&session_id) {
                session.selection = None;
            }
        }

        // Handle selection
        self.handle_selection(&response, rect, scrollbar_area, session_id);

        // Handle scroll
        self.handle_scroll(&response, session_id);

        // Handle scrollbar
        if let Some(action) = self.scrollbar.show(
            ui,
            &painter,
            rect,
            scroll_offset,
            max_scroll,
            visible_row_count,
        ) && let Some(session) = self.state.sessions.get_mut(&session_id)
        {
            match action {
                ScrollAction::Up(n) => session.terminal_handler.grid_mut().scroll_view_up(n),
                ScrollAction::Down(n) => session.terminal_handler.grid_mut().scroll_view_down(n),
                ScrollAction::To(offset) => {
                    session.terminal_handler.grid_mut().scroll_to_offset(offset)
                }
            }
        }

        // Delegate pure rendering to TerminalRenderer
        let mut renderer =
            TerminalRenderer::new(&self.state, &mut self.render, &self.theme, &self.timing);
        renderer.draw(&painter, rect, session_id, has_focus);
    }

    /// Handles mouse selection.
    fn handle_selection(
        &mut self,
        response: &egui::Response,
        rect: Rect,
        scrollbar_area: Rect,
        session_id: SessionId,
    ) {
        if response.drag_started()
            && let Some(pos) = response.interact_pointer_pos()
            && !scrollbar_area.contains(pos)
            && let Some((row, col)) = self
                .state
                .sessions
                .get(&session_id)
                .map(|s| self.screen_to_grid(pos, rect, s))
            && let Some(session) = self.state.sessions.get_mut(&session_id)
        {
            session.selection = Some(TextSelection::new(row, col));
        }

        if response.dragged()
            && let Some(pos) = response.interact_pointer_pos()
            && !self.scrollbar.is_dragging()
            && let Some((row, col)) = self
                .state
                .sessions
                .get(&session_id)
                .map(|s| self.screen_to_grid(pos, rect, s))
            && let Some(session) = self.state.sessions.get_mut(&session_id)
            && let Some(ref mut sel) = session.selection
        {
            sel.update_end(row, col);
        }

        if response.drag_stopped()
            && let Some(session) = self.state.sessions.get_mut(&session_id)
            && let Some(ref mut sel) = session.selection
        {
            sel.active = false;
        }
    }

    /// Handles mouse wheel scrolling.
    fn handle_scroll(&mut self, response: &egui::Response, session_id: SessionId) {
        let scroll_delta = response.ctx.input(|i| i.smooth_scroll_delta.y);
        if response.hovered()
            && scroll_delta != 0.0
            && let Some(session) = self.state.sessions.get_mut(&session_id)
        {
            let lines = (scroll_delta / self.render.char_height).round() as i32;
            let grid = session.terminal_handler.grid_mut();
            if lines > 0 {
                grid.scroll_view_up(lines as usize);
            } else if lines < 0 {
                grid.scroll_view_down((-lines) as usize);
            }
        }
    }

    /// Converts screen coordinates to grid position.
    fn screen_to_grid(&self, pos: Pos2, rect: Rect, session: &TerminalSession) -> (usize, usize) {
        let col = ((pos.x - rect.left()) / self.render.char_width).max(0.0) as usize;
        let row = ((pos.y - rect.top()) / self.render.char_height).max(0.0) as usize;

        let max_col = session.terminal_size.0.saturating_sub(1) as usize;
        let max_row = session.terminal_size.1.saturating_sub(1) as usize;

        (row.min(max_row), col.min(max_col))
    }

    /// Returns a reference to the sessions HashMap.
    pub fn sessions(&self) -> &HashMap<SessionId, TerminalSession> {
        &self.state.sessions
    }

    /// Returns the active session ID.
    pub fn active_session_id(&self) -> SessionId {
        self.state.active_session_id
    }

    /// Sets the active session ID.
    pub fn set_active_session_id(&mut self, id: SessionId) {
        self.state.active_session_id = id;
    }

    /// Returns a reference to the logo texture.
    pub fn logo_texture(&self) -> Option<&egui::TextureHandle> {
        self.logo_texture.as_ref()
    }

    /// Sets the tab selection pending flag.
    pub fn set_tab_selection_pending(&mut self, pending: bool) {
        self.tab_selection_pending = pending;
    }
}

impl eframe::App for InfrawareApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Apply theme once on startup
        if !self.theme_applied {
            self.theme.apply(ctx);
            self.theme_applied = true;
        }

        // Handle window focus
        let has_focus = ctx.input(|i| i.focused);
        let terminal_id = egui::Id::new("terminal_main_area");

        if has_focus && !self.had_window_focus {
            ctx.memory_mut(|mem| mem.request_focus(terminal_id));
        }

        if has_focus && !ctx.memory(|mem| mem.has_focus(terminal_id)) {
            ctx.memory_mut(|mem| mem.request_focus(terminal_id));
        }

        self.had_window_focus = has_focus;

        // Handle pending focus request
        if let Some(session_id) = self.pending_focus_session.take()
            && let Some(session) = self.state.sessions.get(&session_id)
        {
            ctx.memory_mut(|mem| mem.request_focus(session.terminal_egui_id));
            ctx.request_repaint();
            tracing::debug!("Focused new tab session {}", session_id);
        }

        // Check for quit
        if self.state.should_quit {
            ctx.send_viewport_cmd(ViewportCommand::Close);
            return;
        }

        let is_minimized = ctx.input(|i| i.viewport().minimized.unwrap_or(false));

        // Update cursor blink
        if !is_minimized {
            self.timing.update_cursor_blink(has_focus);
        }

        // Check for SIGINT from system signal handler
        if crate::SIGINT_RECEIVED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            tracing::info!("System SIGINT received, sending to process group");
            self.send_sigint();
            if let Some(session) = self.state.active_session_mut() {
                session.output_pause_until =
                    Some(Instant::now() + std::time::Duration::from_millis(200));
            }
        }

        // Poll background events
        self.process_llm_events();

        // Check for pending LLM query from shell hook
        let pending_llm_query = self
            .state
            .active_session_mut()
            .and_then(|s| s.terminal_handler.take_pending_llm_query());
        if let Some(failed_cmd) = pending_llm_query {
            tracing::info!("Triggering LLM for failed command: {}", failed_cmd);
            let query = format!(
                "I tried to run '{}' but got 'command not found'. What should I do?",
                failed_cmd
            );
            self.llm.start_query(&self.runtime, query);
            if let Some(session) = self.state.active_session_mut() {
                session.mode = AppMode::WaitingLLM;
                session.agent_state.start_stream();
            }
        }

        // Handle keyboard FIRST
        self.handle_keyboard(ctx);

        // Poll PTY output
        let pty_had_data = self.poll_all_sessions();

        // Initialize shells
        let session_ids: SmallVec<[SessionId; 4]> = self.state.sessions.keys().copied().collect();
        for session_id in session_ids {
            SessionManager::initialize_shell(&mut self.state, session_id);
        }

        // Activate pending tab
        if let Some(tile_id) = self.pending_active_tab.take()
            && let Some(ref mut tree) = self.tiles
            && let Some(root_id) = tree.root()
            && let Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tabs))) =
                tree.tiles.get_mut(root_id)
        {
            tabs.active = Some(tile_id);
            tracing::debug!("Activated pending tab {:?}", tile_id);
        }

        // Render UI
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(self.theme.split_separator))
            .show(ctx, |ui| {
                if let Some(mut tree) = self.tiles.take() {
                    let mut behavior = TerminalBehavior::new(self, has_focus);
                    tree.ui(&mut behavior, ui);
                    self.tiles = Some(tree);
                }
            });

        // Handle pending tab selection
        if self.tab_selection_pending {
            self.tab_selection_pending = false;

            if let Some(ref tree) = self.tiles
                && let Some(root_id) = tree.root()
                && let Some(egui_tiles::Tile::Container(egui_tiles::Container::Tabs(tabs))) =
                    tree.tiles.get(root_id)
                && let Some(active_tile_id) = tabs.active
                && let Some(session_id) =
                    TilesManager::find_first_pane_session(&tree.tiles, active_tile_id)
                && self.state.active_session_id != session_id
            {
                self.state.active_session_id = session_id;
                self.pending_focus_session = Some(session_id);
                tracing::info!(
                    "Tab selection detected: switched to session {session_id}, pending focus"
                );
            }
        }

        // Reactive repaint scheduling
        if is_minimized {
            ctx.request_repaint_after(timing::BACKGROUND_REPAINT * 2);
        } else if has_focus {
            let had_user_input = ctx.input(|i| {
                !i.events.is_empty() || i.pointer.any_down() || i.pointer.any_released()
            });

            let blink_interval = timing::CURSOR_BLINK_INTERVAL;
            let time_since_blink = self.timing.last_cursor_blink.elapsed();
            let cursor_needs_blink = time_since_blink >= blink_interval;

            let is_waiting_llm = self
                .state
                .sessions
                .values()
                .any(|s| matches!(s.mode, AppMode::WaitingLLM));

            if pty_had_data || cursor_needs_blink || had_user_input {
                ctx.request_repaint();
            } else if is_waiting_llm {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            } else {
                let time_to_next_blink = blink_interval.saturating_sub(time_since_blink);
                ctx.request_repaint_after(time_to_next_blink);
            }
        } else {
            ctx.request_repaint_after(timing::BACKGROUND_REPAINT);
        }
    }
}
