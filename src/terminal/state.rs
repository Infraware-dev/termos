/// Terminal state management
/// Maximum number of lines to keep in output buffer
const MAX_OUTPUT_LINES: usize = 10_000;
/// Number of lines to remove when buffer is full
const TRIM_LINES: usize = 1_000;

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
#[derive(Debug)]
pub struct TerminalState {
    /// Command/LLM output history
    pub output_buffer: Vec<String>,
    /// Current user input
    pub input_buffer: String,
    /// Current terminal mode
    pub mode: TerminalMode,
    /// Cursor position in input buffer
    pub cursor_position: usize,
    /// Command history for up/down arrow navigation
    pub command_history: Vec<String>,
    /// Current position in command history
    pub history_position: Option<usize>,
    /// Scroll position in output buffer
    pub scroll_position: usize,
}

impl TerminalState {
    /// Create a new terminal state
    pub fn new() -> Self {
        Self {
            output_buffer: Vec::new(),
            input_buffer: String::new(),
            mode: TerminalMode::Normal,
            cursor_position: 0,
            command_history: Vec::new(),
            history_position: None,
            scroll_position: 0,
        }
    }

    /// Add a line to the output buffer
    pub fn add_output(&mut self, line: String) {
        self.output_buffer.push(line);

        // Trim buffer if it exceeds maximum size
        if self.output_buffer.len() > MAX_OUTPUT_LINES {
            self.output_buffer.drain(0..TRIM_LINES);
            self.scroll_position = self.scroll_position.saturating_sub(TRIM_LINES);
        }

        // Auto-scroll to bottom
        self.scroll_position = self.output_buffer.len().saturating_sub(1);
    }

    /// Add multiple lines to the output buffer
    pub fn add_output_lines(&mut self, lines: Vec<String>) {
        // Use extend() for better performance - single allocation instead of multiple pushes
        self.output_buffer.extend(lines);

        // Trim buffer if it exceeds maximum size
        if self.output_buffer.len() > MAX_OUTPUT_LINES {
            let lines_to_remove = self.output_buffer.len() - MAX_OUTPUT_LINES + TRIM_LINES;
            self.output_buffer.drain(0..lines_to_remove);
            self.scroll_position = self.scroll_position.saturating_sub(lines_to_remove);
        }

        // Auto-scroll to bottom
        self.scroll_position = self.output_buffer.len().saturating_sub(1);
    }

    /// Clear the input buffer
    pub fn clear_input(&mut self) {
        self.input_buffer.clear();
        self.cursor_position = 0;
    }

    /// Submit the current input and add to history
    pub fn submit_input(&mut self) -> String {
        let input = self.input_buffer.clone();
        if !input.trim().is_empty() {
            self.command_history.push(input.clone());
        }
        self.clear_input();
        self.history_position = None;
        input
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        self.input_buffer.insert(self.cursor_position, c);
        self.cursor_position += 1;
    }

    /// Delete character before cursor
    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            self.input_buffer.remove(self.cursor_position - 1);
            self.cursor_position -= 1;
        }
    }

    /// Move cursor left
    pub fn move_cursor_left(&mut self) {
        if self.cursor_position > 0 {
            self.cursor_position -= 1;
        }
    }

    /// Move cursor right
    pub fn move_cursor_right(&mut self) {
        if self.cursor_position < self.input_buffer.len() {
            self.cursor_position += 1;
        }
    }

    /// Navigate to previous command in history
    pub fn history_previous(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        let new_position = match self.history_position {
            None => Some(self.command_history.len() - 1),
            Some(pos) if pos > 0 => Some(pos - 1),
            Some(pos) => Some(pos),
        };

        if let Some(pos) = new_position {
            self.history_position = Some(pos);
            self.input_buffer = self.command_history[pos].clone();
            self.cursor_position = self.input_buffer.len();
        }
    }

    /// Navigate to next command in history
    pub fn history_next(&mut self) {
        if self.command_history.is_empty() {
            return;
        }

        match self.history_position {
            None => {}
            Some(pos) if pos < self.command_history.len() - 1 => {
                self.history_position = Some(pos + 1);
                self.input_buffer = self.command_history[pos + 1].clone();
                self.cursor_position = self.input_buffer.len();
            }
            Some(_) => {
                self.history_position = None;
                self.clear_input();
            }
        }
    }

    /// Scroll output up
    pub fn scroll_up(&mut self) {
        if self.scroll_position > 0 {
            self.scroll_position -= 1;
        }
    }

    /// Scroll output down
    pub fn scroll_down(&mut self) {
        if self.scroll_position < self.output_buffer.len().saturating_sub(1) {
            self.scroll_position += 1;
        }
    }
}

impl Default for TerminalState {
    fn default() -> Self {
        Self::new()
    }
}
