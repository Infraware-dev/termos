/// Custom error types for Infraware Terminal (M2/M3)
use thiserror::Error;

/// Result type alias for Infraware Terminal
#[allow(dead_code)]
pub type InfraResult<T> = Result<T, InfraError>;

/// Main error type for the application
#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum InfraError {
    /// IO errors
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Command execution errors
    #[error("Command execution failed: {0}")]
    CommandExecution(String),

    /// Command not found
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    /// LLM client errors
    #[error("LLM request failed: {0}")]
    LLMRequest(String),

    /// Network errors
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    /// Parsing errors
    #[error("Parse error: {0}")]
    Parse(String),

    /// Terminal UI errors
    #[error("Terminal UI error: {0}")]
    TerminalUI(String),

    /// Package installation errors
    #[error("Package installation failed: {0}")]
    PackageInstall(String),

    /// No package manager found
    #[error("No supported package manager found on this system")]
    NoPackageManager,

    /// Configuration errors
    #[error("Configuration error: {0}")]
    Config(String),

    /// Generic error
    #[error("Error: {0}")]
    Generic(String),

    /// Wrapped anyhow error
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

#[allow(dead_code)]
impl InfraError {
    /// Create a command execution error
    pub fn command_execution(msg: impl Into<String>) -> Self {
        InfraError::CommandExecution(msg.into())
    }

    /// Create a command not found error
    pub fn command_not_found(cmd: impl Into<String>) -> Self {
        InfraError::CommandNotFound(cmd.into())
    }

    /// Create an LLM request error
    pub fn llm_request(msg: impl Into<String>) -> Self {
        InfraError::LLMRequest(msg.into())
    }

    /// Create a parse error
    pub fn parse(msg: impl Into<String>) -> Self {
        InfraError::Parse(msg.into())
    }

    /// Create a terminal UI error
    pub fn terminal_ui(msg: impl Into<String>) -> Self {
        InfraError::TerminalUI(msg.into())
    }

    /// Create a package install error
    pub fn package_install(msg: impl Into<String>) -> Self {
        InfraError::PackageInstall(msg.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        InfraError::Config(msg.into())
    }

    /// Create a generic error
    pub fn generic(msg: impl Into<String>) -> Self {
        InfraError::Generic(msg.into())
    }

    /// Check if this is a command not found error
    pub fn is_command_not_found(&self) -> bool {
        matches!(self, InfraError::CommandNotFound(_))
    }

    /// Get the error message
    pub fn message(&self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_not_found() {
        let err = InfraError::command_not_found("test");
        assert!(err.is_command_not_found());
        assert!(err.message().contains("test"));
    }

    #[test]
    fn test_error_display() {
        let err = InfraError::command_execution("Failed to run");
        assert_eq!(err.to_string(), "Command execution failed: Failed to run");
    }

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let infra_err: InfraError = io_err.into();
        assert!(matches!(infra_err, InfraError::Io(_)));
    }

    #[test]
    fn test_llm_request_error() {
        let err = InfraError::llm_request("API timeout");
        assert_eq!(err.to_string(), "LLM request failed: API timeout");
        assert!(err.message().contains("API timeout"));
    }

    #[test]
    fn test_parse_error() {
        let err = InfraError::parse("Invalid syntax");
        assert_eq!(err.to_string(), "Parse error: Invalid syntax");
    }

    #[test]
    fn test_terminal_ui_error() {
        let err = InfraError::terminal_ui("Render failed");
        assert_eq!(err.to_string(), "Terminal UI error: Render failed");
    }

    #[test]
    fn test_package_install_error() {
        let err = InfraError::package_install("apt-get failed");
        assert_eq!(
            err.to_string(),
            "Package installation failed: apt-get failed"
        );
    }

    #[test]
    fn test_config_error() {
        let err = InfraError::config("Invalid config file");
        assert_eq!(err.to_string(), "Configuration error: Invalid config file");
    }

    #[test]
    fn test_generic_error() {
        let err = InfraError::generic("Something went wrong");
        assert_eq!(err.to_string(), "Error: Something went wrong");
    }

    #[test]
    fn test_no_package_manager() {
        let err = InfraError::NoPackageManager;
        assert_eq!(
            err.to_string(),
            "No supported package manager found on this system"
        );
    }

    #[test]
    fn test_is_command_not_found_negative() {
        let err = InfraError::command_execution("test");
        assert!(!err.is_command_not_found());
    }

    #[test]
    fn test_message_method() {
        let err = InfraError::CommandNotFound("kubectl".to_string());
        let msg = err.message();
        assert!(msg.contains("kubectl"));
        assert!(msg.contains("not found"));
    }

    #[test]
    fn test_from_anyhow_error() {
        let anyhow_err = anyhow::anyhow!("test error");
        let infra_err: InfraError = anyhow_err.into();
        assert!(matches!(infra_err, InfraError::Other(_)));
    }

    #[test]
    fn test_all_error_constructors() {
        // Test that all constructor methods work
        let _ = InfraError::command_execution("test");
        let _ = InfraError::command_not_found("test");
        let _ = InfraError::llm_request("test");
        let _ = InfraError::parse("test");
        let _ = InfraError::terminal_ui("test");
        let _ = InfraError::package_install("test");
        let _ = InfraError::config("test");
        let _ = InfraError::generic("test");
    }

    #[test]
    fn test_error_debug() {
        let err = InfraError::CommandNotFound("test".to_string());
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("CommandNotFound"));
    }
}
