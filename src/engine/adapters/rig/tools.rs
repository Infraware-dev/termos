//! Tools for the Rig engine adapter
//!
//! This module provides rig-rs native tools for:
//! - Shell command execution with HITL approval
//! - Asking user questions for clarification
//!
//! These tools implement the `rig::tool::Tool` trait and integrate with
//! rig-rs's native function calling system via `PromptHook`.

mod ask_user;
mod diagnostic_command;
mod shell;
mod start_incident;

// Tool result types - used by orchestrator for HITL detection
pub use ask_user::AskUserResult;
// Tool implementations - registered with agent via .tool()
pub use ask_user::{AskUserArgs, AskUserTool};
pub use diagnostic_command::{DiagnosticCommandArgs, DiagnosticCommandTool, format_hitl_message};
use serde::{Deserialize, Serialize};
pub use shell::{ShellCommandArgs, ShellCommandResult, ShellCommandTool};
pub use start_incident::{StartIncidentArgs, StartIncidentInvestigationTool};

use crate::engine::shared::Interrupt;

/// Unified HITL marker that can represent any tool result requiring user interaction
///
/// This enum provides a common type for converting tool results into interrupt events.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hitl_type", rename_all = "snake_case")]
pub enum HitlMarker {
    /// The LLM wants to execute a shell command and needs user approval
    CommandApproval {
        /// The shell command to execute
        command: String,
        /// Explanation of what the command does
        message: String,
        /// Whether the agent needs to process output after execution
        #[serde(default)]
        needs_continuation: bool,
    },
    /// The LLM needs to ask the user a question
    Question {
        /// The question to ask
        question: String,
        /// Optional predefined answer choices
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<Vec<String>>,
    },
}

impl HitlMarker {
    /// Create a command approval marker
    pub fn command_approval(
        command: impl Into<String>,
        message: impl Into<String>,
        needs_continuation: bool,
    ) -> Self {
        Self::CommandApproval {
            command: command.into(),
            message: message.into(),
            needs_continuation,
        }
    }

    /// Create a question marker
    pub fn question(question: impl Into<String>, options: Option<Vec<String>>) -> Self {
        Self::Question {
            question: question.into(),
            options,
        }
    }
}

impl From<HitlMarker> for Interrupt {
    fn from(marker: HitlMarker) -> Self {
        match marker {
            HitlMarker::CommandApproval {
                command,
                message,
                needs_continuation,
            } => Interrupt::command_approval(command, message, needs_continuation),
            HitlMarker::Question { question, options } => Interrupt::question(question, options),
        }
    }
}

impl From<ShellCommandResult> for Option<HitlMarker> {
    fn from(result: ShellCommandResult) -> Self {
        match result {
            ShellCommandResult::PendingApproval {
                command,
                explanation,
                needs_continuation,
            } => Some(HitlMarker::CommandApproval {
                command,
                message: explanation,
                needs_continuation,
            }),
            _ => None,
        }
    }
}

impl From<AskUserResult> for Option<HitlMarker> {
    fn from(result: AskUserResult) -> Self {
        match result {
            AskUserResult::Pending { question, options } => {
                Some(HitlMarker::Question { question, options })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hitl_marker_command_approval() {
        let marker = HitlMarker::command_approval("ls -la", "List files", false);
        assert!(matches!(marker, HitlMarker::CommandApproval { .. }));
    }

    #[test]
    fn test_hitl_marker_question() {
        let marker = HitlMarker::question("Which env?", Some(vec!["dev".into(), "prod".into()]));
        assert!(matches!(marker, HitlMarker::Question { .. }));
    }

    #[test]
    fn test_hitl_marker_to_interrupt() {
        let marker = HitlMarker::command_approval("ls", "List", false);
        let interrupt: Interrupt = marker.into();
        assert!(matches!(interrupt, Interrupt::CommandApproval { .. }));
    }

    #[test]
    fn test_shell_result_to_marker() {
        let result = ShellCommandResult::PendingApproval {
            command: "ls".to_string(),
            explanation: "List files".to_string(),
            needs_continuation: false,
        };

        let marker: Option<HitlMarker> = result.into();
        assert!(marker.is_some());
        assert!(matches!(
            marker.unwrap(),
            HitlMarker::CommandApproval { .. }
        ));
    }

    #[test]
    fn test_shell_executed_no_marker() {
        let result = ShellCommandResult::Executed {
            command: "ls".to_string(),
            output: "file1".to_string(),
            success: true,
            exit_code: Some(0),
        };

        let marker: Option<HitlMarker> = result.into();
        assert!(marker.is_none());
    }

    #[test]
    fn test_ask_user_result_to_marker() {
        let result = AskUserResult::Pending {
            question: "Which env?".to_string(),
            options: Some(vec!["dev".to_string()]),
        };

        let marker: Option<HitlMarker> = result.into();
        assert!(marker.is_some());
        assert!(matches!(marker.unwrap(), HitlMarker::Question { .. }));
    }
}
