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
//! let command = state.submit_input(true); // true = add to history
//! assert_eq!(command, "ls");
//! ```

use ratatui::text::Line;

/// Maximum number of lines to keep in output buffer before trimming
const MAX_OUTPUT_LINES: usize = 10_000;
/// Number of lines to remove when buffer exceeds MAX_OUTPUT_LINES
/// This prevents frequent trimming by providing headroom
const TRIM_LINES: usize = 1_000;

/// Manages the output display buffer with scrolling support.
///
/// Stores both raw strings (for serialization/debugging) and pre-parsed
/// ratatui Lines (for O(1) rendering without ANSI re-parsing).
#[derive(Debug, Clone)]
pub struct OutputBuffer {
    /// Raw string buffer (kept for backward compatibility and debugging)
    buffer: Vec<String>,
    /// Pre-parsed lines with ANSI codes converted to ratatui styles.
    /// This eliminates O(N²) ANSI parsing on every render.
    parsed_buffer: Vec<Line<'static>>,
    scroll_position: usize,
    /// Number of visible lines in the viewport (for scroll calculations)
    visible_lines: usize,
    /// Extra lines added by rendering (prompt, interaction, etc.)
    /// Updated by tui.rs during render to allow proper scroll calculations
    extra_lines: usize,
}

impl OutputBuffer {
    /// Create a new empty output buffer
    pub fn new() -> Self {
        Self {
            buffer: Vec::new(),
            parsed_buffer: Vec::new(),
            scroll_position: 0,
            visible_lines: 0, // Initialized to 0, set on first render from actual terminal height
            extra_lines: 1,   // At least 1 for prompt line
        }
    }

    /// Parse a raw string with ANSI codes into a ratatui Line.
    /// This is done once when adding, not on every render.
    fn parse_ansi(line: &str) -> Line<'static> {
        use ansi_to_tui::IntoText;
        match line.into_text() {
            Ok(text) => text
                .lines
                .into_iter()
                .next()
                .unwrap_or_else(|| Line::from(line.to_string())),
            Err(_) => Line::from(line.to_string()),
        }
    }

    /// Add a single line to the output buffer.
    /// Parses ANSI codes once and caches the result.
    pub fn add_line(&mut self, line: String) {
        let parsed = Self::parse_ansi(&line);
        self.parsed_buffer.push(parsed);
        self.buffer.push(line);
        self.trim_if_needed();
        self.auto_scroll_to_bottom();
    }

    /// Add multiple lines to the output buffer.
    /// Parses ANSI codes once per line and caches the results.
    pub fn add_lines(&mut self, lines: Vec<String>) {
        for line in &lines {
            self.parsed_buffer.push(Self::parse_ansi(line));
        }
        self.buffer.extend(lines);
        self.trim_if_needed();
        self.auto_scroll_to_bottom();
    }

    /// Get a reference to the raw buffer contents (for backward compatibility)
    pub fn lines(&self) -> &[String] {
        &self.buffer
    }

    /// Get pre-parsed lines ready for rendering (no ANSI parsing needed)
    pub fn parsed_lines(&self) -> &[Line<'static>] {
        &self.parsed_buffer
    }

    /// Get the visible window of parsed lines based on scroll position.
    ///
    /// This is the SINGLE POINT where scroll calculation happens (SOLID: SRP).
    /// The rendering code (`tui.rs`) should use this method instead of
    /// calculating the visible window itself.
    ///
    /// Returns a slice of lines starting at `scroll_position`.
    pub fn visible_window(&self, visible_lines: usize) -> &[Line<'static>] {
        let start = self.scroll_position;
        let end = (start + visible_lines).min(self.parsed_buffer.len());
        &self.parsed_buffer[start..end]
    }

    /// Get output line count only (without prompt/interaction lines)
    pub fn total_lines(&self) -> usize {
        self.parsed_buffer.len()
    }

    /// Get total content lines (output + prompt + interaction)
    /// This is the total scrollable content
    fn total_content_lines(&self) -> usize {
        self.parsed_buffer.len() + self.extra_lines
    }

    /// Calculate maximum scroll position
    /// Returns 0 if visible_lines is 0 (before first render)
    fn max_scroll(&self) -> usize {
        if self.visible_lines == 0 {
            return 0; // No scroll before first render
        }
        self.total_content_lines()
            .saturating_sub(self.visible_lines)
    }

    /// Set extra lines count (prompt, interaction, etc.)
    /// Called by rendering to keep scroll calculations accurate
    pub fn set_extra_lines(&mut self, extra: usize) {
        self.extra_lines = extra;
    }

    /// Check if scroll position is at the bottom.
    ///
    /// Used for smart auto-scroll: only auto-scroll when user was already at bottom.
    pub fn is_at_bottom(&self) -> bool {
        self.scroll_position >= self.max_scroll()
    }

    /// Get the current scroll position
    pub const fn scroll_position(&self) -> usize {
        self.scroll_position
    }

    /// Scroll up by one line (moves view window up)
    pub fn scroll_up(&mut self) {
        if self.scroll_position > 0 {
            self.scroll_position -= 1;
        }
    }

    /// Scroll down by one line (moves view window down)
    pub fn scroll_down(&mut self) {
        let max = self.max_scroll();
        if self.scroll_position < max {
            self.scroll_position += 1;
        }
    }

    /// Set scroll position directly (for scrollbar drag)
    /// Clamps to valid range [0, max_scroll]
    pub fn set_scroll_position(&mut self, position: usize) {
        self.scroll_position = position.min(self.max_scroll());
    }

    /// Set scroll position without clamping (used by rendering after proper clamp)
    pub fn set_scroll_position_exact(&mut self, position: usize) {
        self.scroll_position = position;
    }

    /// Scroll to the end of content (where prompt is)
    /// Called when user types to bring prompt back into view
    pub fn scroll_to_end(&mut self) {
        self.scroll_position = self.max_scroll();
    }

    /// Set the number of visible lines for scroll calculations
    /// Call this when the terminal is resized
    pub fn set_visible_lines(&mut self, visible_lines: usize) {
        // Check if we were at bottom before changing visible_lines
        let was_at_bottom = self.is_at_bottom();

        self.visible_lines = visible_lines;

        // Calculate new max_scroll with updated visible_lines
        let max = self.max_scroll();

        if was_at_bottom {
            // If we were at bottom, stay at bottom
            self.scroll_position = max;
        } else if self.scroll_position > max {
            // Clamp scroll position to valid range
            self.scroll_position = max;
        }
    }

    /// Trim buffer if it exceeds maximum size
    fn trim_if_needed(&mut self) {
        if self.buffer.len() > MAX_OUTPUT_LINES {
            let lines_to_remove = self.buffer.len() - MAX_OUTPUT_LINES + TRIM_LINES;
            self.buffer.drain(0..lines_to_remove);
            self.parsed_buffer.drain(0..lines_to_remove);
            self.scroll_position = self.scroll_position.saturating_sub(lines_to_remove);
        }
    }

    /// Smart auto-scroll: only scroll to bottom if user was already at bottom.
    ///
    /// This follows Linux/Mac terminal behavior:
    /// - If user is viewing old output (scrolled up), new output doesn't move the view
    /// - If user is at the bottom, new output auto-scrolls to stay at bottom
    fn auto_scroll_to_bottom(&mut self) {
        // Skip auto-scroll if visible_lines not yet set (before first render)
        if self.visible_lines == 0 {
            return;
        }
        let total = self.total_content_lines();
        let max = self.max_scroll();
        // Auto-scroll only if:
        // 1. Buffer is empty or just started (scroll_position == 0 and content fits)
        // 2. User was already at bottom (is_at_bottom() would be true before adding line)
        // Since we call this AFTER adding the line, we check if we're at or near max
        let was_at_bottom = self.scroll_position + 1 >= max || total <= self.visible_lines;
        if was_at_bottom {
            self.scroll_position = max;
        }
    }

    /// Remove the last line from the buffer (used for removing temporary messages)
    pub fn pop(&mut self) -> Option<String> {
        self.parsed_buffer.pop();
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
        self.parsed_buffer.clear();
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
    char_count: usize,      // Cached character count for O(1) access
}

impl InputBuffer {
    /// Create a new empty input buffer
    pub const fn new() -> Self {
        Self {
            buffer: String::new(),
            cursor_position: 0,
            char_count: 0,
        }
    }

    /// Get the cached character count (O(1) instead of O(N))
    pub const fn char_count(&self) -> usize {
        self.char_count
    }

    /// Get the current input text
    pub fn text(&self) -> &str {
        &self.buffer
    }

    /// Get the cursor position (in characters, not bytes)
    pub const fn cursor_position(&self) -> usize {
        self.cursor_position
    }

    /// Convert character position to byte index in the string
    fn char_to_byte_idx(&self, char_idx: usize) -> usize {
        self.buffer
            .char_indices()
            .nth(char_idx)
            .map_or(self.buffer.len(), |(byte_idx, _)| byte_idx)
    }

    /// Insert a character at the cursor position
    pub fn insert_char(&mut self, c: char) {
        // Use cached char_count (O(1) instead of O(N))
        // Defensive: ensure cursor position is within valid range
        debug_assert!(
            self.cursor_position <= self.char_count,
            "Cursor position {} exceeds character count {}",
            self.cursor_position,
            self.char_count
        );
        // Clamp cursor position to prevent panic
        self.cursor_position = self.cursor_position.min(self.char_count);

        // Convert char position to byte index
        let byte_idx = self.char_to_byte_idx(self.cursor_position);
        self.buffer.insert(byte_idx, c);
        self.cursor_position += 1;
        self.char_count += 1; // O(1) update
    }

    /// Delete character before cursor (backspace)
    pub fn delete_char(&mut self) {
        if self.cursor_position > 0 {
            let byte_idx = self.char_to_byte_idx(self.cursor_position - 1);
            self.buffer.remove(byte_idx);
            self.cursor_position -= 1;
            self.char_count -= 1; // O(1) update
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
        // Use cached char_count (O(1) instead of O(N))
        if self.cursor_position < self.char_count {
            self.cursor_position += 1;
        }
    }

    /// Clear the input buffer and reset cursor
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.cursor_position = 0;
        self.char_count = 0;
    }

    /// Set the input buffer to a specific text and position cursor at end
    pub fn set_text(&mut self, text: String) {
        // Must compute char_count once when setting text (unavoidable O(N))
        // but cached for all subsequent operations (O(1))
        self.char_count = text.chars().count();
        self.cursor_position = self.char_count;
        self.buffer = text;
    }

    /// Take the current input, leaving the buffer empty
    pub fn take(&mut self) -> String {
        self.cursor_position = 0;
        self.char_count = 0;
        std::mem::take(&mut self.buffer)
    }

    /// Calculate cursor position when input wraps across multiple lines.
    /// Returns (total_visual_lines, cursor_row, cursor_col).
    ///
    /// - `prompt_width`: width of the prompt on the first line
    /// - `terminal_width`: total width of the terminal
    pub fn calculate_wrapped_cursor(
        &self,
        prompt_width: usize,
        terminal_width: usize,
    ) -> (usize, usize, usize) {
        use unicode_width::UnicodeWidthChar;

        // Edge case: zero-width terminal
        if terminal_width == 0 {
            return (1, 0, 0);
        }

        let text = self.text();
        let cursor_pos = self.cursor_position;

        let mut current_row = 0;
        let mut current_col = prompt_width; // First line starts after prompt

        for (i, c) in text.chars().enumerate() {
            // Check if cursor is at this position
            if i == cursor_pos {
                return (current_row + 1, current_row, current_col);
            }

            let char_width = c.width().unwrap_or(1);

            // Check if adding this char would exceed terminal width
            if current_col + char_width > terminal_width {
                // Wrap to next line
                current_row += 1;
                current_col = char_width;
            } else {
                current_col += char_width;
            }
        }

        // Cursor is at the end of input
        if current_col >= terminal_width {
            // Cursor wraps to start of new line
            (current_row + 2, current_row + 1, 0)
        } else {
            (current_row + 1, current_row, current_col)
        }
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
    pub const fn new() -> Self {
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

    /// Search history for commands containing the query (reverse order - most recent first)
    /// Returns indices of matching commands. Search is case-insensitive (matches bash behavior).
    ///
    /// # Performance
    /// O(N × M) where N = history size, M = average command length.
    /// Called once per keystroke during reverse search (results are cached in `ReverseSearchState`).
    ///
    /// TODO(M2): Consider early termination after K matches for very large histories (10k+).
    pub fn search(&self, query: &str) -> Vec<usize> {
        if query.is_empty() {
            return Vec::new();
        }
        let query_lower = query.to_lowercase();
        self.history
            .iter()
            .enumerate()
            .rev() // Most recent first
            .filter(|(_, cmd)| cmd.to_lowercase().contains(&query_lower))
            .map(|(idx, _)| idx)
            .collect()
    }

    /// Get command at specific index
    pub fn get(&self, index: usize) -> Option<&String> {
        self.history.get(index)
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
        // Set extra_lines to 0 for unit test (no prompt in isolation)
        buffer.set_extra_lines(0);
        // Set visible lines first (simulating TUI setup)
        buffer.set_visible_lines(2);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // With 3 lines and 2 visible, max_scroll = 3-2 = 1
        // Auto-scrolls to bottom (scroll_position = max_scroll = 1)
        assert_eq!(buffer.scroll_position(), 1);

        // Scroll up from position 1 to 0
        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 0);

        // Try scrolling up past 0 - should stay at 0
        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 0);

        // Scroll down back to max
        buffer.scroll_down();
        assert_eq!(buffer.scroll_position(), 1);

        // Try scrolling down past max - should stay at max
        buffer.scroll_down();
        assert_eq!(buffer.scroll_position(), 1);
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
        history.add(String::new());
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
    fn test_set_text_unicode() {
        let mut buffer = InputBuffer::new();

        // Test with Chinese characters (3 bytes each)
        buffer.set_text("中文测试".to_string()); // 4 chars, 12 bytes
        assert_eq!(buffer.cursor_position(), 4); // Should be 4 chars, not 12 bytes
        assert_eq!(buffer.text(), "中文测试");

        // Test with emoji (4 bytes each)
        buffer.set_text("😀😃😄".to_string()); // 3 chars, 12 bytes
        assert_eq!(buffer.cursor_position(), 3); // Should be 3 chars, not 12 bytes

        // Test with mixed ASCII and multi-byte
        buffer.set_text("Hello世界".to_string()); // 7 chars (5 ASCII + 2 CJK)
        assert_eq!(buffer.cursor_position(), 7); // Should be 7 chars

        // Test that cursor is at end (can't move right)
        buffer.move_cursor_right();
        assert_eq!(buffer.cursor_position(), 7); // Still at end
    }

    #[test]
    fn test_output_buffer_pop_adjusts_scroll() {
        let mut buffer = OutputBuffer::new();
        buffer.set_extra_lines(0); // No prompt in unit test
                                   // Set visible lines to 1 so we have scrollable content
        buffer.set_visible_lines(1);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // With 3 lines and 1 visible, max_scroll = 2
        // Auto-scrolls to bottom (scroll_position = 2)
        assert_eq!(buffer.scroll_position(), 2);

        // Scroll up from bottom
        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 1);

        // Pop last line (now 2 lines, max_scroll = 2-1 = 1)
        let popped = buffer.pop();
        assert_eq!(popped, Some("line 3".to_string()));
        assert_eq!(buffer.lines().len(), 2);

        // Scroll position should still be valid (1 <= max_scroll=1)
        assert_eq!(buffer.scroll_position(), 1);
    }

    #[test]
    fn test_output_buffer_pop_empty() {
        let mut buffer = OutputBuffer::new();
        assert_eq!(buffer.pop(), None);
        assert_eq!(buffer.scroll_position(), 0);
    }

    #[test]
    fn test_visible_window() {
        let mut buffer = OutputBuffer::new();
        buffer.set_extra_lines(0); // No prompt in unit test
        buffer.set_visible_lines(2);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // At scroll_position=1 (bottom), visible window is lines 2-3
        let window = buffer.visible_window(2);
        assert_eq!(window.len(), 2);

        // Scroll to top
        buffer.scroll_up();
        let window = buffer.visible_window(2);
        assert_eq!(window.len(), 2);
        assert_eq!(buffer.scroll_position(), 0);
    }

    #[test]
    fn test_total_lines() {
        let mut buffer = OutputBuffer::new();
        assert_eq!(buffer.total_lines(), 0);

        buffer.add_line("line 1".to_string());
        assert_eq!(buffer.total_lines(), 1);

        buffer.add_line("line 2".to_string());
        assert_eq!(buffer.total_lines(), 2);
    }

    #[test]
    fn test_is_at_bottom() {
        let mut buffer = OutputBuffer::new();
        buffer.set_extra_lines(0); // No prompt in unit test
        buffer.set_visible_lines(2);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // Should be at bottom after adding lines
        assert!(buffer.is_at_bottom());

        // Scroll up - no longer at bottom
        buffer.scroll_up();
        assert!(!buffer.is_at_bottom());

        // Scroll back down - at bottom again
        buffer.scroll_down();
        assert!(buffer.is_at_bottom());
    }

    #[test]
    fn test_smart_auto_scroll_stays_at_bottom() {
        let mut buffer = OutputBuffer::new();
        buffer.set_extra_lines(0); // No prompt in unit test
        buffer.set_visible_lines(2);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());

        // Start at bottom
        assert!(buffer.is_at_bottom());

        // Add new line while at bottom - should stay at bottom
        buffer.add_line("line 3".to_string());
        assert!(buffer.is_at_bottom());
        assert_eq!(buffer.scroll_position(), 1); // max_scroll = 3-2 = 1
    }

    #[test]
    fn test_smart_auto_scroll_preserves_scroll_position() {
        let mut buffer = OutputBuffer::new();
        buffer.set_extra_lines(0); // No prompt in unit test
        buffer.set_visible_lines(2);

        buffer.add_line("line 1".to_string());
        buffer.add_line("line 2".to_string());
        buffer.add_line("line 3".to_string());

        // Scroll to top
        buffer.scroll_up();
        assert_eq!(buffer.scroll_position(), 0);
        assert!(!buffer.is_at_bottom());

        // Add new line while scrolled up - should NOT auto-scroll
        buffer.add_line("line 4".to_string());
        assert_eq!(buffer.scroll_position(), 0); // Position preserved
        assert!(!buffer.is_at_bottom()); // Still not at bottom
    }

    // === Reverse History Search Tests ===

    #[test]
    fn test_command_history_search_basic() {
        let mut history = CommandHistory::new();
        history.add("ls -la".to_string());
        history.add("cd /home".to_string());
        history.add("git status".to_string());
        history.add("ls -lh".to_string());

        // Search for "ls" should find both ls commands
        let matches = history.search("ls");
        assert_eq!(matches.len(), 2);
        // Most recent first
        assert_eq!(matches[0], 3); // "ls -lh"
        assert_eq!(matches[1], 0); // "ls -la"
    }

    #[test]
    fn test_command_history_search_case_insensitive() {
        let mut history = CommandHistory::new();
        history.add("Git Status".to_string());
        history.add("GIT PUSH".to_string());
        history.add("git pull".to_string());

        // Lowercase query should find all git commands
        let matches = history.search("git");
        assert_eq!(matches.len(), 3);

        // Uppercase query should also find all
        let matches = history.search("GIT");
        assert_eq!(matches.len(), 3);

        // Mixed case should work too
        let matches = history.search("Git");
        assert_eq!(matches.len(), 3);
    }

    #[test]
    fn test_command_history_search_empty_query() {
        let mut history = CommandHistory::new();
        history.add("ls".to_string());
        history.add("cd".to_string());

        let matches = history.search("");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_command_history_search_no_matches() {
        let mut history = CommandHistory::new();
        history.add("ls".to_string());
        history.add("cd".to_string());

        let matches = history.search("nonexistent");
        assert!(matches.is_empty());
    }

    #[test]
    fn test_command_history_get() {
        let mut history = CommandHistory::new();
        history.add("cmd1".to_string());
        history.add("cmd2".to_string());

        assert_eq!(history.get(0), Some(&"cmd1".to_string()));
        assert_eq!(history.get(1), Some(&"cmd2".to_string()));
        assert_eq!(history.get(2), None);
    }
}
