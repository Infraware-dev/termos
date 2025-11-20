/// Centralized message formatting for consistent output styling
use super::ansi::AnsiColor;

/// Message formatter for creating consistently styled output
pub struct MessageFormatter;

impl MessageFormatter {
    /// Format an error message
    pub fn error(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Red.colorize("✗"), message.as_ref())
    }

    /// Format a success message
    pub fn success(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Green.colorize("✓"), message.as_ref())
    }

    /// Format an info message
    pub fn info(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Blue.colorize("ℹ"), message.as_ref())
    }

    /// Format a command prompt/echo
    pub fn command(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Cyan.colorize("❯"), message.as_ref())
    }

    /// Format a suggestion/hint
    pub fn suggestion(message: impl AsRef<str>) -> String {
        format!("  {} {}", AnsiColor::Yellow.colorize("→"), message.as_ref())
    }

    /// Format command not found error
    pub fn command_not_found(cmd: impl AsRef<str>) -> String {
        Self::error(format!("Command '{}' not found", cmd.as_ref()))
    }

    /// Format command exit code error
    pub fn command_failed(exit_code: i32) -> String {
        Self::error(format!("Command exited with code {}", exit_code))
    }

    /// Format execution error
    pub fn execution_error(error: impl AsRef<str>) -> String {
        Self::error(format!("Error executing command: {}", error.as_ref()))
    }

    /// Format install suggestion
    pub fn install_suggestion(available: bool) -> String {
        if available {
            Self::suggestion("Would you like to install it? (Feature coming in next version)")
        } else {
            Self::info("Package manager not available for auto-install")
        }
    }

    /// Format welcome banner header line
    pub fn banner_line(content: impl AsRef<str>) -> String {
        AnsiColor::Cyan.colorize(content.as_ref())
    }

    /// Format welcome banner hint/footer text
    pub fn banner_hint(content: impl AsRef<str>) -> String {
        AnsiColor::BrightBlack.colorize(content.as_ref())
    }

    /// Format stderr output line (for failed commands)
    pub fn stderr_error(line: impl AsRef<str>) -> String {
        AnsiColor::Red.colorize(line.as_ref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_message() {
        let msg = MessageFormatter::error("Test error");
        assert!(msg.contains("Test error"));
        assert!(msg.contains("✗"));
    }

    #[test]
    fn test_success_message() {
        let msg = MessageFormatter::success("Test success");
        assert!(msg.contains("Test success"));
        assert!(msg.contains("✓"));
    }

    #[test]
    fn test_command_not_found() {
        let msg = MessageFormatter::command_not_found("docker");
        assert!(msg.contains("docker"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_info_message() {
        let msg = MessageFormatter::info("Information");
        assert!(msg.contains("Information"));
        assert!(msg.contains("ℹ"));
    }

    #[test]
    fn test_command_message() {
        let msg = MessageFormatter::command("ls -la");
        assert!(msg.contains("ls -la"));
        assert!(msg.contains("❯"));
    }

    #[test]
    fn test_suggestion_message() {
        let msg = MessageFormatter::suggestion("Try this");
        assert!(msg.contains("Try this"));
        assert!(msg.contains("→"));
    }

    #[test]
    fn test_command_failed() {
        let msg = MessageFormatter::command_failed(127);
        assert!(msg.contains("127"));
        assert!(msg.contains("exited"));
    }

    #[test]
    fn test_execution_error() {
        let msg = MessageFormatter::execution_error("Permission denied");
        assert!(msg.contains("Permission denied"));
        assert!(msg.contains("Error executing"));
    }

    #[test]
    fn test_install_suggestion_available() {
        let msg = MessageFormatter::install_suggestion(true);
        assert!(msg.contains("install"));
    }

    #[test]
    fn test_install_suggestion_unavailable() {
        let msg = MessageFormatter::install_suggestion(false);
        assert!(msg.contains("Package manager"));
        assert!(msg.contains("not available"));
    }

    #[test]
    fn test_banner_line() {
        let msg = MessageFormatter::banner_line("Welcome");
        assert!(msg.contains("Welcome"));
    }

    #[test]
    fn test_banner_hint() {
        let msg = MessageFormatter::banner_hint("Press Ctrl+C to quit");
        assert!(msg.contains("Press Ctrl+C"));
    }

    #[test]
    fn test_stderr_error() {
        let msg = MessageFormatter::stderr_error("Error line");
        assert!(msg.contains("Error line"));
    }

    #[test]
    fn test_as_ref_str_compatibility() {
        // Test that methods work with both &str and String
        let _ = MessageFormatter::error("string literal");
        let _ = MessageFormatter::error(String::from("owned string"));
        let _ = MessageFormatter::success("reference");
    }
}
