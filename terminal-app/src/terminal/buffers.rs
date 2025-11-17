//! Terminal buffer components following Single Responsibility Principle
//!
//! This module provides three specialized buffer types that compose together
//! to form the terminal state:
//!
//! - [`OutputBuffer`]: Manages scrollable output display with automatic memory limits
//! - [`InputBuffer`]: Handles text input with cursor positioning
//! - [`CommandHistory`]: Provides command history navigation
//!
//! Each buffer component has a single responsibility and can be tested
//! independently. They are composed together in [`TerminalState`](super::TerminalState).
//!
//! # Design Philosophy
//!
//! This module follows the **Single Responsibility Principle (SRP)** and
//! **Interface Segregation Principle (ISP)** from SOLID design principles.
//! By separating concerns into focused components, the code becomes:
//!
//! - **More testable**: Each component can be tested in isolation
//! - **More maintainable**: Changes to scrolling don't affect input handling
//! - **More reusable**: Components can be used independently if needed
//! - **Better encapsulated**: Internal state is private with controlled access
//!
//! # Examples
//!
//! ```
//! use infraware_terminal::terminal::state::TerminalState;
//!
//! let mut state = TerminalState::new();
//!
//! // Add output
//! state.add_output("Hello, world!".to_string());
//!
//! // Handle input
//! state.insert_char('l');
//! state.insert_char('s');
//!
//! // Submit and add to history
//! let command = state.submit_input();
//! assert_eq!(command, "ls");
//! ```

/// Maximum number of lines to keep in output buffer before trimming
const MAX_OUTPUT_LINES: usize = 10_000;
/// Number of lines to remove when buffer exceeds MAX_OUTPUT_LINES
/// This prevents frequent trimming by providing headroom
const TRIM_LINES: usize = 1_000;

/// Manages the output display buffer with scrolling support
#[derive(Debug, Clone)]
pub struct OutputBuffer {
    buffer: Vec<String>,
    scroll_position: usize,
}

impl OutputBuffer {
    /// Create a new empty output buffer
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            scroll_position: 0,
        }
    }

    /// Add a single line to the output buffer
    pub fn add_line(&mut self, line: String) {
        self.buffer.push(line);
        self.trim_if_needed();
        self.auto_scroll_to_bottom();
    }

    /// Add multiple lines to the output buffer
    pub fn add_lines(&mut self, lines: Vec<String>) {
        self.buffer.extend(lines);
        self.trim_if_needed();
        self.auto_scroll_to_bottom();
    }

    /// Get a reference to the buffer contents
    pub fn lines(&self) -> &[String] {
        &self.buffer
    }

    /// Get the current scroll position
    pub fn scroll_position(&self) -> usize {
        self.scroll_position
    }

    /// Scroll up by one line
    pub fn scroll_up(&mut self) {
        if self.scroll_position > 0 {
            self.scroll_position -= 1;
        }
    }

    /// Scroll down by one line
    pub fn scroll_down(&mut self) {
        if self.scroll_position < self.buffer.len().saturating_sub(1) {
            self.scroll_position += 1;
        }
    }

    /// Trim buffer if it exceeds maximum size
    fn trim_if_needed(&mut self) {
        if self.buffer.len() > MAX_OUTPUT_LINES {
            let lines_to_remove = self.buffer.len() - MAX_OUTPUT_LINES + TRIM_LINES;
            self.buffer.drain(0..lines_to_remove);
            self.scroll_position = self.scroll_position.saturating_sub(lines_to_remove);
        }
    }

    /// Auto-scroll to the bottom of the buffer
    fn auto_scroll_to_bottom(&mut self) {
        self.scroll_position = self.buffer.len().saturating_sub(1);
    }

    /// Remove the last line from the buffer (used for removing temporary messages)
    pub fn pop(&mut self) -> Option<String> {
        let result = self.buffer.pop();
        // Adjust scroll position if needed
        if self.scroll_position >= self.buffer.len() {
            self.scroll_position = self.buffer.len().saturating_sub(1);
        }
        result
    }

    /// Clear all lines from the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.scroll_position = 0;
    }
}

impl Default for OutputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages user input with cursor positioning
/// Note: cursor_position is in **character** units, not bytes
#[derive(Debug, Clone)]
pub struct InputBuffer {
    buffer: String,
    cursor_position: usize, // Character index, not byte index
}

impl InputBuffer {
    /// Create a new empty input buffer
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_position: 0,
        }
    }

    /// Get the current input text
    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Get the cursor position (in characters, not bytes)
    pub fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Convert character position to byte index in the string
    fn char_to_byte_idx(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map(|(byte_idx, _)| byte_idx)
            .unwrap_or(self.buffer.len())
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        let char_count = self.buffer.chars().count();
        // Defensive: ensure cursor position is within valid range
        debug_assert!(
            self.cursor_position <= char_count,
            "Cursor position {} exceeds character count {}",
            self.cursor_position,
            char_count
        );
        // Clamp cursor position to prevent panic
        self.cursor_position = self.cursor_position.min(char_count);

        // Convert char position to byte index
        let byte_idx = self.char_to_byte_idx(self.cursor_position);
        self.buffer.insert(byte_idx, c);
        self.cursor_position += 1;
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            let byte_idx = self.char_to_byte_idx(self.cursor_position - 1);
            self.buffer.remove(byte_idx);
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
        let char_count = self.buffer.chars().count();
        if self.cursor_position < char_count {
            self.cursor_position += 1;
        }
    }

    /// Clear the input buffer and reset cursor
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_position = 0;
    }

    /// Set the input buffer to a specific text and position cursor at end
    pub fn set_text(&mut self, text: String) {
        self.buffer = text;
        self.cursor_position = self.buffer.len();
    }

    /// Take the current input, leaving the buffer empty
    pub fn take(&mut self) -> String {
        self.cursor_position = 0;
        std::mem::take(&mut self.buffer)
    }
}

impl Default for InputBuffer {
    fn default() -> Self {
        Self::new()
    }
}

/// Manages command history with navigation support
#[derive(Debug, Clone)]
pub struct CommandHistory {
    history: Vec<String>,
    position: Option<usize>,
}

impl CommandHistory {
    /// Create a new empty command history
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            position: None,
        }
    }

    /// Add a command to the history
    pub fn add(&mut self, command: String) {
        if !command.trim().is_empty() {
            self.history.push(command);
        }
        self.position = None;
    }

    /// Navigate to the previous command in history
    /// Returns the command if available
    pub fn previous(&mut self) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }

        let new_position = match self.position {
            None => Some(self.history.len() - 1),
            Some(pos) if pos > 0 => Some(pos - 1),
            Some(pos) => Some(pos),
        };

        self.position = new_position;
        new_position.map(|pos| self.history[pos].clone())
    }

    /// Navigate to the next command in history
    /// Returns the command if available, or None if at the end
    pub fn next(&mut self) -> Option<String> {
        if self.history.is_empty() {
            return None;
        }

        match self.position {
            None => None,
            Some(pos) if pos < self.history.len() - 1 => {
                let new_pos = pos + 1;
                self.position = Some(new_pos);
                Some(self.history[new_pos].clone())
            }
            Some(_) => {
                self.position = None;
                None
            }
        }
    }

    /// Reset the history navigation position
    pub fn reset_position(&mut self) {
        self.position = None;
    }

    /// Get a reference to all commands in history
    pub fn all(&self) -> &[String] {
        &self.history
    }
}

impl Default for CommandHistory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_buffer_add_line() {
        let mut buffer = OutputBuffer::new();
        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());

        assert_eq!(buffer.lines().len(), 2);
        assert_eq!(buffer.lines()[0], "line 1");
        assert_eq!(buffer.lines()[1], "line 2");
    }

    #[test]
    fn test_output_buffer_scroll() {
        let mut buffer = OutputBuffer::new();
        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // Auto-scrolls to bottom
        assert_eq!(buffer.scroll_position(), 2);

        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 1);

        buffer.scroll_down();
        assert_eq!(buffer.scroll_position(), 2);
    }

    #[test]
    fn test_input_buffer_insert_char() {
        let mut buffer = InputBuffer::new();
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.insert_char('c');

        assert_eq!(buffer.text(), "abc");
        assert_eq!(buffer.cursor_position(), 3);
    }

    #[test]
    fn test_input_buffer_delete_char() {
        let mut buffer = InputBuffer::new();
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.delete_char();

        assert_eq!(buffer.text(), "a");
        assert_eq!(buffer.cursor_position(), 1);
    }

    #[test]
    fn test_input_buffer_cursor_movement() {
        let mut buffer = InputBuffer::new();
        buffer.insert_char('a');
        buffer.insert_char('b');
        buffer.insert_char('c');

        buffer.move_cursor_left();
        assert_eq!(buffer.cursor_position(), 2);

        buffer.move_cursor_right();
        assert_eq!(buffer.cursor_position(), 3);
    }

    #[test]
    fn test_command_history_navigation() {
        let mut history = CommandHistory::new();
        history.add("cmd1".to_string());
        history.add("cmd2".to_string());
        history.add("cmd3".to_string());

        assert_eq!(history.previous(), Some("cmd3".to_string()));
        assert_eq!(history.previous(), Some("cmd2".to_string()));
        assert_eq!(history.next(), Some("cmd3".to_string()));
        assert_eq!(history.next(), None);
    }

    #[test]
    fn test_command_history_ignores_empty() {
        let mut history = CommandHistory::new();
        history.add("".to_string());
        history.add("  ".to_string());

        assert_eq!(history.all().len(), 0);
    }

    #[test]
    fn test_input_buffer_unicode() {
        let mut buffer = InputBuffer::new();
        buffer.insert_char('😀'); // 4-byte emoji
        buffer.insert_char('中'); // 3-byte CJK character
        buffer.insert_char('a');

        assert_eq!(buffer.text(), "😀中a");
        assert_eq!(buffer.cursor_position(), 3); // 3 chars, not 8 bytes

        buffer.move_cursor_left();
        buffer.delete_char();
        assert_eq!(buffer.text(), "😀a");
        assert_eq!(buffer.cursor_position(), 1);
    }

    #[test]
    fn test_output_buffer_pop_adjusts_scroll() {
        let mut buffer = OutputBuffer::new();
        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // Scroll up from bottom
        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 1);

        // Pop last line
        let popped = buffer.pop();
        assert_eq!(popped, Some("line 3".to_string()));
        assert_eq!(buffer.lines().len(), 2);

        // Scroll should still be valid
        assert!(buffer.scroll_position() <= buffer.lines().len());
    }

    #[test]
    fn test_output_buffer_pop_empty() {
        let mut buffer = OutputBuffer::new();
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.scroll_position(), 0);
    }
}
