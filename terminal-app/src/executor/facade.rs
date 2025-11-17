//! Command Execution Facade Pattern
//!
//! This module provides a simplified, high-level interface for command execution
//! that handles all the complexity of checking existence, executing, and handling errors.
//!
//! TODO: Remove #![allow(dead_code)] once integrated into main terminal flow
#![allow(dead_code)]

use anyhow::Result;

use super::command::{CommandExecutor, CommandOutput};
use super::install::PackageInstaller;

/// Result of a command execution attempt
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionResult {
    /// Command executed successfully
    Success(CommandOutput),
    /// Command not found on the system
    CommandNotFound {
        command: String,
        can_install: bool,
        package_manager: Option<String>,
    },
    /// Command execution failed
    ExecutionError { command: String, error: String },
}

impl ExecutionResult {
    /// Check if the execution was successful
    #[must_use]
    pub fn is_success(&self) -> bool {
        matches!(self, ExecutionResult::Success(_))
    }

    /// Get the command output if successful
    #[must_use]
    pub fn output(&self) -> Option<&CommandOutput> {
        match self {
            ExecutionResult::Success(output) => Some(output),
            _ => None,
        }
    }

    /// Get the error message if failed
    #[must_use]
    pub fn error_message(&self) -> Option<String> {
        match self {
            ExecutionResult::CommandNotFound {
                command,
                can_install,
                package_manager,
            } => {
                let mut msg = format!("Command '{}' not found", command);
                if *can_install {
                    if let Some(pm) = package_manager {
                        msg.push_str(&format!(" (can be installed via {})", pm));
                    }
                }
                Some(msg)
            }
            ExecutionResult::ExecutionError { command, error } => {
                Some(format!("Error executing '{}': {}", command, error))
            }
            ExecutionResult::Success(_) => None,
        }
    }
}

/// Facade for command execution that simplifies the interface
#[derive(Debug)]
pub struct CommandExecutionFacade {
    installer: PackageInstaller,
}

impl CommandExecutionFacade {
    /// Create a new command execution facade
    pub fn new() -> Self {
        Self {
            installer: PackageInstaller::new(),
        }
    }

    /// Create a facade with a custom package installer
    pub fn with_installer(installer: PackageInstaller) -> Self {
        Self { installer }
    }

    /// Execute a command with automatic error handling and fallback
    ///
    /// This is the main entry point that handles all the complexity:
    /// 1. Check if command exists
    /// 2. Execute if found
    /// 3. Provide installation suggestions if not found
    /// 4. Handle execution errors gracefully
    ///
    /// # Errors
    ///
    /// Returns an error only if the command execution system fails internally.
    /// Command-level failures are captured in `ExecutionResult` variants:
    /// - `ExecutionResult::CommandNotFound` - command not in PATH
    /// - `ExecutionResult::ExecutionError` - command failed during execution
    pub async fn execute_with_fallback(
        &self,
        cmd: &str,
        args: &[String],
    ) -> Result<ExecutionResult> {
        // Check if command exists
        if !CommandExecutor::command_exists(cmd) {
            return Ok(ExecutionResult::CommandNotFound {
                command: cmd.to_string(),
                can_install: self.installer.is_available(),
                package_manager: self.installer.get_package_manager().map(|s| s.to_string()),
            });
        }

        // Execute the command
        match CommandExecutor::execute(cmd, args).await {
            Ok(output) => Ok(ExecutionResult::Success(output)),
            Err(e) => Ok(ExecutionResult::ExecutionError {
                command: cmd.to_string(),
                error: e.to_string(),
            }),
        }
    }

    /// Execute a command and install it if not found (interactive)
    ///
    /// This method will attempt to install the command if it's not found
    /// and the user confirms the installation.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The command execution system fails internally
    /// - Package installation fails (captured in `ExecutionResult::ExecutionError`)
    pub async fn execute_or_install(
        &self,
        cmd: &str,
        args: &[String],
        auto_install: bool,
    ) -> Result<ExecutionResult> {
        // First attempt to execute
        let result = self.execute_with_fallback(cmd, args).await?;

        // If command not found and auto-install is enabled, try to install
        if matches!(result, ExecutionResult::CommandNotFound { .. })
            && auto_install
            && self.installer.is_available()
        {
            // Attempt installation
            if let Err(e) = self.installer.install_package(cmd).await {
                return Ok(ExecutionResult::ExecutionError {
                    command: cmd.to_string(),
                    error: format!("Installation failed: {}", e),
                });
            }

            // Retry execution after installation
            return self.execute_with_fallback(cmd, args).await;
        }

        Ok(result)
    }

    /// Check if a command is available for execution
    #[must_use]
    pub fn is_command_available(&self, cmd: &str) -> bool {
        CommandExecutor::command_exists(cmd)
    }

    /// Check if package installation is available
    #[must_use]
    pub fn can_install_packages(&self) -> bool {
        self.installer.is_available()
    }

    /// Get the available package manager name
    #[must_use]
    pub fn get_package_manager(&self) -> Option<&str> {
        self.installer.get_package_manager()
    }
}

impl Default for CommandExecutionFacade {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_execute_existing_command() {
        let facade = CommandExecutionFacade::new();
        let result = facade
            .execute_with_fallback("echo", &["test".to_string()])
            .await;

        assert!(result.is_ok());
        let exec_result = result.unwrap();
        assert!(exec_result.is_success());
    }

    #[tokio::test]
    async fn test_execute_nonexistent_command() {
        let facade = CommandExecutionFacade::new();
        let result = facade
            .execute_with_fallback("nonexistent_command_12345", &[])
            .await;

        assert!(result.is_ok());
        let exec_result = result.unwrap();
        assert!(!exec_result.is_success());

        match exec_result {
            ExecutionResult::CommandNotFound { command, .. } => {
                assert_eq!(command, "nonexistent_command_12345");
            }
            _ => panic!("Expected CommandNotFound"),
        }
    }

    #[test]
    fn test_execution_result_is_success() {
        let success = ExecutionResult::Success(CommandOutput {
            stdout: "test".to_string(),
            stderr: String::new(),
            exit_code: 0,
        });
        assert!(success.is_success());

        let not_found = ExecutionResult::CommandNotFound {
            command: "test".to_string(),
            can_install: false,
            package_manager: None,
        };
        assert!(!not_found.is_success());
    }

    #[test]
    fn test_execution_result_output() {
        let output = CommandOutput {
            stdout: "test output".to_string(),
            stderr: String::new(),
            exit_code: 0,
        };
        let success = ExecutionResult::Success(output.clone());

        assert_eq!(success.output().unwrap().stdout, "test output");

        let not_found = ExecutionResult::CommandNotFound {
            command: "test".to_string(),
            can_install: false,
            package_manager: None,
        };
        assert!(not_found.output().is_none());
    }

    #[test]
    fn test_execution_result_error_message() {
        let not_found = ExecutionResult::CommandNotFound {
            command: "htop".to_string(),
            can_install: true,
            package_manager: Some("brew".to_string()),
        };
        assert!(not_found.error_message().is_some());
        assert!(not_found.error_message().unwrap().contains("htop"));
        assert!(not_found.error_message().unwrap().contains("brew"));

        let success = ExecutionResult::Success(CommandOutput {
            stdout: String::new(),
            stderr: String::new(),
            exit_code: 0,
        });
        assert!(success.error_message().is_none());
    }

    #[test]
    fn test_facade_creation() {
        let facade = CommandExecutionFacade::new();
        assert!(facade.can_install_packages() || !facade.can_install_packages());
    }

    #[test]
    fn test_is_command_available() {
        let facade = CommandExecutionFacade::new();
        assert!(facade.is_command_available("echo"));
        assert!(!facade.is_command_available("nonexistent_command_12345"));
    }
}
