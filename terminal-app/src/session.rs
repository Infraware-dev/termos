//! Terminal session management for split view support.
//!
//! Each `TerminalSession` represents an independent terminal pane with its own:
//! - PTY (shell process)
//! - VTE parser and terminal handler
//! - State (mode, throbber, etc.)
//!
//! Multiple sessions can run concurrently in split view.

use crate::config::{pty as pty_config, rendering, size};
use crate::input::{OutputCapture, PromptDetector, TextSelection};
use crate::pty::{PtyManager, PtyReader, PtyWrite, PtyWriter};
use crate::state::{AgentState, AppMode};
use crate::terminal::TerminalHandler;
use egui::Id as EguiId;
use std::sync::Arc;
use std::sync::mpsc;
use std::time::Instant;
use tokio::runtime::Handle;
use tokio::sync::Mutex as TokioMutex;

/// Unique identifier for a terminal session (used as pane ID in egui_tiles).
pub type SessionId = usize;

/// A terminal session representing an independent terminal pane.
///
/// Each session has its own PTY, VTE parser, and state.
pub struct TerminalSession {
    /// Unique session identifier (also used as pane ID in egui_tiles)
    pub id: SessionId,

    /// Current application mode for this session
    pub mode: AppMode,

    /// VTE parser for escape sequences
    pub vte_parser: vte::Parser,

    /// Terminal handler with grid state
    pub terminal_handler: TerminalHandler,

    /// PTY writer for sending input
    pub pty_writer: Option<Arc<PtyWriter>>,

    /// PTY output receiver channel
    pub pty_output_rx: Option<mpsc::Receiver<Vec<u8>>>,

    /// PTY reader (must be kept alive to keep reader thread running)
    #[expect(dead_code, reason = "Held to keep reader thread alive via Drop")]
    pub pty_reader: Option<PtyReader>,

    /// PTY manager for resize and SIGINT
    pub pty_manager: Option<Arc<TokioMutex<PtyManager>>>,

    /// Current terminal size (cols, rows)
    pub terminal_size: (u16, u16),

    /// Shell initialization done
    pub shell_initialized: bool,

    /// Startup time for delayed init
    pub startup_time: Instant,

    /// When set, pause output reading to let kernel process Ctrl+C
    pub output_pause_until: Option<Instant>,

    /// Agent state for tracking LLM stream activity
    pub agent_state: AgentState,

    /// Interactive prompt detector for PTY output
    pub prompt_detector: PromptDetector,

    /// Timestamp of last PTY activity (for throbber suppression)
    pub last_pty_activity: Instant,

    /// Output capture for commands executed in PTY
    pub output_capture: OutputCapture,

    /// Flag indicating this session should be closed (shell exited)
    pub should_close: bool,

    /// Flag indicating PTY had output this frame (for smart repaint)
    pub needs_repaint: bool,

    /// Pre-calculated X coordinates for each column (avoids per-cell multiplication)
    pub column_x_coords: Vec<f32>,

    /// Cached egui Id for this terminal pane (avoids format! every frame)
    pub terminal_egui_id: EguiId,

    /// Cached tab title (avoids format! allocation every frame)
    pub cached_title: String,

    /// Current text selection for this session (None if no selection active)
    pub selection: Option<TextSelection>,
}

impl std::fmt::Debug for TerminalSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TerminalSession")
            .field("id", &self.id)
            .field("mode", &self.mode)
            .field("terminal_size", &self.terminal_size)
            .field("shell_initialized", &self.shell_initialized)
            .field("should_close", &self.should_close)
            .finish_non_exhaustive()
    }
}

impl TerminalSession {
    /// Create a new terminal session with PTY.
    ///
    /// # Arguments
    /// * `id` - Unique session identifier
    /// * `runtime_handle` - Handle to tokio runtime for async PTY initialization
    pub fn new(id: SessionId, runtime_handle: &Handle) -> Self {
        let (rows, cols) = (size::DEFAULT_ROWS, size::DEFAULT_COLS);

        // Initialize PTY
        let (pty_writer, pty_output_rx, pty_reader, pty_manager) = runtime_handle.block_on(async {
            match PtyManager::new().await {
                Ok(mut manager) => {
                    log::info!(
                        "Session {}: PTY initialized with shell: {}",
                        id,
                        manager.shell()
                    );

                    let (tx, rx) = mpsc::sync_channel(pty_config::CHANNEL_CAPACITY);
                    let writer = manager.take_writer().await.ok();
                    let reader = manager.take_reader(tx).await.ok();
                    let manager = Arc::new(TokioMutex::new(manager));

                    (writer, Some(rx), reader, Some(manager))
                }
                Err(e) => {
                    log::error!("Session {}: Failed to initialize PTY: {}", id, e);
                    (None, None, None, None)
                }
            }
        });

        Self {
            id,
            mode: AppMode::Normal,
            vte_parser: vte::Parser::new(),
            terminal_handler: TerminalHandler::new(rows, cols),
            pty_writer,
            pty_output_rx,
            pty_reader,
            pty_manager,
            terminal_size: (cols, rows),
            shell_initialized: false,
            startup_time: Instant::now(),
            output_pause_until: None,
            agent_state: AgentState::new(),
            prompt_detector: PromptDetector::new(),
            last_pty_activity: Instant::now() - std::time::Duration::from_secs(10),
            output_capture: OutputCapture::new(),
            should_close: false,
            needs_repaint: true, // Initial repaint needed
            column_x_coords: (0..cols)
                .map(|c| c as f32 * rendering::CHAR_WIDTH)
                .collect(),
            // Cached egui Id (avoids format! allocation every frame)
            terminal_egui_id: EguiId::new(("terminal_pane", id)),
            // Cached tab title (avoids format! allocation every frame)
            cached_title: format!("Terminal {}", id),
            selection: None,
        }
    }

    /// Send data to PTY synchronously.
    pub fn send_to_pty(&self, data: &[u8]) {
        if let Some(ref writer) = self.pty_writer {
            log::debug!(
                "Session {}: Writing {} bytes to PTY: {:?}",
                self.id,
                data.len(),
                data
            );
            match writer.write_bytes(data) {
                Ok(n) => log::debug!("Session {}: Wrote {} bytes to PTY", self.id, n),
                Err(e) => log::error!("Session {}: Failed to write to PTY: {}", self.id, e),
            }
        } else {
            log::warn!("Session {}: No PTY writer available!", self.id);
        }
    }

    /// Send SIGINT to the foreground process group.
    pub fn send_sigint(&self) {
        if let Some(ref manager) = self.pty_manager {
            if let Ok(mgr) = manager.try_lock() {
                if let Err(e) = mgr.send_sigint() {
                    log::error!("Session {}: Failed to send SIGINT: {}", self.id, e);
                }
            } else {
                log::warn!("Session {}: Could not lock PTY manager for SIGINT", self.id);
            }
        }
    }

    /// Resize terminal grid and PTY to match pane size.
    ///
    /// # Sync/Async Boundary
    /// - Grid resize is immediate (sync)
    /// - PTY resize is spawned as async task (best-effort, may fail silently)
    ///
    /// # Debouncing
    /// Small changes (≤1 row/col) are debounced. Large changes bypass debounce.
    ///
    /// # Returns
    /// `true` if grid was resized, `false` if debounced or unchanged.
    /// Note: Returns `true` even if PTY is unavailable (grid-only resize).
    pub fn resize_pty(&mut self, cols: u16, rows: u16, runtime_handle: &Handle) -> bool {
        let (grid_rows, grid_cols) = self.terminal_handler.grid().size();

        // Check if size actually changed compared to the internal Grid
        if grid_cols == cols && grid_rows == rows {
            return false;
        }

        // Calculate size delta from the current Grid size (the ultimate truth)
        let delta_rows = (rows as i32 - grid_rows as i32).abs();
        let delta_cols = (cols as i32 - grid_cols as i32).abs();

        // No debounce - resize immediately to match available space (follows egui pattern).
        // Content should always match the allocated space to avoid rendering artifacts.
        self.terminal_size = (cols, rows);

        log::debug!(
            "Session {}: Resizing Grid from {}x{} to {}x{} (delta: {}x{})",
            self.id,
            grid_cols,
            grid_rows,
            cols,
            rows,
            delta_cols,
            delta_rows
        );

        // Update column X coordinates - resize in place to reuse allocation
        let new_len = cols as usize;
        self.column_x_coords.resize(new_len, 0.0);
        for (i, x) in self.column_x_coords.iter_mut().enumerate() {
            *x = i as f32 * rendering::CHAR_WIDTH;
        }

        // Resize terminal handler (immediate, sync)
        self.terminal_handler.resize(rows, cols);

        // Resize PTY (async) - spawned task, may complete after this returns
        if let Some(ref manager) = self.pty_manager {
            let manager = manager.clone();
            runtime_handle.spawn(async move {
                let mut mgr = manager.lock().await;
                if let Err(e) = mgr.resize(rows, cols).await {
                    log::error!("Failed to resize PTY: {}", e);
                }
            });
        } else {
            // Grid was resized but PTY is unavailable (likely failed to initialize)
            log::warn!("Session {}: PTY unavailable, only grid resized", self.id);
        }

        true // Grid resize always succeeds; PTY resize is best-effort async
    }

    /// Poll PTY output and feed to VTE parser.
    ///
    /// # Arguments
    /// * `byte_limit` - Maximum bytes to process this frame (for adaptive throttling)
    ///
    /// # Returns
    /// `(had_output, command_completed)` tuple:
    /// - `had_output`: true if any PTY output was processed
    /// - `command_completed`: true if a command in ExecutingCommand mode finished (prompt detected)
    pub fn poll_pty_output(&mut self, byte_limit: usize) -> (bool, bool) {
        // If paused (after Ctrl+C), skip reading
        if let Some(until) = self.output_pause_until {
            if Instant::now() < until {
                return (false, false);
            }
            self.output_pause_until = None;
        }

        let mut bytes_processed = 0;
        let mut command_completed = false;
        let is_executing = matches!(self.mode, AppMode::ExecutingCommand { .. });

        if let Some(ref rx) = self.pty_output_rx {
            loop {
                if bytes_processed >= byte_limit {
                    break;
                }

                match rx.try_recv() {
                    Ok(bytes) => {
                        bytes_processed += bytes.len();
                        self.prompt_detector.process_output(&bytes);
                        self.last_pty_activity = Instant::now();
                        self.vte_parser.advance(&mut self.terminal_handler, &bytes);
                        self.terminal_handler.grid_mut().scroll_to_bottom();
                        self.needs_repaint = true;

                        // Feed output to capture when executing a command (HITL flow)
                        // OutputCapture.append() returns true when prompt is detected (command done)
                        if is_executing && self.output_capture.is_capturing() {
                            let text = String::from_utf8_lossy(&bytes);
                            if self.output_capture.append(&text) {
                                command_completed = true;
                                log::debug!(
                                    "Session {}: Command completion detected via prompt",
                                    self.id
                                );
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        log::info!("Session {}: Shell exited", self.id);
                        self.should_close = true;
                        break;
                    }
                }
            }
        }

        (bytes_processed > 0, command_completed)
    }

    /// Check if session is in a state that shows the throbber.
    pub fn should_show_throbber(&self) -> bool {
        let throbber_suppressed = self.prompt_detector.is_prompt_active()
            || self.last_pty_activity.elapsed() < std::time::Duration::from_millis(500);
        matches!(self.mode, AppMode::WaitingLLM) && !throbber_suppressed
    }
}
