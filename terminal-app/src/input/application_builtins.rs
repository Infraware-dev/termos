//! Application-specific builtin commands
//!
//! This module defines commands that are built into the terminal application itself,
//! as opposed to shell builtins or external system commands.
//!
//! These commands are handled directly by the application and should be recognized
//! early in the classification chain to avoid being misclassified as natural language.

/// List of application builtin commands
///
/// These are commands implemented directly in the terminal application:
/// - `cd`: Change working directory (must be handled by parent process)
/// - `clear`: Clear the terminal output buffer
/// - `exit`: Exit the terminal application
/// - `jobs`: List background jobs
/// - `reload-aliases`: Reload alias definitions from system and user config files
/// - `reload-commands`: Clear the command cache (useful after installing new commands)
/// - `auth-status`: Check backend authentication status
///
/// # Example
/// ```
/// use infraware_terminal::input::application_builtins::APPLICATION_BUILTINS;
///
/// assert!(APPLICATION_BUILTINS.contains(&"cd"));
/// assert!(APPLICATION_BUILTINS.contains(&"clear"));
/// assert!(APPLICATION_BUILTINS.contains(&"exit"));
/// assert!(APPLICATION_BUILTINS.contains(&"jobs"));
/// assert!(APPLICATION_BUILTINS.contains(&"reload-aliases"));
/// assert!(APPLICATION_BUILTINS.contains(&"reload-commands"));
/// assert!(APPLICATION_BUILTINS.contains(&"auth-status"));
/// ```
pub const APPLICATION_BUILTINS: &[&str] = &[
    "cd",
    "clear",
    "exit",
    "jobs",
    "reload-aliases",
    "reload-commands",
    "auth-status",
];

/// Check if a command is an application builtin
///
/// # Arguments
/// * `command` - The command name to check
///
/// # Returns
/// `true` if the command is an application builtin, `false` otherwise
///
/// # Example
/// ```
/// use infraware_terminal::input::application_builtins::is_application_builtin;
///
/// assert!(is_application_builtin("clear"));
/// assert!(is_application_builtin("reload-aliases"));
/// assert!(!is_application_builtin("docker"));
/// ```
pub fn is_application_builtin(command: &str) -> bool {
    APPLICATION_BUILTINS.contains(&command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clear_is_builtin() {
        assert!(is_application_builtin("clear"));
    }

    #[test]
    fn test_reload_aliases_is_builtin() {
        assert!(is_application_builtin("reload-aliases"));
    }

    #[test]
    fn test_reload_commands_is_builtin() {
        assert!(is_application_builtin("reload-commands"));
    }

    #[test]
    fn test_not_builtin() {
        assert!(!is_application_builtin("docker"));
        assert!(!is_application_builtin("ls"));
        assert!(!is_application_builtin("unknown"));
    }

    #[test]
    fn test_case_sensitive() {
        assert!(!is_application_builtin("Clear"));
        assert!(!is_application_builtin("CLEAR"));
        assert!(!is_application_builtin("Reload-Aliases"));
    }

    #[test]
    fn test_auth_status_is_builtin() {
        assert!(is_application_builtin("auth-status"));
    }

    #[test]
    fn test_exit_is_builtin() {
        assert!(is_application_builtin("exit"));
    }

    #[test]
    fn test_builtin_list_count() {
        // Verify we have exactly 7 application builtins
        assert_eq!(APPLICATION_BUILTINS.len(), 7);
        // Verify they are the expected ones
        assert!(APPLICATION_BUILTINS.contains(&"cd"));
        assert!(APPLICATION_BUILTINS.contains(&"clear"));
        assert!(APPLICATION_BUILTINS.contains(&"exit"));
        assert!(APPLICATION_BUILTINS.contains(&"jobs"));
        assert!(APPLICATION_BUILTINS.contains(&"reload-aliases"));
        assert!(APPLICATION_BUILTINS.contains(&"reload-commands"));
        assert!(APPLICATION_BUILTINS.contains(&"auth-status"));
    }

    #[test]
    fn test_jobs_is_builtin() {
        assert!(is_application_builtin("jobs"));
    }

    #[test]
    fn test_cd_is_builtin() {
        assert!(is_application_builtin("cd"));
    }
}
