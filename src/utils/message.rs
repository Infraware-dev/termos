/// Centralized message formatting for consistent output styling
use super::ansi::AnsiColor;

/// Message type determines the styling and prefix
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum MessageType {
    Error,
    Warning,
    Success,
    Info,
    Command,
    Question,
}

/// Message formatter for creating consistently styled output
pub struct MessageFormatter;

#[allow(dead_code)]
impl MessageFormatter {
    /// Format an error message
    pub fn error(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Red.colorize("✗"), message.as_ref())
    }

    /// Format a warning message
    pub fn warning(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Yellow.colorize("⚠"), message.as_ref())
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

    /// Format a question/prompt for user
    pub fn question(message: impl AsRef<str>) -> String {
        format!("{} {}", AnsiColor::Magenta.colorize("?"), message.as_ref())
    }

    /// Format a suggestion/hint
    pub fn suggestion(message: impl AsRef<str>) -> String {
        format!("  {} {}", AnsiColor::Yellow.colorize("→"), message.as_ref())
    }

    /// Format a generic message with custom type
    pub fn format(msg_type: MessageType, message: impl AsRef<str>) -> String {
        match msg_type {
            MessageType::Error => Self::error(message),
            MessageType::Warning => Self::warning(message),
            MessageType::Success => Self::success(message),
            MessageType::Info => Self::info(message),
            MessageType::Command => Self::command(message),
            MessageType::Question => Self::question(message),
        }
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
    fn test_message_type_format() {
        let msg = MessageFormatter::format(MessageType::Warning, "Test");
        assert!(msg.contains("Test"));
        assert!(msg.contains("⚠"));
    }
}
