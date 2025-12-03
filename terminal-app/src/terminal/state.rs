/// Terminal state management using separated buffer components
use super::buffers::{CommandHistory, InputBuffer, OutputBuffer};

/// Represents the current mode of the terminal
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalMode {
    Normal,                  // Waiting for input
    ExecutingCommand,        // Running shell command
    WaitingLLM,              // Querying LLM
    PromptingInstall,        // Asking to install missing command (M2/M3)
    AwaitingCommandApproval, // Human-in-the-loop: waiting for user to approve LLM command (y/n)
    AwaitingAnswer, // Human-in-the-loop: waiting for user to answer LLM question (free text)
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
}

impl TerminalState {
    /// Create a new terminal state
    pub const fn new() -> Self {
        Self {
            output: OutputBuffer::new(),
            input: InputBuffer::new(),
            history: CommandHistory::new(),
            mode: TerminalMode::Normal,
            pending_interaction: None,
            visible_lines: 20, // Default, updated during render
        }
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
}

impl Default for TerminalState {
    fn default() -> Self {
        Self::new()
    }
}
