/// History expansion handler
///
/// This module provides support for bash-style history expansions like `!!`, `!$`, `!^`, `!*`.
/// These expansions are commonly used in shells to reference previous commands.
///
/// # Supported Expansions
///
/// - `!!` - Entire previous command
/// - `!$` - Last argument of previous command
/// - `!^` - First argument of previous command
/// - `!*` - All arguments of previous command
///
/// # Example
///
/// ```text
/// > ls -la /tmp
/// [output]
///
/// > sudo !!
/// # Expands to: sudo ls -la /tmp
/// ```
use super::handler::InputHandler;
use super::parser::CommandParser;
use super::InputType;
use std::sync::{Arc, RwLock};

/// Handler for bash-style history expansions
#[derive(Debug, Clone)]
pub struct HistoryExpansionHandler {
    /// Reference to command history (thread-safe)
    history: Option<Arc<RwLock<Vec<String>>>>,
}

impl HistoryExpansionHandler {
    /// Create a new history expansion handler without history
    pub const fn new() -> Self {
        Self { history: None }
    }

    /// Create a handler with access to command history
    pub const fn with_history(history: Arc<RwLock<Vec<String>>>) -> Self {
        Self {
            history: Some(history),
        }
    }

    /// Check if input contains history expansion patterns
    fn has_history_expansion(input: &str) -> bool {
        input.contains("!!") || input.contains("!$") || input.contains("!^") || input.contains("!*")
    }

    /// Get the last command from history
    ///
    /// Returns the second-to-last command because by the time we classify input,
    /// the current input has already been added to history by submit_input().
    /// For example, if history is ["ls", "pwd", "!!"], we want to return "pwd",
    /// not "!!" itself.
    fn get_last_command(&self) -> Option<String> {
        let history = self.history.as_ref()?;
        let guard = match history.read() {
            Ok(g) => g,
            Err(poisoned) => poisoned.into_inner(),
        };

        // Get second-to-last command (skip the current input which is last)
        if guard.len() < 2 {
            return None;
        }
        guard.get(guard.len() - 2).cloned()
    }

    /// Parse a command into command and args
    fn parse_command_parts(cmd: &str) -> (String, Vec<String>) {
        if let Ok((command, args)) = CommandParser::parse(cmd) {
            (command, args)
        } else {
            // Fallback: split by whitespace
            let mut parts: Vec<String> = cmd.split_whitespace().map(String::from).collect();
            if parts.is_empty() {
                (String::new(), vec![])
            } else {
                let command = parts.remove(0);
                (command, parts)
            }
        }
    }

    /// Expand `!!` to the entire previous command
    fn expand_bang_bang(&self, input: &str) -> Option<String> {
        let last_cmd = self.get_last_command()?;
        Some(input.replace("!!", &last_cmd))
    }

    /// Expand `!$` to the last argument of previous command
    ///
    /// Bash-compatible behavior: If the previous command has no arguments,
    /// `!$` expands to the command itself.
    ///
    /// Examples:
    /// - `ls -la /tmp` → `!$` = `/tmp`
    /// - `pwd` → `!$` = `pwd` (command itself when no args)
    fn expand_bang_dollar(&self, input: &str) -> Option<String> {
        let last_cmd = self.get_last_command()?;
        let (command, args) = Self::parse_command_parts(&last_cmd);

        // Bash behavior: if no args, !$ expands to command itself
        let last_arg = args.last().unwrap_or(&command);

        Some(input.replace("!$", last_arg))
    }

    /// Expand `!^` to the first argument of previous command
    fn expand_bang_caret(&self, input: &str) -> Option<String> {
        let last_cmd = self.get_last_command()?;
        let (_, args) = Self::parse_command_parts(&last_cmd);
        let first_arg = args.first()?;
        Some(input.replace("!^", first_arg))
    }

    /// Expand `!*` to all arguments of previous command
    fn expand_bang_star(&self, input: &str) -> Option<String> {
        let last_cmd = self.get_last_command()?;
        let (_, args) = Self::parse_command_parts(&last_cmd);
        if args.is_empty() {
            return None;
        }
        let all_args = args.join(" ");
        Some(input.replace("!*", &all_args))
    }

    /// Expand all history patterns in the input
    fn expand_history(&self, input: &str) -> Option<String> {
        let mut expanded = input.to_string();

        // Expand in order: !!, !$, !^, !* (!! first because it's most common)
        if expanded.contains("!!") {
            expanded = self.expand_bang_bang(&expanded)?;
        }
        if expanded.contains("!$") {
            expanded = self.expand_bang_dollar(&expanded)?;
        }
        if expanded.contains("!^") {
            expanded = self.expand_bang_caret(&expanded)?;
        }
        if expanded.contains("!*") {
            expanded = self.expand_bang_star(&expanded)?;
        }

        Some(expanded)
    }
}

impl Default for HistoryExpansionHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler for HistoryExpansionHandler {
    fn handle(&self, input: &str, ctx: &super::handler::ClassifierContext) -> Option<InputType> {
        // Skip if no history available
        self.history.as_ref()?;

        // Skip if no history expansion patterns
        if !Self::has_history_expansion(input) {
            return None;
        }

        // Try to expand history
        let expanded = self.expand_history(input)?;

        // Check if the first word is an alias and expand it
        // This handles cases like: ll -> !! where ll is an alias
        let expanded = {
            use crate::input::discovery::CommandCache;

            if let Some(first_word) = expanded.split_whitespace().next() {
                if let Some(alias_expansion) = CommandCache::expand_alias(first_word) {
                    // Get the rest of the arguments (everything after first word)
                    let first_word_len = first_word.len();
                    let rest = if first_word_len < expanded.len() {
                        expanded[first_word_len..].trim_start()
                    } else {
                        ""
                    };

                    // Construct expanded input: alias_expansion + rest
                    if rest.is_empty() {
                        alias_expansion
                    } else {
                        format!("{alias_expansion} {rest}")
                    }
                } else {
                    expanded
                }
            } else {
                expanded
            }
        };

        // Parse the expanded command
        match CommandParser::parse(&expanded) {
            Ok((command, args)) => {
                // Check if expanded command has shell operators
                let original_input = if ctx.patterns.has_shell_operators(&expanded) {
                    Some(expanded.clone())
                } else {
                    None
                };

                Some(InputType::Command {
                    command,
                    args,
                    original_input,
                })
            }
            Err(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::handler::ClassifierContext;

    fn create_handler_with_history(history: Vec<&str>) -> HistoryExpansionHandler {
        let history_vec: Vec<String> = history.iter().map(|s| (*s).to_string()).collect();
        let history_arc = Arc::new(RwLock::new(history_vec));
        HistoryExpansionHandler::with_history(history_arc)
    }

    fn create_context() -> ClassifierContext {
        ClassifierContext::new()
    }

    #[test]
    fn test_expand_bang_bang() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["ls -la", "!!"]);
        let ctx = create_context();
        let result = handler.handle("!!", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "ls");
                assert_eq!(args, vec!["-la"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_expand_bang_bang_with_sudo() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["apt update", "sudo !!"]);
        let ctx = create_context();
        let result = handler.handle("sudo !!", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "sudo");
                assert_eq!(args, vec!["apt", "update"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_expand_bang_dollar() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["cat file.txt", "vim !$"]);
        let ctx = create_context();
        let result = handler.handle("vim !$", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "vim");
                assert_eq!(args, vec!["file.txt"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_expand_bang_caret() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["cat file1.txt file2.txt", "vim !^"]);
        let ctx = create_context();
        let result = handler.handle("vim !^", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "vim");
                assert_eq!(args, vec!["file1.txt"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_expand_bang_star() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["cat file1.txt file2.txt", "echo !*"]);
        let ctx = create_context();
        let result = handler.handle("echo !*", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "echo");
                assert_eq!(args, vec!["file1.txt", "file2.txt"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_no_history() {
        let handler = HistoryExpansionHandler::new();
        let ctx = create_context();
        let result = handler.handle("!!", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_history() {
        let handler = create_handler_with_history(vec![]);
        let ctx = create_context();
        let result = handler.handle("!!", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_no_expansion_pattern() {
        let handler = create_handler_with_history(vec!["ls -la"]);
        let ctx = create_context();
        let result = handler.handle("echo hello", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_command_without_args() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["pwd", "!!"]);
        let ctx = create_context();
        let result = handler.handle("!!", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "pwd");
                assert!(args.is_empty());
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_bang_dollar_no_args() {
        // Bash behavior: !$ expands to command itself when no args
        let handler = create_handler_with_history(vec!["pwd", "echo !$"]);
        let ctx = create_context();
        let result = handler.handle("echo !$", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "echo");
                assert_eq!(args, vec!["pwd"]); // !$ → pwd (command itself)
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_handler_name() {
        let _handler = HistoryExpansionHandler::new();
    }

    #[test]
    fn test_preserve_shell_operators() {
        // Simulate real flow: history has previous command + current input
        let handler = create_handler_with_history(vec!["echo hello", "!! | grep hello"]);
        let ctx = create_context();
        let result = handler.handle("!! | grep hello", &ctx).unwrap();

        match result {
            InputType::Command {
                command,
                original_input,
                ..
            } => {
                assert_eq!(command, "echo");
                assert!(original_input.is_some());
                assert_eq!(original_input.unwrap(), "echo hello | grep hello");
            }
            _ => panic!("Expected Command with original_input"),
        }
    }

    #[test]
    fn test_bang_star_no_args() {
        // !* should fail when no args (Bash behavior)
        let handler = create_handler_with_history(vec!["pwd", "echo !*"]);
        let ctx = create_context();
        let result = handler.handle("echo !*", &ctx);
        // Should fail because pwd has no args and !* requires args
        assert!(result.is_none());
    }

    #[test]
    fn test_bang_caret_no_args() {
        // !^ should fail when no args (Bash behavior)
        let handler = create_handler_with_history(vec!["pwd", "echo !^"]);
        let ctx = create_context();
        let result = handler.handle("echo !^", &ctx);
        // Should fail because pwd has no args and !^ requires args
        assert!(result.is_none());
    }

    #[test]
    fn test_multiple_expansions() {
        // Multiple expansions in same input
        let handler = create_handler_with_history(vec!["echo hello world", "printf '%s %s' !^ !$"]);
        let ctx = create_context();
        let result = handler.handle("printf '%s %s' !^ !$", &ctx).unwrap();

        match result {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "printf");
                // !^ = hello (first arg), !$ = world (last arg)
                // Note: shell-words parser removes outer quotes from '%s %s'
                assert_eq!(args, vec!["%s %s", "hello", "world"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_history_with_only_current_input() {
        // Edge case: history has only current input (no previous command)
        let handler = create_handler_with_history(vec!["!!"]);
        let ctx = create_context();
        let result = handler.handle("!!", &ctx);
        // Should fail because there's no previous command (need at least 2 entries)
        assert!(result.is_none());
    }
}
