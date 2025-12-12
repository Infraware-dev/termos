/// Terminal state management using separated buffer components
use super::buffers::{CommandHistory, InputBuffer, OutputBuffer};
use crate::input::IncompleteReason;
use std::time::Instant;

/// Represents the current mode of the terminal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalMode {
    Normal,                              // Waiting for input
    ExecutingCommand,                    // Running shell command
    WaitingLLM,                          // Querying LLM
    PromptingInstall,                    // Asking to install missing command (M2/M3)
    AwaitingCommandApproval, // Human-in-the-loop: waiting for user to approve LLM command (y/n)
    AwaitingAnswer, // Human-in-the-loop: waiting for user to answer LLM question (free text)
    AwaitingMoreInput(IncompleteReason), // Multiline: waiting for more input lines
}

/// Type of shell confirmation being requested
/// Distinguishes shell-originated confirmations from LLM-originated ones
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmationType {
    /// rm on write-protected files - when approved, execute with rm -f
    RmWriteProtected,
    /// rm -i: prompt before every removal
    RmInteractive {
        /// List of files to confirm individually
        files: Vec<String>,
        /// Current file index being confirmed
        current_index: usize,
        /// Original command string
        command: String,
    },
    /// rm -I: prompt once if >3 files or recursive
    RmInteractiveBulk {
        /// Number of arguments/files
        file_count: usize,
        /// Whether -r/-R flag is present
        is_recursive: bool,
    },
    /// cp -i: prompt before overwrite
    CpInteractive {
        /// Destination file that would be overwritten
        destination: String,
    },
    /// mv -i: prompt before overwrite
    MvInteractive {
        /// Destination file that would be overwritten
        destination: String,
    },
    /// ln -i: prompt before removing destination
    LnInteractive {
        /// Destination that would be removed
        destination: String,
    },
}

/// Pending interaction with the LLM for human-in-the-loop flow
#[derive(Debug, Clone)]
pub enum PendingInteraction {
    /// Command waiting for approval (y/n response)
    CommandApproval {
        /// The command that the LLM wants to execute
        command: String,
        /// Description/reason from the LLM
        message: String,
        /// If Some, this is a shell confirmation (not LLM) - determines execution behavior
        confirmation_type: Option<ConfirmationType>,
    },
    /// Question waiting for text answer (free-form response)
    Question {
        /// The question being asked
        question: String,
        /// Optional predefined choices
        options: Option<Vec<String>>,
    },
}

/// Main terminal state structure
/// Refactored to follow Single Responsibility Principle with separated buffers
#[derive(Debug)]
pub struct TerminalState {
    /// Output display buffer with scrolling
    pub output: OutputBuffer,
    /// User input buffer with cursor management
    pub input: InputBuffer,
    /// Command history with navigation
    pub history: CommandHistory,
    /// Current terminal mode
    pub mode: TerminalMode,
    /// Pending interaction for human-in-the-loop (HITL) flow
    pub pending_interaction: Option<PendingInteraction>,
    /// Number of visible lines in output area (updated during render)
    visible_lines: usize,
    /// Accumulated lines for multiline input (backslash continuation, heredoc, etc.)
    pub multiline_buffer: Vec<String>,
    /// Pending heredoc delimiter (if waiting for heredoc content)
    pub pending_heredoc: Option<String>,
    /// Cached prompt string (updated when cwd changes)
    cached_prompt: String,
    /// Animation start time for loading indicators
    animation_start: Option<Instant>,
    /// Whether terminal is in elevated (root) mode via sudo su
    is_root_mode: bool,
}

impl TerminalState {
    /// Create a new terminal state
    pub fn new() -> Self {
        let mut state = Self {
            output: OutputBuffer::new(),
            input: InputBuffer::new(),
            history: CommandHistory::new(),
            mode: TerminalMode::Normal,
            pending_interaction: None,
            visible_lines: 20, // Default, updated during render
            multiline_buffer: Vec::new(),
            pending_heredoc: None,
            cached_prompt: String::new(), // Will be set below
            animation_start: None,
            is_root_mode: false,
        };
        state.cached_prompt = state.build_prompt();
        state
    }

    /// Build the prompt string in Linux shell style
    /// Normal user: user@hostname:~/path$
    /// Root mode or actual root: user@hostname:~/path#
    fn build_prompt(&self) -> String {
        // Get username
        let username = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "user".to_string());

        // Get hostname
        let hostname = hostname::get()
            .map(|h| h.to_string_lossy().into_owned())
            .unwrap_or_else(|_| "localhost".to_string());

        // Get current working directory with ~ substitution for home
        let cwd = std::env::current_dir()
            .map(|p| {
                // Use ~ for home directory (only for non-root users)
                if let Some(home) = dirs::home_dir() {
                    if p == home {
                        return "~".to_string();
                    }
                    if let Ok(suffix) = p.strip_prefix(&home) {
                        return format!("~/{}", suffix.display());
                    }
                }
                p.display().to_string()
            })
            .unwrap_or_else(|_| "~".to_string());

        // Prompt symbol: # for root mode or actual root, $ for normal user
        let symbol = if self.is_root_mode || Self::is_actual_root_user() {
            '#'
        } else {
            '$'
        };

        format!("|~| {}@{}:{}{} ", username, hostname, cwd, symbol)
    }

    /// Check if the actual OS user is root/superuser (uid 0)
    #[cfg(unix)]
    fn is_actual_root_user() -> bool {
        unsafe { libc::getuid() == 0 }
    }

    #[cfg(not(unix))]
    fn is_actual_root_user() -> bool {
        false
    }

    /// Get the current prompt string (cached for performance)
    pub fn get_prompt(&self) -> String {
        self.cached_prompt.clone()
    }

    /// Refresh the cached prompt (call after cd or cwd changes)
    pub fn refresh_prompt(&mut self) {
        self.cached_prompt = self.build_prompt();
    }

    /// Enter root mode (after sudo su, su, etc.)
    pub fn enter_root_mode(&mut self) {
        self.is_root_mode = true;
        self.refresh_prompt();
    }

    /// Exit root mode (back to normal user)
    pub fn exit_root_mode(&mut self) {
        log::info!(
            "exit_root_mode() called, current is_root_mode={}",
            self.is_root_mode
        );
        self.is_root_mode = false;
        self.refresh_prompt();
    }

    /// Check if in root mode
    pub fn is_root_mode(&self) -> bool {
        self.is_root_mode
    }

    /// Start animation timer (for loading indicators)
    pub fn start_animation(&mut self) {
        self.animation_start = Some(Instant::now());
    }

    /// Stop animation timer
    pub fn stop_animation(&mut self) {
        self.animation_start = None;
    }

    /// Get elapsed time since animation started (in milliseconds)
    /// Returns None if animation is not running
    pub fn animation_elapsed(&self) -> Option<u64> {
        self.animation_start
            .map(|start| start.elapsed().as_millis() as u64)
    }

    /// Get window title string (current directory in ~/path format)
    pub fn get_window_title(&self) -> String {
        std::env::current_dir()
            .map(|p| {
                if let Some(home) = dirs::home_dir() {
                    if let Ok(suffix) = p.strip_prefix(&home) {
                        if suffix.as_os_str().is_empty() {
                            return "~".to_string();
                        }
                        return format!("~/{}", suffix.display());
                    }
                }
                p.display().to_string()
            })
            .unwrap_or_else(|_| "~".to_string())
    }

    /// Update the number of visible lines (called during render)
    /// Also propagates to OutputBuffer for scroll calculations
    pub fn set_visible_lines(&mut self, lines: usize) {
        self.visible_lines = lines;
        self.output.set_visible_lines(lines);
    }

    /// Get the number of visible lines
    pub const fn visible_lines(&self) -> usize {
        self.visible_lines
    }

    /// Add a line to the output buffer
    pub fn add_output(&mut self, line: String) {
        self.output.add_line(line);
    }

    /// Add multiple lines to the output buffer
    pub fn add_output_lines(&mut self, lines: Vec<String>) {
        self.output.add_lines(lines);
    }

    /// Clear the input buffer
    pub fn clear_input(&mut self) {
        self.input.clear();
    }

    /// Submit the current input and add to history
    pub fn submit_input(&mut self) -> String {
        let input = self.input.take();
        self.history.add(input.clone());
        self.history.reset_position();
        input
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input.insert_char(c);
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        self.input.delete_char();
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        self.input.move_cursor_left();
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self) {
        self.input.move_cursor_right();
    }

    /// Navigate to previous command in history
    pub fn history_previous(&mut self) {
        if let Some(cmd) = self.history.previous() {
            self.input.set_text(cmd);
        }
    }

    /// Navigate to next command in history
    pub fn history_next(&mut self) {
        match self.history.next() {
            Some(cmd) => self.input.set_text(cmd),
            None => self.input.clear(),
        }
    }

    /// Scroll output up
    pub fn scroll_up(&mut self) {
        self.output.scroll_up();
    }

    /// Scroll output down
    pub fn scroll_down(&mut self) {
        self.output.scroll_down();
    }

    /// Check if terminal is in a Human-in-the-Loop (HITL) waiting state
    ///
    /// Returns true if waiting for user approval (y/n) or answer (free text)
    pub fn is_in_hitl_mode(&self) -> bool {
        matches!(
            self.mode,
            TerminalMode::AwaitingCommandApproval | TerminalMode::AwaitingAnswer
        )
    }

    /// Check if terminal is in multiline input mode
    pub fn is_in_multiline_mode(&self) -> bool {
        matches!(self.mode, TerminalMode::AwaitingMoreInput(_))
    }

    /// Clear multiline state and return to normal mode
    pub fn cancel_multiline(&mut self) {
        self.multiline_buffer.clear();
        self.pending_heredoc = None;
        self.mode = TerminalMode::Normal;
    }

    /// Get the full accumulated multiline input joined together
    pub fn get_multiline_input(&self) -> String {
        crate::input::multiline::join_lines(&self.multiline_buffer)
    }
}

impl Default for TerminalState {
    fn default() -> Self {
        Self::new()
    }
}
