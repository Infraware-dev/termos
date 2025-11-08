/// Terminal state management using separated buffer components
use super::buffers::{CommandHistory, InputBuffer, OutputBuffer};

/// Represents the current mode of the terminal
#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum TerminalMode {
    Normal,           // Waiting for input
    ExecutingCommand, // Running shell command
    WaitingLLM,       // Querying LLM
    PromptingInstall, // Asking to install missing command (M2/M3)
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
}

impl TerminalState {
    /// Create a new terminal state
    pub fn new() -> Self {
        Self {
            output: OutputBuffer::new(),
            input: InputBuffer::new(),
            history: CommandHistory::new(),
            mode: TerminalMode::Normal,
        }
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
}

impl Default for TerminalState {
    fn default() -> Self {
        Self::new()
    }
}
