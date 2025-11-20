/// Shell builtin command detection
///
/// This module provides detection for shell builtin commands that may not exist in PATH
/// but are valid shell commands. This includes punctuation commands like `.`, `:`, `[`, `[[`.
use super::handler::InputHandler;
use super::parser::CommandParser;
use super::InputType;

/// Handler for shell builtin commands
///
/// Shell builtins are commands built into the shell itself (bash, sh, zsh, etc.)
/// rather than separate executable programs. Many don't appear in PATH, so they
/// can't be detected via `which` or PATH verification.
///
/// Examples of shell builtins:
/// - `.` (dot) - source a file (POSIX)
/// - `:` (colon) - no-op command (POSIX)
/// - `[` (single bracket) - test command (POSIX, also exists as /usr/bin/[)
/// - `[[` (double bracket) - extended test (bash/zsh only)
/// - `source` - source a file (bash/zsh, equivalent to `.`)
/// - `true`, `false` - boolean commands (POSIX)
/// - `eval`, `exec`, `export`, `set`, `unset` - common builtins
///
/// This handler recognizes these builtins by name without PATH verification.
#[derive(Debug, Clone)]
pub struct ShellBuiltinHandler {
    /// List of known shell builtins
    builtins: Vec<&'static str>,
}

impl ShellBuiltinHandler {
    /// Create a new handler with default shell builtins
    ///
    /// Includes builtins from POSIX sh, bash, and zsh that are commonly used
    /// and may not be in PATH.
    pub fn new() -> Self {
        Self::with_builtins(Self::default_builtins())
    }

    /// Create a handler with a custom list of builtins
    #[allow(dead_code)]
    pub fn with_builtins(builtins: Vec<&'static str>) -> Self {
        Self { builtins }
    }

    /// Default list of shell builtins to recognize
    ///
    /// This list focuses on builtins that:
    /// 1. Are not reliably in PATH
    /// 2. Use punctuation or are commonly used
    /// 3. Should be executed through a shell
    pub fn default_builtins() -> Vec<&'static str> {
        vec![
            // Source commands (POSIX and bash)
            ".",      // POSIX: source a file
            "source", // Bash/Zsh: equivalent to .
            // No-op and boolean commands (POSIX)
            ":",     // No-op command (always succeeds)
            "true",  // Always succeeds (exit 0)
            "false", // Always fails (exit 1)
            // Test commands (POSIX and bash)
            "[",    // POSIX test command (also /usr/bin/[)
            "[[",   // Bash/Zsh extended test
            "test", // POSIX test command (also /usr/bin/test)
            // Variable and environment commands
            "export",   // Export environment variables
            "unset",    // Unset variables
            "set",      // Set shell options/positional parameters
            "declare",  // Bash: declare variables with attributes
            "local",    // Bash: declare local variables in functions
            "readonly", // Mark variables as read-only
            "typeset",  // Ksh/Zsh: declare variables
            // Evaluation and execution
            "eval",   // Evaluate arguments as shell commands
            "exec",   // Replace shell with command
            "return", // Return from function
            "exit",   // Exit shell
            // Flow control
            "break",    // Break out of loop
            "continue", // Continue to next iteration
            "shift",    // Shift positional parameters
            // Alias management
            "alias",   // Define command aliases
            "unalias", // Remove aliases
            // I/O and read commands
            "read",   // Read input (may be interactive)
            "echo",   // Print to stdout (builtin in bash)
            "printf", // Formatted print (builtin in bash)
            // Job control
            "jobs", // List jobs
            "fg",   // Foreground job
            "bg",   // Background job
            "wait", // Wait for job completion
            // Directory stack
            "pushd", // Push directory onto stack
            "popd",  // Pop directory from stack
            "dirs",  // Display directory stack
            // Builtin command management
            "builtin", // Run builtin command
            "command", // Run command bypassing functions
            "enable",  // Enable/disable builtins
            // Miscellaneous
            "type",   // Display command type
            "hash",   // Remember/display command locations
            "times",  // Display process times
            "umask",  // Set file creation mask
            "ulimit", // Set resource limits
        ]
    }

    /// Check if a word is a recognized shell builtin
    fn is_builtin(&self, word: &str) -> bool {
        self.builtins.contains(&word)
    }
}

impl Default for ShellBuiltinHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl InputHandler for ShellBuiltinHandler {
    fn handle(&self, input: &str) -> Option<InputType> {
        // Extract first word
        let first_word = input.split_whitespace().next()?;

        // Check if it's a shell builtin
        if self.is_builtin(first_word) {
            // Parse as command using the standard parser
            match CommandParser::parse(input) {
                Ok((command, args)) => Some(InputType::Command {
                    command,
                    args,
                    // Preserve original input for shell operators
                    original_input: if input.contains('|')
                        || input.contains('>')
                        || input.contains('<')
                        || input.contains('&')
                        || input.contains(';')
                    {
                        Some(input.to_string())
                    } else {
                        None
                    },
                }),
                Err(_) => None,
            }
        } else {
            None
        }
    }

    fn name(&self) -> &str {
        "ShellBuiltinHandler"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recognize_dot_builtin() {
        let handler = ShellBuiltinHandler::new();

        // Test . (source) command
        let result = handler.handle(".");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, .. } => {
                assert_eq!(command, ".");
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_dot_with_file() {
        let handler = ShellBuiltinHandler::new();

        // Test . ~/.bashrc
        let result = handler.handle(". ~/.bashrc");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, ".");
                assert_eq!(args, vec!["~/.bashrc"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_colon_builtin() {
        let handler = ShellBuiltinHandler::new();

        // Test : (no-op) command
        let result = handler.handle(":");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, .. } => {
                assert_eq!(command, ":");
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_single_bracket() {
        let handler = ShellBuiltinHandler::new();

        // Test [ command
        let result = handler.handle("[ -f file.txt ]");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "[");
                assert_eq!(args, vec!["-f", "file.txt", "]"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_double_bracket() {
        let handler = ShellBuiltinHandler::new();

        // Test [[ command
        let result = handler.handle("[[ -f file.txt ]]");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "[[");
                assert_eq!(args, vec!["-f", "file.txt", "]]"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_test_command() {
        let handler = ShellBuiltinHandler::new();

        let result = handler.handle("test -f file.txt");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "test");
                assert_eq!(args, vec!["-f", "file.txt"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_true_false() {
        let handler = ShellBuiltinHandler::new();

        let true_result = handler.handle("true");
        assert!(true_result.is_some());

        let false_result = handler.handle("false");
        assert!(false_result.is_some());
    }

    #[test]
    fn test_recognize_source() {
        let handler = ShellBuiltinHandler::new();

        let result = handler.handle("source ~/.bashrc");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, args, .. } => {
                assert_eq!(command, "source");
                assert_eq!(args, vec!["~/.bashrc"]);
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_recognize_export() {
        let handler = ShellBuiltinHandler::new();

        let result = handler.handle("export PATH=/usr/bin");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { command, .. } => {
                assert_eq!(command, "export");
            }
            _ => panic!("Expected Command"),
        }
    }

    #[test]
    fn test_not_builtin() {
        let handler = ShellBuiltinHandler::new();

        // Non-builtin command should return None
        let result = handler.handle("ls -la");
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_input() {
        let handler = ShellBuiltinHandler::new();

        let result = handler.handle("");
        assert!(result.is_none());
    }

    #[test]
    fn test_handler_name() {
        let handler = ShellBuiltinHandler::new();
        assert_eq!(handler.name(), "ShellBuiltinHandler");
    }

    #[test]
    fn test_is_builtin_check() {
        let handler = ShellBuiltinHandler::new();

        assert!(handler.is_builtin("."));
        assert!(handler.is_builtin(":"));
        assert!(handler.is_builtin("["));
        assert!(handler.is_builtin("[["));
        assert!(handler.is_builtin("source"));
        assert!(handler.is_builtin("export"));
        assert!(!handler.is_builtin("ls"));
        assert!(!handler.is_builtin("nonexistent"));
    }

    #[test]
    fn test_custom_builtins() {
        let handler = ShellBuiltinHandler::with_builtins(vec!["custom", "builtin"]);

        let result = handler.handle("custom arg");
        assert!(result.is_some());

        let result2 = handler.handle("export PATH=/bin");
        assert!(result2.is_none()); // export not in custom list
    }

    #[test]
    fn test_preserve_shell_operators() {
        let handler = ShellBuiltinHandler::new();

        // Test with pipe
        let result = handler.handle(": | grep test");
        assert!(result.is_some());
        match result.unwrap() {
            InputType::Command { original_input, .. } => {
                assert!(original_input.is_some());
                assert_eq!(original_input.unwrap(), ": | grep test");
            }
            _ => panic!("Expected Command with original_input"),
        }
    }
}
