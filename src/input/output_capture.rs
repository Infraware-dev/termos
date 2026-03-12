//! Output capture module for PTY command execution.
//!
//! Captures command output for sending to the backend after command completion.
//! Detects when the shell prompt reappears to determine command completion.
//! Uses a debounce window to avoid false positives from partial PTY chunks.

use std::time::{Duration, Instant};

use once_cell::sync::Lazy;
use regex::Regex;

/// Regex for stripping ANSI escape sequences from strings.
/// Matches:
/// - CSI sequences: \x1b[...X where params may include `?` for DEC private modes
///   (e.g., `\x1b[?2004h` for bracketed paste)
/// - OSC sequences: \x1b]...\x07 (window title, etc.)
/// - Charset sequences: \x1b(A, \x1b)B, etc.
/// - Other escapes: \x1b>, \x1b=, \x1b<
static ANSI_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07|\x1b[()][AB012]|\x1b[>=<]").unwrap()
});

/// Strip ANSI escape sequences from a string.
///
/// Handles standard CSI sequences (colors, styles), DEC private mode
/// sequences (`\x1b[?2004h`), OSC sequences, and charset sequences.
fn strip_ansi(s: &str) -> String {
    ANSI_RE.replace_all(s, "").to_string()
}

/// Default debounce window before confirming prompt detection.
///
/// PTY data arrives in arbitrary chunks, so a prompt-like pattern may appear
/// at the end of a partial chunk before more output follows. Waiting this long
/// after the last prompt detection with no new non-prompt data avoids false
/// positives.
const DEFAULT_PROMPT_DEBOUNCE: Duration = Duration::from_millis(150);

/// Captures PTY output during command execution.
///
/// When a command is approved by the user, `OutputCapture` starts capturing
/// all PTY output. When the shell prompt reappears (detected via regex patterns)
/// and no new output arrives within the debounce window, the captured output is
/// available for sending to the backend.
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
    /// Timestamp when a prompt pattern was first detected at buffer tail.
    /// Reset to `None` when new data pushes a non-prompt line to the tail.
    prompt_detected_at: Option<Instant>,
    /// Debounce window: prompt must persist this long with no new data before
    /// we report command completion.
    prompt_debounce: Duration,
}

impl Default for OutputCapture {
    fn default() -> Self {
        Self::new()
    }
}

impl OutputCapture {
    /// Create a new output capture instance.
    #[must_use]
    pub fn new() -> Self {
        // Patterns to detect shell prompt reappearance.
        // These indicate the command has finished executing.
        //
        // IMPORTANT: patterns are intentionally specific to avoid false positives
        // from command output lines that happen to end with `$`, `#`, or `%`.
        let prompt_patterns = vec![
            // user@host:path$ / user@host:path# (most common)
            Regex::new(r"^.*[\w\-]+@[\w\-]+:.*[$#>]\s*$").unwrap(),
            // user@host$ without colon separator
            Regex::new(r"^.*[\w\-]+@[\w\-]+\s*[$#>]\s*$").unwrap(),
            // Bare prompt characters (only whitespace before)
            Regex::new(r"^\s*[$#>]\s*$").unwrap(),
            // (venv) prefix with user@host
            Regex::new(r"^\(.*\)\s*[\w\-]+@[\w\-]+.*[$#>]\s*$").unwrap(),
            // Path-like prompt: /some/path$ or ~/path# (no user@host)
            Regex::new(r"^[~/][\w\-/\.~]*\s*[$#>%]\s*$").unwrap(),
            // PowerShell style: PS> or username>
            Regex::new(r"^[\w\-]+\s*>\s*$").unwrap(),
            // zsh with username: user ~/path %
            Regex::new(r"^[\w\-]+\s+[~/][\w\-/\.~]*\s*%\s*$").unwrap(),
            // starship/pure prompt
            Regex::new(r"^❯\s*$").unwrap(),
        ];

        Self {
            buffer: String::with_capacity(4096),
            capturing: false,
            current_command: None,
            prompt_patterns,
            lines_received: 0,
            skip_command_echo: true,
            prompt_detected_at: None,
            prompt_debounce: DEFAULT_PROMPT_DEBOUNCE,
        }
    }

    /// Create an output capture instance with a custom debounce duration.
    ///
    /// Useful in tests where the default debounce would slow things down.
    #[cfg(test)]
    #[must_use]
    pub fn with_debounce(debounce: Duration) -> Self {
        let mut capture = Self::new();
        capture.prompt_debounce = debounce;
        capture
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
        self.prompt_detected_at = None;
    }

    /// Check if capture is currently active.
    #[must_use]
    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    /// Get the command being executed.
    #[cfg(test)]
    #[must_use]
    pub fn current_command(&self) -> Option<&str> {
        self.current_command.as_deref()
    }

    /// Append output from the VTE parser.
    ///
    /// Updates internal prompt-detection state. After calling this, use
    /// [`is_command_complete`] to check whether the command has finished
    /// (prompt detected and debounce window elapsed).
    pub fn append(&mut self, text: &str) {
        if !self.capturing {
            return;
        }

        // Track newlines to count lines
        let newline_count = text.chars().filter(|&c| c == '\n').count();
        self.lines_received += newline_count;

        // Skip the first line which is usually the command echo
        if self.skip_command_echo && self.lines_received == 0 && !text.contains('\n') {
            // Still on first line, might be command echo
            return;
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

        // Update prompt-detection timestamp
        if self.check_prompt_at_tail() {
            // Prompt pattern present at buffer tail — start or maintain debounce
            if self.prompt_detected_at.is_none() {
                tracing::debug!(
                    "OutputCapture: Prompt detected, starting debounce. Last line: '{}'",
                    self.get_last_line()
                );
                self.prompt_detected_at = Some(Instant::now());
            }
        } else {
            // New data pushed a non-prompt line to the tail — reset
            if self.prompt_detected_at.is_some() {
                tracing::debug!("OutputCapture: Prompt detection reset by new output");
            }
            self.prompt_detected_at = None;
        }
    }

    /// Returns `true` when the command is considered complete.
    ///
    /// Completion requires:
    /// 1. A prompt pattern was detected at the buffer tail
    /// 2. The debounce window has elapsed with no new non-prompt output
    #[must_use]
    pub fn is_command_complete(&self) -> bool {
        self.prompt_detected_at
            .is_some_and(|t| t.elapsed() >= self.prompt_debounce)
    }

    /// Check whether the last line of the buffer matches a shell prompt pattern.
    ///
    /// This is a raw check with no debounce — callers should use
    /// [`is_command_complete`] for the debounced result.
    fn check_prompt_at_tail(&self) -> bool {
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

        // Strip ANSI escape codes before matching — shell prompts often have colors
        let clean_line = strip_ansi(last_line);

        // Check if last line matches any prompt pattern
        self.prompt_patterns
            .iter()
            .any(|pattern| pattern.is_match(&clean_line))
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
        self.prompt_detected_at = None;
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
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a capture with zero debounce so tests don't need to sleep.
    fn zero_debounce_capture() -> OutputCapture {
        OutputCapture::with_debounce(Duration::ZERO)
    }

    #[test]
    fn test_basic_capture() {
        let mut capture = zero_debounce_capture();

        // Start capture
        capture.start("ls -la");
        assert!(capture.is_capturing());
        assert_eq!(capture.current_command(), Some("ls -la"));

        // Simulate command output
        capture.append("ls -la\n"); // Command echo
        assert!(!capture.is_command_complete());
        capture.append("total 16\n"); // Output
        assert!(!capture.is_command_complete());
        capture.append("drwxr-xr-x  2 user user 4096 Jan  1 12:00 .\n");
        assert!(!capture.is_command_complete());
        capture.append("user@host:~$ "); // Prompt
        assert!(capture.is_command_complete());

        let output = capture.take_output();
        assert!(output.contains("total 16"));
        assert!(!output.contains("user@host")); // Prompt removed
        assert!(!capture.is_capturing());
    }

    #[test]
    fn test_prompt_patterns() {
        let mut capture = zero_debounce_capture();
        capture.start("echo test");

        // Test various prompt styles
        capture.lines_received = 2; // Pretend we've received lines

        capture.buffer = "test\nuser@host:~$ ".to_string();
        assert!(capture.check_prompt_at_tail());

        capture.buffer = "test\n$ ".to_string();
        assert!(capture.check_prompt_at_tail());

        capture.buffer = "test\n# ".to_string();
        assert!(capture.check_prompt_at_tail());

        capture.buffer = "test\n(venv) user@host:~/project$ ".to_string();
        assert!(capture.check_prompt_at_tail());

        capture.buffer = "test\n~/project$ ".to_string();
        assert!(capture.check_prompt_at_tail());
    }

    #[test]
    fn test_not_prompt() {
        let mut capture = zero_debounce_capture();
        capture.start("cat file.txt");
        capture.lines_received = 2;

        // These should NOT be detected as prompts
        capture.buffer = "The price is $50\n".to_string();
        assert!(!capture.check_prompt_at_tail());

        capture.buffer = "Use # for comments\n".to_string();
        assert!(!capture.check_prompt_at_tail());

        // Lines ending with # that are NOT prompts (previously false positives)
        capture.buffer = "test\nStep 1: install packages #".to_string();
        assert!(!capture.check_prompt_at_tail());

        capture.buffer = "test\nThe total is $99".to_string();
        assert!(!capture.check_prompt_at_tail());

        capture.buffer = "test\nProgress: 50%".to_string();
        assert!(!capture.check_prompt_at_tail());
    }

    #[test]
    fn test_clean_output() {
        let mut capture = zero_debounce_capture();
        capture.start("uname -a");

        capture.buffer = "uname -a\nLinux hostname 5.15.0\nuser@host:~$ ".to_string();
        capture.lines_received = 3;

        let output = capture.get_clean_output();
        assert_eq!(output, "Linux hostname 5.15.0");
    }

    #[test]
    fn test_buffer_limit() {
        let mut capture = zero_debounce_capture();
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

        // Test DEC private mode sequences (bracketed paste mode)
        let with_dec = "\x1b[?2004h|~| root@7b8900c12e7c:/# ";
        assert_eq!(strip_ansi(with_dec), "|~| root@7b8900c12e7c:/# ");

        // Test mixed DEC private mode + color codes
        let mixed = "\x1b[?2004h\x1b[38;2;198;208;214m|~| root@host:/# \x1b[0m\x1b[K";
        assert_eq!(strip_ansi(mixed), "|~| root@host:/# ");
    }

    #[test]
    fn test_colored_prompt_detection() {
        let mut capture = zero_debounce_capture();
        capture.start("uname -s");
        capture.lines_received = 2;

        // Simulate colored prompt (common in bash with PS1 colors)
        capture.buffer = "Linux\n\x1b[01;32muser@host\x1b[00m:\x1b[01;34m~\x1b[00m$ ".to_string();
        assert!(
            capture.check_prompt_at_tail(),
            "Should detect colored prompt"
        );

        // Test with green username, blue path (common Ubuntu style)
        capture.buffer =
            "output\n\x1b[32muser\x1b[0m@\x1b[32mhost\x1b[0m:\x1b[34m~/dir\x1b[0m$ ".to_string();
        assert!(
            capture.check_prompt_at_tail(),
            "Should detect Ubuntu-style colored prompt"
        );
    }

    #[test]
    fn test_dec_private_mode_prompt_detection() {
        let mut capture = zero_debounce_capture();
        capture.start("ls");
        capture.lines_received = 2;

        // The exact case from the bug report — DEC private mode before prompt
        capture.buffer = "output\n\x1b[?2004h\x1b[38;2;198;208;214m|~| root@7b8900c12e7c:/# \x1b[0m\x1b[K".to_string();
        assert!(
            capture.check_prompt_at_tail(),
            "Should detect prompt with DEC private mode prefix"
        );
    }

    #[test]
    fn test_clean_output_with_ansi() {
        let mut capture = zero_debounce_capture();
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

    #[test]
    fn test_debounce_prevents_premature_completion() {
        // Use a long debounce to verify the window is respected
        let mut capture = OutputCapture::with_debounce(Duration::from_secs(10));

        capture.start("ls -la");
        capture.append("ls -la\n");
        capture.append("file.txt\n");
        capture.append("user@host:~$ "); // Prompt appears

        // Prompt detected but debounce window has not elapsed
        assert!(!capture.is_command_complete());
    }

    #[test]
    fn test_debounce_reset_on_new_output() {
        let mut capture = zero_debounce_capture();

        capture.start("some-command");
        capture.append("some-command\n");
        capture.append("user@host:~$ "); // Looks like prompt
        assert!(capture.prompt_detected_at.is_some());

        // More output arrives — prompt was a false positive in the data
        capture.append("\nactual output continues\n");
        assert!(
            capture.prompt_detected_at.is_none(),
            "Prompt detection should reset when new non-prompt output arrives"
        );
        assert!(!capture.is_command_complete());
    }
}
