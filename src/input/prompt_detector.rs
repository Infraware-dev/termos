//! Interactive prompt detection for PTY output.
//!
//! Detects patterns like `[sudo]`, `Password:`, etc. in PTY output
//! to determine when the terminal is waiting for user input (e.g., password).

use regex::Regex;

/// Detects interactive prompts in PTY output.
///
/// Used to hide the throbber when the shell is waiting for user input
/// (e.g., sudo password, SSH passphrase, confirmation prompts).
#[derive(Debug)]
pub struct PromptDetector {
    /// Compiled regex patterns for prompt detection
    patterns: Vec<Regex>,
    /// Buffer for the last line of PTY output (for end-of-line matching)
    last_line_buffer: String,
    /// Whether an interactive prompt is currently detected
    prompt_detected: bool,
}

impl Default for PromptDetector {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptDetector {
    /// Creates a new prompt detector with default patterns.
    #[must_use]
    pub fn new() -> Self {
        // Patterns that indicate an interactive prompt requiring user input
        // These are matched against the end of the last line of output
        let pattern_strings = [
            // sudo prompts
            r"\[sudo\]",
            r"[Pp]assword:",
            r"password for \w+",
            // SSH prompts
            r"[Ee]nter passphrase",
            r"passphrase for",
            r"\(yes/no\)\??\s*$",
            r"\(yes/no/\[fingerprint\]\)",
            // GPG prompts
            r"[Ee]nter PIN",
            r"[Pp]assphrase:",
            // Generic confirmation prompts (at end of line)
            r"\[y/N\]:?\s*$",
            r"\[Y/n\]:?\s*$",
            r"\(y/n\)\s*$",
            // apt/package manager
            r"[Dd]o you want to continue\?",
            // Generic colon prompts (must be at end with no following text)
            r":\s*$",
        ];

        let patterns = pattern_strings
            .iter()
            .filter_map(|p| {
                Regex::new(p)
                    .map_err(|e| tracing::warn!("Invalid prompt pattern '{}': {}", p, e))
                    .ok()
            })
            .collect();

        Self {
            patterns,
            last_line_buffer: String::with_capacity(256),
            prompt_detected: false,
        }
    }

    /// Process PTY output and detect interactive prompts.
    ///
    /// Returns `true` if a prompt was newly detected in this output.
    ///
    /// # Arguments
    /// * `output` - Raw bytes from PTY output
    pub fn process_output(&mut self, output: &[u8]) -> bool {
        // Convert to string, handling invalid UTF-8 gracefully
        let text = String::from_utf8_lossy(output);

        // Reset detection on new output (unless it's just the prompt itself)
        // We only keep prompt_detected if this output continues the same line
        let has_newline = text.contains('\n') || text.contains('\r');

        if has_newline {
            // New line(s) received - reset buffer and check last line
            self.prompt_detected = false;

            // Get the last line from the new output
            let lines: Vec<&str> = text.split(['\n', '\r']).collect();
            if let Some(last) = lines.last() {
                self.last_line_buffer.clear();
                self.last_line_buffer.push_str(last.trim());
            }
        } else {
            // Continuation of current line - append to buffer
            self.last_line_buffer.push_str(&text);
            // Keep buffer from growing too large
            if self.last_line_buffer.len() > 512 {
                let drain_len = self.last_line_buffer.len() - 256;
                self.last_line_buffer.drain(..drain_len);
            }
        }

        // Check if the last line matches any prompt pattern
        let was_detected = self.prompt_detected;
        self.prompt_detected = self.check_patterns(&self.last_line_buffer.clone());

        // Return true only if newly detected
        !was_detected && self.prompt_detected
    }

    /// Check if text matches any prompt pattern.
    fn check_patterns(&self, text: &str) -> bool {
        // Skip empty text
        if text.trim().is_empty() {
            return false;
        }

        // Skip if text looks like normal command output (has lots of content after prompt-like text)
        // This reduces false positives
        if text.len() > 100 && !text.ends_with(':') && !text.ends_with('?') {
            return false;
        }

        self.patterns.iter().any(|pattern| pattern.is_match(text))
    }

    /// Check if an interactive prompt is currently active.
    #[must_use]
    pub fn is_prompt_active(&self) -> bool {
        self.prompt_detected
    }

    /// Clear prompt detection state.
    ///
    /// Should be called when user starts typing (any keystroke clears the prompt state,
    /// since the user is now interacting with whatever was waiting).
    pub fn clear(&mut self) {
        self.prompt_detected = false;
        self.last_line_buffer.clear();
    }

    /// Get the current buffer content (for debugging).
    #[cfg(test)]
    pub fn buffer(&self) -> &str {
        &self.last_line_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sudo_prompt() {
        let mut detector = PromptDetector::new();

        // Simulate sudo prompt
        assert!(detector.process_output(b"[sudo] password for user: "));
        assert!(detector.is_prompt_active());

        // Clear on user input
        detector.clear();
        assert!(!detector.is_prompt_active());
    }

    #[test]
    fn test_password_prompt() {
        let mut detector = PromptDetector::new();

        assert!(detector.process_output(b"Password: "));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_ssh_yes_no() {
        let mut detector = PromptDetector::new();

        let prompt = b"Are you sure you want to continue connecting (yes/no)? ";
        assert!(detector.process_output(prompt));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_ssh_fingerprint() {
        let mut detector = PromptDetector::new();

        let prompt = b"Are you sure you want to continue (yes/no/[fingerprint])? ";
        assert!(detector.process_output(prompt));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_passphrase_prompt() {
        let mut detector = PromptDetector::new();

        assert!(detector.process_output(b"Enter passphrase for key '/home/user/.ssh/id_rsa': "));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_apt_continue() {
        let mut detector = PromptDetector::new();

        assert!(detector.process_output(b"Do you want to continue? [Y/n] "));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_newline_resets() {
        let mut detector = PromptDetector::new();

        // First, detect a prompt
        detector.process_output(b"Password: ");
        assert!(detector.is_prompt_active());

        // New output with newline resets detection
        detector.process_output(b"\nsome other output\n");
        assert!(!detector.is_prompt_active());
    }

    #[test]
    fn test_false_positive_in_text() {
        let mut detector = PromptDetector::new();

        // "Password:" appearing in normal output (e.g., help text) with lots of content
        let long_text = b"The password: field in /etc/passwd contains the encrypted password or a placeholder character. Use 'passwd' command to change it.";
        detector.process_output(long_text);
        // Should NOT detect because it's long text, not an actual prompt
        assert!(!detector.is_prompt_active());
    }

    #[test]
    fn test_continuation() {
        let mut detector = PromptDetector::new();

        // Simulate output coming in chunks - partial text that doesn't match
        detector.process_output(b"Enter ");
        assert!(!detector.is_prompt_active()); // Not yet - "Enter " alone doesn't match

        // Complete the prompt
        detector.process_output(b"passphrase for key: ");
        assert!(detector.is_prompt_active()); // Now detected - full pattern matches
    }

    #[test]
    fn test_yn_prompt() {
        let mut detector = PromptDetector::new();

        assert!(detector.process_output(b"Continue? [y/N]: "));
        assert!(detector.is_prompt_active());
    }

    #[test]
    fn test_clear() {
        let mut detector = PromptDetector::new();

        detector.process_output(b"Password: ");
        assert!(detector.is_prompt_active());

        detector.clear();
        assert!(!detector.is_prompt_active());
        assert!(detector.buffer().is_empty());
    }
}
