/// Shell builtin command detection
///
/// This module provides detection for shell builtin commands that may not exist in PATH
/// but are valid shell commands. This includes punctuation commands like `.`, `:`, `[`, `[[`.
///
/// # Platform Support
///
/// Most shell builtins are POSIX/bash-specific for Unix-like systems (Linux, macOS).
/// On Windows, builtins are executed via `cmd.exe` which has different built-in commands.
/// Unix builtins like `.`, `source`, `[[` are not available on Windows.
///
/// # Execution Model
///
/// Shell builtins are executed through the system shell (`sh -c` on Unix, `cmd /C` on Windows)
/// rather than as direct executables, since they're built into the shell itself.
use super::handler::InputHandler;
use super::parser::CommandParser;
use super::InputType;

/// Metadata for a shell builtin command
#[derive(Debug, Clone, Copy)]
pub struct ShellBuiltinInfo {
    /// The builtin command name
    pub name: &'static str,
    /// Whether this MUST be executed through a shell (true) or can also exist as standalone binary (false)
    pub requires_shell: bool,
    /// Whether this is Unix-only (not available on Windows)
    /// Note: This field is used in conditional compilation for Windows targets
    #[allow(dead_code)] // Used for platform-specific conditional execution
    pub unix_only: bool,
}

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
    pub const fn with_builtins(builtins: Vec<&'static str>) -> Self {
        Self { builtins }
    }

    /// Get metadata for all shell builtins (single source of truth)
    ///
    /// This is the authoritative list of shell builtins with platform and execution metadata.
    /// Other parts of the codebase should query this list rather than maintaining their own.
    pub const fn builtin_info() -> &'static [ShellBuiltinInfo] {
        &[
            // Source commands (POSIX and bash) - MUST use shell, Unix-only
            ShellBuiltinInfo {
                name: ".",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "source",
                requires_shell: true,
                unix_only: true,
            },
            // No-op command - MUST use shell, Unix-only
            ShellBuiltinInfo {
                name: ":",
                requires_shell: true,
                unix_only: true,
            },
            // Boolean commands - can exist as /usr/bin/true and /usr/bin/false but prefer shell
            ShellBuiltinInfo {
                name: "true",
                requires_shell: false,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "false",
                requires_shell: false,
                unix_only: true,
            },
            // Test commands
            ShellBuiltinInfo {
                name: "[",
                requires_shell: false,
                unix_only: true,
            }, // Also /usr/bin/[
            ShellBuiltinInfo {
                name: "[[",
                requires_shell: true,
                unix_only: true,
            }, // Bash/Zsh only
            ShellBuiltinInfo {
                name: "test",
                requires_shell: false,
                unix_only: true,
            }, // Also /usr/bin/test
            // Variable and environment commands - MUST use shell
            ShellBuiltinInfo {
                name: "export",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "unset",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "set",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "declare",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "local",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "readonly",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "typeset",
                requires_shell: true,
                unix_only: true,
            },
            // Evaluation and execution - MUST use shell
            ShellBuiltinInfo {
                name: "eval",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "exec",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "return",
                requires_shell: true,
                unix_only: true,
            },
            // Note: "exit" is handled as application builtin (not shell builtin)
            // Flow control - MUST use shell
            ShellBuiltinInfo {
                name: "break",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "continue",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "shift",
                requires_shell: true,
                unix_only: true,
            },
            // Alias management - MUST use shell
            ShellBuiltinInfo {
                name: "alias",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "unalias",
                requires_shell: true,
                unix_only: true,
            },
            // I/O commands - prefer shell but can exist as standalone
            ShellBuiltinInfo {
                name: "read",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "echo",
                requires_shell: false,
                unix_only: false,
            }, // Cross-platform
            ShellBuiltinInfo {
                name: "printf",
                requires_shell: false,
                unix_only: true,
            },
            // Job control - MUST use shell
            ShellBuiltinInfo {
                name: "jobs",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "fg",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "bg",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "wait",
                requires_shell: true,
                unix_only: true,
            },
            // Directory stack - MUST use shell
            ShellBuiltinInfo {
                name: "pushd",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "popd",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "dirs",
                requires_shell: true,
                unix_only: true,
            },
            // Builtin management - MUST use shell
            ShellBuiltinInfo {
                name: "builtin",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "command",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "enable",
                requires_shell: true,
                unix_only: true,
            },
            // System info - MUST use shell
            ShellBuiltinInfo {
                name: "type",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "hash",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "times",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "umask",
                requires_shell: true,
                unix_only: true,
            },
            ShellBuiltinInfo {
                name: "ulimit",
                requires_shell: true,
                unix_only: true,
            },
        ]
    }

    /// Default list of shell builtin names for classification
    ///
    /// Extracts just the names from builtin_info() for use in the handler.
    pub fn default_builtins() -> Vec<&'static str> {
        Self::builtin_info().iter().map(|info| info.name).collect()
    }

    /// Check if a command MUST be executed through a shell
    ///
    /// Returns true for builtins that don't exist as standalone executables
    /// and must be run via `sh -c` (or `cmd /C` on Windows).
    pub fn requires_shell_execution(cmd: &str) -> bool {
        Self::builtin_info()
            .iter()
            .find(|info| info.name == cmd)
            .is_some_and(|info| info.requires_shell)
    }

    /// Check if a command is Unix-only and not available on Windows
    /// Note: This function is used in conditional compilation for Windows targets
    #[allow(dead_code)] // Used for platform-specific conditional execution
    pub fn is_unix_only(cmd: &str) -> bool {
        Self::builtin_info()
            .iter()
            .find(|info| info.name == cmd)
            .is_some_and(|info| info.unix_only)
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
    fn handle(&self, input: &str, ctx: &super::handler::ClassifierContext) -> Option<InputType> {
        // Extract first word
        let first_word = input.split_whitespace().next()?;

        // Check if it's a shell builtin
        if self.is_builtin(first_word) {
            // Parse as command using the standard parser
            match CommandParser::parse(input) {
                Ok((command, args)) => {
                    // Preserve original input for shell operators
                    let original_input = if ctx.patterns.has_shell_operators(input) {
                        Some(input.to_string())
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
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::handler::ClassifierContext;

    fn create_context() -> ClassifierContext {
        ClassifierContext::new()
    }

    #[test]
    fn test_recognize_dot_builtin() {
        let handler = ShellBuiltinHandler::new();
        let ctx = create_context();

        // Test . (source) command
        let result = handler.handle(".", &ctx);
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
        let ctx = create_context();

        // Test . ~/.bashrc
        let result = handler.handle(". ~/.bashrc", &ctx);
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
        let ctx = create_context();

        // Test : (no-op) command
        let result = handler.handle(":", &ctx);
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
        let ctx = create_context();

        // Test [ command
        let result = handler.handle("[ -f file.txt ]", &ctx);
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
        let ctx = create_context();

        // Test [[ command
        let result = handler.handle("[[ -f file.txt ]]", &ctx);
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
        let ctx = create_context();

        let result = handler.handle("test -f file.txt", &ctx);
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
        let ctx = create_context();

        let true_result = handler.handle("true", &ctx);
        assert!(true_result.is_some());

        let false_result = handler.handle("false", &ctx);
        assert!(false_result.is_some());
    }

    #[test]
    fn test_recognize_source() {
        let handler = ShellBuiltinHandler::new();
        let ctx = create_context();

        let result = handler.handle("source ~/.bashrc", &ctx);
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
        let ctx = create_context();

        let result = handler.handle("export PATH=/usr/bin", &ctx);
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
        let ctx = create_context();

        // Non-builtin command should return None
        let result = handler.handle("ls -la", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_empty_input() {
        let handler = ShellBuiltinHandler::new();
        let ctx = create_context();

        let result = handler.handle("", &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_handler_name() {
        let _handler = ShellBuiltinHandler::new();
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
        let ctx = create_context();

        let result = handler.handle("custom arg", &ctx);
        assert!(result.is_some());

        let result2 = handler.handle("export PATH=/bin", &ctx);
        assert!(result2.is_none()); // export not in custom list
    }

    #[test]
    fn test_preserve_shell_operators() {
        let handler = ShellBuiltinHandler::new();
        let ctx = create_context();

        // Test with pipe
        let result = handler.handle(": | grep test", &ctx);
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
