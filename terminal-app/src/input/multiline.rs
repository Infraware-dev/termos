//! Multiline command detection and handling
//!
//! This module provides functions to detect incomplete shell commands that require
//! additional input lines before they can be executed.
//!
//! # Supported Patterns
//! - **Backslash continuation**: Lines ending with `\` (not in quotes)
//! - **Unclosed quotes**: Double `"` quotes not closed, or single `'` quotes in shell contexts
//!   (Single quotes after letters are treated as apostrophes, not shell quotes)
//! - **Heredoc**: `<<DELIMITER` waiting for closing DELIMITER
//!
//! # Example
//! ```
//! use infraware_terminal::input::multiline::{is_incomplete, IncompleteReason};
//!
//! // Backslash continuation
//! assert!(matches!(
//!     is_incomplete("echo hello \\", None),
//!     Some(IncompleteReason::TrailingBackslash)
//! ));
//!
//! // Unclosed quote
//! assert!(matches!(
//!     is_incomplete("echo \"hello", None),
//!     Some(IncompleteReason::UnclosedDoubleQuote)
//! ));
//!
//! // Complete command
//! assert!(is_incomplete("echo hello", None).is_none());
//! ```

/// Reason why a command is incomplete and needs more input
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IncompleteReason {
    /// Line ends with backslash (continuation)
    TrailingBackslash,
    /// Single quote `'` not closed
    UnclosedSingleQuote,
    /// Double quote `"` not closed
    UnclosedDoubleQuote,
    /// Heredoc `<<DELIMITER` waiting for closing delimiter
    HeredocPending { delimiter: String },
}

impl std::fmt::Display for IncompleteReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IncompleteReason::TrailingBackslash => write!(f, "backslash continuation"),
            IncompleteReason::UnclosedSingleQuote => write!(f, "unclosed single quote"),
            IncompleteReason::UnclosedDoubleQuote => write!(f, "unclosed double quote"),
            IncompleteReason::HeredocPending { delimiter } => {
                write!(f, "heredoc waiting for '{}'", delimiter)
            }
        }
    }
}

/// Check if the input (possibly accumulated from multiple lines) is incomplete
///
/// # Arguments
/// * `input` - The current line of input
/// * `pending_heredoc` - If we're waiting for a heredoc delimiter from a previous line
///
/// # Returns
/// * `Some(IncompleteReason)` if the input is incomplete
/// * `None` if the input is complete and can be executed
pub fn is_incomplete(input: &str, pending_heredoc: Option<&str>) -> Option<IncompleteReason> {
    // If we're waiting for a heredoc delimiter, check if this line closes it
    if let Some(delimiter) = pending_heredoc {
        if input.trim() == delimiter {
            return None; // Heredoc is now complete
        }
        return Some(IncompleteReason::HeredocPending {
            delimiter: delimiter.to_string(),
        });
    }

    // Check for new heredoc start
    if let Some(delimiter) = extract_heredoc_delimiter(input) {
        return Some(IncompleteReason::HeredocPending { delimiter });
    }

    // Check for unclosed quotes
    if let Some(reason) = check_unclosed_quotes(input) {
        return Some(reason);
    }

    // Check for trailing backslash (line continuation)
    if has_trailing_backslash(input) {
        return Some(IncompleteReason::TrailingBackslash);
    }

    None
}

/// Check if accumulated lines form a complete command
///
/// This is used when we have multiple lines accumulated and need to check
/// if the full input is now complete.
///
/// # Arguments
/// * `lines` - All accumulated lines including the current one
///
/// # Returns
/// * `Some(IncompleteReason)` if still incomplete
/// * `None` if complete
pub fn is_multiline_complete(lines: &[String]) -> Option<IncompleteReason> {
    if lines.is_empty() {
        return None;
    }

    // Check for heredoc across all lines
    let mut heredoc_delimiter: Option<String> = None;

    for (i, line) in lines.iter().enumerate() {
        if let Some(ref delimiter) = heredoc_delimiter {
            // We're inside a heredoc, check if this line closes it
            if line.trim() == delimiter {
                heredoc_delimiter = None;
            }
        } else {
            // Check if this line starts a heredoc
            if let Some(delimiter) = extract_heredoc_delimiter(line) {
                heredoc_delimiter = Some(delimiter);
            }
        }

        // For the last line, also check quotes and backslash
        if i == lines.len() - 1 {
            // If we're still in a heredoc, we're incomplete
            if let Some(delimiter) = heredoc_delimiter {
                return Some(IncompleteReason::HeredocPending { delimiter });
            }

            // Check quotes across all joined lines
            let joined = join_lines_for_quote_check(lines);
            if let Some(reason) = check_unclosed_quotes(&joined) {
                return Some(reason);
            }

            // Check trailing backslash on last line only
            if has_trailing_backslash(line) {
                return Some(IncompleteReason::TrailingBackslash);
            }
        }
    }

    // Check if we ended inside a heredoc
    if let Some(delimiter) = heredoc_delimiter {
        return Some(IncompleteReason::HeredocPending { delimiter });
    }

    None
}

/// Join accumulated lines into a single command string
///
/// - Lines ending with `\` have the backslash removed and are joined with a space
/// - Other lines are joined with newlines (for heredoc content)
pub fn join_lines(lines: &[String]) -> String {
    if lines.is_empty() {
        return String::new();
    }

    // Pre-allocate with estimated capacity to avoid reallocations
    let estimated_size: usize = lines.iter().map(|s| s.len() + 1).sum();
    let mut result = String::with_capacity(estimated_size);

    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            // Check if previous line ended with backslash
            if let Some(prev) = lines.get(i - 1) {
                if has_trailing_backslash(prev) {
                    // Add space for backslash continuation (shell behavior)
                    result.push(' ');
                } else {
                    result.push('\n');
                }
            }
        }

        // Remove trailing backslash if present
        if has_trailing_backslash(line) {
            // Optimized: find backslash position and slice directly
            let trimmed = line.trim_end();
            // We know it ends with odd number of backslashes, find content before last one
            if let Some(pos) = trimmed.rfind(|c| c != '\\') {
                result.push_str(trimmed[..=pos].trim_end());
            }
            // If entire string is backslashes, push nothing
        } else {
            result.push_str(line);
        }
    }

    result
}

/// Check if line ends with a backslash (not escaped)
fn has_trailing_backslash(input: &str) -> bool {
    let trimmed = input.trim_end();
    if !trimmed.ends_with('\\') {
        return false;
    }

    // Count consecutive backslashes at the end
    let backslash_count = trimmed.chars().rev().take_while(|&c| c == '\\').count();

    // Odd number of backslashes = line continuation
    // Even number = escaped backslash
    backslash_count % 2 == 1
}

/// Check for unclosed quotes in the input
///
/// Single quotes are only treated as shell quotes when preceded by:
/// - Whitespace or start of input
/// - Shell metacharacters (|, &, ;, (, $, =, <, >)
///
/// This avoids false positives with apostrophes in natural language
/// (e.g., Italian "qual'è", English "it's").
fn check_unclosed_quotes(input: &str) -> Option<IncompleteReason> {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut prev_char: Option<char> = None;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\\' if in_double_quote => {
                // In double quotes, backslash escapes the next character
                chars.next();
                prev_char = Some(c);
                continue;
            }
            '\\' if !in_single_quote => {
                // Outside quotes, backslash escapes the next character
                chars.next();
                prev_char = Some(c);
                continue;
            }
            '\'' if !in_double_quote => {
                if in_single_quote {
                    // Always allow closing a single quote
                    in_single_quote = false;
                } else if is_quote_start_context(prev_char) {
                    // Only start single quote in shell-like contexts
                    in_single_quote = true;
                }
                // Otherwise: apostrophe in natural language, ignore
            }
            '"' if !in_single_quote => {
                in_double_quote = !in_double_quote;
            }
            _ => {}
        }
        prev_char = Some(c);
    }

    if in_single_quote {
        Some(IncompleteReason::UnclosedSingleQuote)
    } else if in_double_quote {
        Some(IncompleteReason::UnclosedDoubleQuote)
    } else {
        None
    }
}

/// Check if the previous character indicates a shell quote context
///
/// Returns true if prev_char is None (start of input) or is a character
/// that typically precedes a shell quote.
fn is_quote_start_context(prev_char: Option<char>) -> bool {
    match prev_char {
        None => true, // Start of input
        Some(c) => c.is_whitespace() || matches!(c, '|' | '&' | ';' | '(' | '$' | '=' | '<' | '>'),
    }
}

/// Extract heredoc delimiter from input line
///
/// Supports formats:
/// - `<<EOF`
/// - `<<'EOF'` (literal, no expansion)
/// - `<<"EOF"` (literal, no expansion)
/// - `<<-EOF` (strip leading tabs)
fn extract_heredoc_delimiter(input: &str) -> Option<String> {
    // Find << pattern
    let mut chars = input.chars().peekable();
    let mut found_heredoc = false;

    while let Some(c) = chars.next() {
        if c == '<' && chars.peek() == Some(&'<') {
            chars.next(); // consume second <
            found_heredoc = true;
            break;
        }
        // Skip if we're inside quotes (simplistic check)
        if c == '"' || c == '\'' {
            // Skip to closing quote
            let quote = c;
            while let Some(qc) = chars.next() {
                if qc == quote {
                    break;
                }
                if qc == '\\' {
                    chars.next();
                }
            }
        }
    }

    if !found_heredoc {
        return None;
    }

    // Skip optional - (for <<-)
    if chars.peek() == Some(&'-') {
        chars.next();
    }

    // Skip whitespace
    while chars.peek().is_some_and(|c| c.is_whitespace()) {
        chars.next();
    }

    // Extract delimiter
    let remaining: String = chars.collect();
    let delimiter = remaining.split_whitespace().next()?;

    // Remove surrounding quotes if present
    let delimiter = delimiter
        .trim_start_matches('\'')
        .trim_end_matches('\'')
        .trim_start_matches('"')
        .trim_end_matches('"');

    if delimiter.is_empty() {
        return None;
    }

    Some(delimiter.to_string())
}

/// Join lines for quote checking (preserves newlines)
fn join_lines_for_quote_check(lines: &[String]) -> String {
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Trailing Backslash Tests ==========

    #[test]
    fn test_trailing_backslash_simple() {
        assert!(has_trailing_backslash("echo hello \\"));
        assert!(has_trailing_backslash("echo \\"));
    }

    #[test]
    fn test_trailing_backslash_with_space() {
        // Trailing space after backslash - still counts (trimmed)
        assert!(has_trailing_backslash("echo hello \\  "));
    }

    #[test]
    fn test_no_trailing_backslash() {
        assert!(!has_trailing_backslash("echo hello"));
        assert!(!has_trailing_backslash("echo hello\\n"));
    }

    #[test]
    fn test_escaped_backslash() {
        // Double backslash = escaped, not continuation
        assert!(!has_trailing_backslash("echo hello\\\\"));
        // Triple backslash = escaped + continuation
        assert!(has_trailing_backslash("echo hello\\\\\\"));
    }

    // ========== Quote Detection Tests ==========

    #[test]
    fn test_unclosed_double_quote() {
        assert_eq!(
            check_unclosed_quotes("echo \"hello"),
            Some(IncompleteReason::UnclosedDoubleQuote)
        );
    }

    #[test]
    fn test_unclosed_single_quote() {
        assert_eq!(
            check_unclosed_quotes("echo 'hello"),
            Some(IncompleteReason::UnclosedSingleQuote)
        );
    }

    #[test]
    fn test_closed_quotes() {
        assert_eq!(check_unclosed_quotes("echo \"hello\""), None);
        assert_eq!(check_unclosed_quotes("echo 'hello'"), None);
        assert_eq!(check_unclosed_quotes("echo \"hello\" 'world'"), None);
    }

    #[test]
    fn test_escaped_quote() {
        // Escaped quote inside double quotes
        assert_eq!(check_unclosed_quotes("echo \"hello\\\"world\""), None);
    }

    #[test]
    fn test_single_quote_in_double() {
        // Single quote inside double quotes doesn't close
        assert_eq!(check_unclosed_quotes("echo \"it's fine\""), None);
    }

    #[test]
    fn test_double_quote_in_single() {
        // Double quote inside single quotes doesn't close
        assert_eq!(check_unclosed_quotes("echo 'say \"hello\"'"), None);
    }

    // ========== Apostrophe vs Quote Detection Tests ==========

    #[test]
    fn test_apostrophe_not_multiline() {
        // Italian apostrophes - should NOT trigger multiline
        assert_eq!(is_incomplete("qual'è il mio hostname?", None), None);
        assert_eq!(is_incomplete("l'applicazione non funziona", None), None);
        assert_eq!(is_incomplete("dov'è il file?", None), None);

        // English contractions - should NOT trigger multiline
        assert_eq!(is_incomplete("what's my hostname?", None), None);
        assert_eq!(is_incomplete("it's working", None), None);
        assert_eq!(is_incomplete("don't do that", None), None);
        assert_eq!(is_incomplete("I'm fine", None), None);
    }

    #[test]
    fn test_shell_quotes_still_work() {
        // Proper shell quotes should still trigger multiline when unclosed
        assert_eq!(
            is_incomplete("echo 'hello", None),
            Some(IncompleteReason::UnclosedSingleQuote)
        );

        // Closed shell quotes should work fine
        assert_eq!(is_incomplete("echo 'hello world'", None), None);
        assert_eq!(is_incomplete("echo 'hello' 'world'", None), None);

        // After pipe
        assert_eq!(
            is_incomplete("cat file | grep 'pattern", None),
            Some(IncompleteReason::UnclosedSingleQuote)
        );

        // After assignment
        assert_eq!(
            is_incomplete("VAR='value", None),
            Some(IncompleteReason::UnclosedSingleQuote)
        );

        // Quote at start of input
        assert_eq!(
            is_incomplete("'unclosed", None),
            Some(IncompleteReason::UnclosedSingleQuote)
        );
    }

    #[test]
    fn test_quote_start_context() {
        // These should be treated as quote starts
        assert!(is_quote_start_context(None)); // Start of input
        assert!(is_quote_start_context(Some(' '))); // After space
        assert!(is_quote_start_context(Some('\t'))); // After tab
        assert!(is_quote_start_context(Some('|'))); // After pipe
        assert!(is_quote_start_context(Some('&'))); // After ampersand
        assert!(is_quote_start_context(Some(';'))); // After semicolon
        assert!(is_quote_start_context(Some('('))); // After open paren
        assert!(is_quote_start_context(Some('$'))); // After dollar
        assert!(is_quote_start_context(Some('='))); // After equals

        // These should NOT be treated as quote starts (apostrophe context)
        assert!(!is_quote_start_context(Some('l'))); // qual'è
        assert!(!is_quote_start_context(Some('t'))); // it's
        assert!(!is_quote_start_context(Some('n'))); // don't
    }

    // ========== Heredoc Tests ==========

    #[test]
    fn test_heredoc_detection() {
        assert_eq!(
            extract_heredoc_delimiter("cat <<EOF"),
            Some("EOF".to_string())
        );
        assert_eq!(
            extract_heredoc_delimiter("cat <<'EOF'"),
            Some("EOF".to_string())
        );
        assert_eq!(
            extract_heredoc_delimiter("cat <<-EOF"),
            Some("EOF".to_string())
        );
        assert_eq!(
            extract_heredoc_delimiter("cat <<MARKER"),
            Some("MARKER".to_string())
        );
    }

    #[test]
    fn test_no_heredoc() {
        assert_eq!(extract_heredoc_delimiter("cat file.txt"), None);
        assert_eq!(extract_heredoc_delimiter("echo < input"), None);
        assert_eq!(extract_heredoc_delimiter("echo <<"), None);
    }

    #[test]
    fn test_heredoc_incomplete() {
        assert_eq!(
            is_incomplete("cat <<EOF", None),
            Some(IncompleteReason::HeredocPending {
                delimiter: "EOF".to_string()
            })
        );
    }

    #[test]
    fn test_heredoc_pending() {
        // Inside heredoc, waiting for delimiter
        assert_eq!(
            is_incomplete("some content", Some("EOF")),
            Some(IncompleteReason::HeredocPending {
                delimiter: "EOF".to_string()
            })
        );
    }

    #[test]
    fn test_heredoc_closed() {
        // Line matches delimiter, heredoc complete
        assert_eq!(is_incomplete("EOF", Some("EOF")), None);
    }

    // ========== is_incomplete Integration Tests ==========

    #[test]
    fn test_complete_commands() {
        assert!(is_incomplete("echo hello", None).is_none());
        assert!(is_incomplete("ls -la", None).is_none());
        assert!(is_incomplete("cat file.txt | grep pattern", None).is_none());
    }

    #[test]
    fn test_incomplete_backslash() {
        assert_eq!(
            is_incomplete("echo hello \\", None),
            Some(IncompleteReason::TrailingBackslash)
        );
    }

    #[test]
    fn test_incomplete_quotes() {
        assert_eq!(
            is_incomplete("echo \"hello", None),
            Some(IncompleteReason::UnclosedDoubleQuote)
        );
        assert_eq!(
            is_incomplete("echo 'hello", None),
            Some(IncompleteReason::UnclosedSingleQuote)
        );
    }

    // ========== join_lines Tests ==========

    #[test]
    fn test_join_backslash_continuation() {
        let lines = vec!["echo hello \\".to_string(), "world".to_string()];
        assert_eq!(join_lines(&lines), "echo hello world");
    }

    #[test]
    fn test_join_heredoc() {
        let lines = vec![
            "cat <<EOF".to_string(),
            "line1".to_string(),
            "line2".to_string(),
            "EOF".to_string(),
        ];
        assert_eq!(join_lines(&lines), "cat <<EOF\nline1\nline2\nEOF");
    }

    #[test]
    fn test_join_empty() {
        let lines: Vec<String> = vec![];
        assert_eq!(join_lines(&lines), "");
    }

    // ========== is_multiline_complete Tests ==========

    #[test]
    fn test_multiline_backslash_complete() {
        let lines = vec!["echo hello \\".to_string(), "world".to_string()];
        assert!(is_multiline_complete(&lines).is_none());
    }

    #[test]
    fn test_multiline_backslash_incomplete() {
        let lines = vec!["echo hello \\".to_string(), "world \\".to_string()];
        assert_eq!(
            is_multiline_complete(&lines),
            Some(IncompleteReason::TrailingBackslash)
        );
    }

    #[test]
    fn test_multiline_heredoc_complete() {
        let lines = vec![
            "cat <<EOF".to_string(),
            "content".to_string(),
            "EOF".to_string(),
        ];
        assert!(is_multiline_complete(&lines).is_none());
    }

    #[test]
    fn test_multiline_heredoc_incomplete() {
        let lines = vec!["cat <<EOF".to_string(), "content".to_string()];
        assert_eq!(
            is_multiline_complete(&lines),
            Some(IncompleteReason::HeredocPending {
                delimiter: "EOF".to_string()
            })
        );
    }

    #[test]
    fn test_multiline_quotes_complete() {
        let lines = vec!["echo \"hello".to_string(), "world\"".to_string()];
        assert!(is_multiline_complete(&lines).is_none());
    }

    #[test]
    fn test_multiline_quotes_incomplete() {
        let lines = vec!["echo \"hello".to_string(), "world".to_string()];
        assert_eq!(
            is_multiline_complete(&lines),
            Some(IncompleteReason::UnclosedDoubleQuote)
        );
    }

    // ========== Display Tests ==========

    #[test]
    fn test_incomplete_reason_display() {
        assert_eq!(
            format!("{}", IncompleteReason::TrailingBackslash),
            "backslash continuation"
        );
        assert_eq!(
            format!("{}", IncompleteReason::UnclosedSingleQuote),
            "unclosed single quote"
        );
        assert_eq!(
            format!("{}", IncompleteReason::UnclosedDoubleQuote),
            "unclosed double quote"
        );
        assert_eq!(
            format!(
                "{}",
                IncompleteReason::HeredocPending {
                    delimiter: "EOF".to_string()
                }
            ),
            "heredoc waiting for 'EOF'"
        );
    }
}
