//! Output capture module for PTY command execution.
//!
//! Captures command output for sending to the backend after command completion.
//! Detects when the shell prompt reappears to determine command completion.

use once_cell::sync::Lazy;
use regex::Regex;

/// Regex for stripping ANSI escape sequences from strings.
/// Matches:
/// - CSI sequences: \x1b[...m (colors, styles)
/// - OSC sequences: \x1b]...\x07 (window title, etc.)
/// - Charset sequences: \x1b(A, \x1b)B, etc.
/// - Other escapes: \x1b>, \x1b=, \x1b<
static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]|\x1b[>=<]").unwrap()
});

/// Strip ANSI escape sequences from a string.
///
/// This is important for prompt detection since shell prompts often contain
/// color codes that would break regex matching.
fn strip_ansi(s: &str) -> String {
    ANSI_RE.replace_all(s, "").to_string()
}

/// Captures PTY output during command execution.
///
/// When a command is approved by the user, `OutputCapture` starts capturing
/// all PTY output. When the shell prompt reappears (detected via regex patterns),
/// the captured output is available for sending to the backend.
#[derive(Debug)]
pub struct OutputCapture {
    /// Buffer for accumulating output
    buffer: String,
    /// Whether capture is active
    capturing: bool,
    /// The command being executed
    current_command: Option<String>,
    /// Shell prompt detection patterns
    prompt_patterns: Vec<Regex>,
    /// Lines received since command was sent
    lines_received: usize,
    /// Skip the first line (echo of command itself)
    skip_command_echo: bool,
}

impl Default for OutputCapture {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)] // Command execution workflow methods - used by HITL orchestrator
impl OutputCapture {
    /// Create a new output capture instance.
    #[must_use]
    pub fn new() -> Self {
        // Patterns to detect shell prompt reappearance
        // These indicate the command has finished executing
        let prompt_patterns = vec![
            // Common shell prompts
            Regex::new(r"^[\w\-]+@[\w\-]+:.*[$#>]\s*$").unwrap(), // user@host:path$
            Regex::new(r"^[\w\-]+@[\w\-]+.*[$#>]\s*$").unwrap(),  // user@host$
            Regex::new(r"^\s*[$#>]\s*$").unwrap(),                // Just $ or # or >
            Regex::new(r"^.*\$\s*$").unwrap(),                    // Anything ending with $
            Regex::new(r"^.*#\s*$").unwrap(),                     // Anything ending with # (root)
            Regex::new(r"^\(.*\)\s*[$#>]\s*$").unwrap(),          // (venv) user@host$
            Regex::new(r"^[\w\-]+\s*>\s*$").unwrap(),             // PS> style prompts
            // zsh/fish style
            Regex::new(r"^.*%\s*$").unwrap(), // zsh default
            Regex::new(r"^❯\s*$").unwrap(),   // starship/pure
        ];

        Self {
            buffer: String::with_capacity(4096),
            capturing: false,
            current_command: None,
            prompt_patterns,
            lines_received: 0,
            skip_command_echo: true,
        }
    }

    /// Start capturing output for a command.
    ///
    /// Call this when the user approves a command and it's sent to the PTY.
    pub fn start(&mut self, command: &str) {
        tracing::debug!("OutputCapture: Starting capture for command: {}", command);
        self.buffer.clear();
        self.capturing = true;
        self.current_command = Some(command.to_string());
        self.lines_received = 0;
        self.skip_command_echo = true;
    }

    /// Stop capturing and reset state.
    pub fn stop(&mut self) {
        tracing::debug!("OutputCapture: Stopping capture");
        self.capturing = false;
        self.current_command = None;
        self.lines_received = 0;
    }

    /// Check if capture is currently active.
    #[must_use]
    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    /// Check if there is any captured output.
    #[must_use]
    pub fn has_output(&self) -> bool {
        !self.buffer.is_empty()
    }

    /// Get the command being executed.
    #[must_use]
    #[allow(dead_code)]
    pub fn current_command(&self) -> Option<&str> {
        self.current_command.as_deref()
    }

    /// Append output from the VTE parser.
    ///
    /// Returns `true` if the command appears to have completed (prompt detected).
    pub fn append(&mut self, text: &str) -> bool {
        if !self.capturing {
            return false;
        }

        // Track newlines to count lines
        let newline_count = text.chars().filter(|&c| c == '\n').count();
        self.lines_received += newline_count;

        // Skip the first line which is usually the command echo
        if self.skip_command_echo && self.lines_received == 0 && !text.contains('\n') {
            // Still on first line, might be command echo
            return false;
        }

        // After first newline, we're past the command echo
        if self.skip_command_echo && newline_count > 0 {
            self.skip_command_echo = false;
        }

        // Append to buffer
        self.buffer.push_str(text);

        // Keep buffer from growing too large (limit to ~1MB)
        if self.buffer.len() > 1_000_000 {
            // Keep the last 500KB
            let keep_from = self.buffer.len() - 500_000;
            self.buffer.drain(..keep_from);
        }

        // Check if prompt appeared (command completed)
        self.check_command_complete()
    }

    /// Check if the shell prompt has reappeared, indicating command completion.
    ///
    /// This is called after each append to detect when the command finishes.
    fn check_command_complete(&self) -> bool {
        // Need at least some output after command
        if self.lines_received < 1 {
            return false;
        }

        // Get the last line of the buffer
        let last_line = self.get_last_line();

        // Empty last line is not a prompt
        if last_line.trim().is_empty() {
            return false;
        }

        // Strip ANSI escape codes before matching - shell prompts often have colors
        let clean_line = strip_ansi(last_line);

        // Check if last line matches any prompt pattern
        let is_prompt = self
            .prompt_patterns
            .iter()
            .any(|pattern| pattern.is_match(&clean_line));

        if is_prompt {
            tracing::debug!(
                "OutputCapture: Prompt detected, command complete. Last line: '{}' (clean: '{}')",
                last_line,
                clean_line
            );
        }

        is_prompt
    }

    /// Get the last line of the buffer.
    fn get_last_line(&self) -> &str {
        // Find the last newline before the end
        let trimmed = self.buffer.trim_end();
        if let Some(pos) = trimmed.rfind('\n') {
            let after_newline = &trimmed[pos + 1..];
            // If there's content after the last newline, that's our last line
            if !after_newline.is_empty() {
                return after_newline;
            }
            // Otherwise, get the line before that
            let before = &trimmed[..pos];
            if let Some(prev_pos) = before.rfind('\n') {
                return &before[prev_pos + 1..];
            }
            return before;
        }
        trimmed
    }

    /// Take the captured output and reset.
    ///
    /// Returns the output without the final prompt line.
    pub fn take_output(&mut self) -> String {
        let output = self.get_clean_output();
        self.buffer.clear();
        self.capturing = false;
        self.current_command = None;
        self.lines_received = 0;
        output
    }

    /// Get clean output without the command echo and final prompt.
    fn get_clean_output(&self) -> String {
        let mut lines: Vec<&str> = self.buffer.lines().collect();

        // Remove the last line if it's a prompt (strip ANSI codes for matching)
        if let Some(last) = lines.last() {
            let clean_last = strip_ansi(last);
            if self.prompt_patterns.iter().any(|p| p.is_match(&clean_last)) {
                lines.pop();
            }
        }

        // Remove the first line if it matches the command (echo)
        // Also strip ANSI from the line for matching
        if let (Some(first), Some(cmd)) = (lines.first(), &self.current_command) {
            let clean_first = strip_ansi(first);
            if clean_first.trim() == cmd.trim() || clean_first.contains(cmd.as_str()) {
                lines.remove(0);
            }
        }

        lines.join("\n")
    }

    /// Get the current buffer content (for debugging).
    #[cfg(test)]
    pub fn buffer(&self) -> &str {
        &self.buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_capture() {
        let mut capture = OutputCapture::new();

        // Start capture
        capture.start("ls -la");
        assert!(capture.is_capturing());
        assert_eq!(capture.current_command(), Some("ls -la"));

        // Simulate command output
        assert!(!capture.append("ls -la\n")); // Command echo, not complete
        assert!(!capture.append("total 16\n")); // Output, not complete
        assert!(!capture.append("drwxr-xr-x  2 user user 4096 Jan  1 12:00 .\n"));
        assert!(capture.append("user@host:~$ ")); // Prompt - complete!

        let output = capture.take_output();
        assert!(output.contains("total 16"));
        assert!(!output.contains("user@host")); // Prompt removed
        assert!(!capture.is_capturing());
    }

    #[test]
    fn test_prompt_patterns() {
        let mut capture = OutputCapture::new();
        capture.start("echo test");

        // Test various prompt styles
        capture.lines_received = 2; // Pretend we've received lines

        capture.buffer = "test\nuser@host:~$ ".to_string();
        assert!(capture.check_command_complete());

        capture.buffer = "test\n$ ".to_string();
        assert!(capture.check_command_complete());

        capture.buffer = "test\n# ".to_string();
        assert!(capture.check_command_complete());

        capture.buffer = "test\n(venv) user@host:~/project$ ".to_string();
        assert!(capture.check_command_complete());

        capture.buffer = "test\nuser@host % ".to_string();
        assert!(capture.check_command_complete());
    }

    #[test]
    fn test_not_prompt() {
        let mut capture = OutputCapture::new();
        capture.start("cat file.txt");
        capture.lines_received = 2;

        // These should NOT be detected as prompts
        capture.buffer = "The price is $50\n".to_string();
        assert!(!capture.check_command_complete());

        capture.buffer = "Use # for comments\n".to_string();
        assert!(!capture.check_command_complete());
    }

    #[test]
    fn test_clean_output() {
        let mut capture = OutputCapture::new();
        capture.start("uname -a");

        capture.buffer = "uname -a\nLinux hostname 5.15.0\nuser@host:~$ ".to_string();
        capture.lines_received = 3;

        let output = capture.get_clean_output();
        assert_eq!(output, "Linux hostname 5.15.0");
    }

    #[test]
    fn test_stop() {
        let mut capture = OutputCapture::new();
        capture.start("ls");
        assert!(capture.is_capturing());

        capture.stop();
        assert!(!capture.is_capturing());
        assert!(capture.current_command().is_none());
    }

    #[test]
    fn test_buffer_limit() {
        let mut capture = OutputCapture::new();
        capture.start("cat large_file");

        // Simulate large output
        let large_chunk = "x".repeat(600_000);
        capture.append(&large_chunk);

        // Buffer should be limited
        assert!(capture.buffer.len() <= 1_000_000);
    }

    #[test]
    fn test_strip_ansi() {
        // Test basic color codes
        let colored = "\x1b[01;32muser@host\x1b[00m:\x1b[01;34m~\x1b[00m$ ";
        assert_eq!(strip_ansi(colored), "user@host:~$ ");

        // Test no ANSI codes
        assert_eq!(strip_ansi("plain text"), "plain text");

        // Test OSC sequences (window title)
        let with_osc = "\x1b]0;title\x07user@host:~$ ";
        assert_eq!(strip_ansi(with_osc), "user@host:~$ ");

        // Test multiple codes in sequence
        let multi = "\x1b[1m\x1b[32mgreen bold\x1b[0m";
        assert_eq!(strip_ansi(multi), "green bold");
    }

    #[test]
    fn test_colored_prompt_detection() {
        let mut capture = OutputCapture::new();
        capture.start("uname -s");
        capture.lines_received = 2;

        // Simulate colored prompt (common in bash with PS1 colors)
        capture.buffer = "Linux\n\x1b[01;32muser@host\x1b[00m:\x1b[01;34m~\x1b[00m$ ".to_string();
        assert!(
            capture.check_command_complete(),
            "Should detect colored prompt"
        );

        // Test with green username, blue path (common Ubuntu style)
        capture.buffer =
            "output\n\x1b[32muser\x1b[0m@\x1b[32mhost\x1b[0m:\x1b[34m~/dir\x1b[0m$ ".to_string();
        assert!(
            capture.check_command_complete(),
            "Should detect Ubuntu-style colored prompt"
        );
    }

    #[test]
    fn test_clean_output_with_ansi() {
        let mut capture = OutputCapture::new();
        capture.start("uname -s");

        // Simulate command echo with colors and colored prompt
        capture.buffer =
            "\x1b[32muname -s\x1b[0m\nLinux\n\x1b[01;32muser@host\x1b[00m:\x1b[01;34m~\x1b[00m$ "
                .to_string();
        capture.lines_received = 3;

        let output = capture.get_clean_output();
        // Should only contain "Linux", with echo and prompt stripped
        assert_eq!(output, "Linux");
    }
}
