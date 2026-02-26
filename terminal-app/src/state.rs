//! Application state management with State Machine pattern.
//!
//! This module provides a type-safe state machine for application modes
//! with validated transitions and logging.
//!
//! # Architecture
//!
//! ```text
//! EngineStatus (from backend) → AppMode (UI state) → Throbber ON/OFF
//! ```
//!
//! `AppMode` is derived from `EngineStatus` via `From` trait implementation.

use infraware_shared::{EngineStatus, Interrupt};

/// Application mode states.
///
/// The state machine enforces valid transitions between modes.
/// Invalid transitions return an error.
#[derive(Debug, Clone, PartialEq, Default)]
#[allow(dead_code)] // Variants used when LLM integration is active
pub enum AppMode {
    /// Normal operation - waiting for user input
    #[default]
    Normal,
    /// Waiting for LLM response after "command not found"
    WaitingLLM,
    /// LLM requested command approval (y/n)
    AwaitingApproval {
        command: String,
        message: String,
        needs_continuation: bool,
    },
    /// LLM asked a question (free-text answer)
    AwaitingAnswer {
        question: String,
        options: Option<Vec<String>>,
    },
    /// Executing approved command in PTY, capturing output
    ExecutingCommand {
        command: String,
        needs_continuation: bool,
    },
}

/// Derive AppMode from EngineStatus
///
/// This is the primary way to convert backend state to UI state.
/// Throbber is ON when `AppMode::WaitingLLM` (i.e., `EngineStatus::Thinking`).
impl From<EngineStatus> for AppMode {
    fn from(status: EngineStatus) -> Self {
        match status {
            EngineStatus::Ready => Self::Normal,
            EngineStatus::Thinking => Self::WaitingLLM,
            EngineStatus::Interrupted(interrupt) => match interrupt {
                Interrupt::CommandApproval {
                    command,
                    message,
                    needs_continuation,
                } => Self::AwaitingApproval {
                    command,
                    message,
                    needs_continuation,
                },
                Interrupt::Question { question, options } => {
                    Self::AwaitingAnswer { question, options }
                }
            },
        }
    }
}

/// Tracks LLM agent stream timing for timeout detection.
///
/// Note: `stream_active` was removed as it's now redundant with
/// `EngineStatus::Thinking`. The throbber is controlled directly
/// by `AppMode::WaitingLLM`.
#[derive(Debug, Clone, Default)]
pub struct AgentState {
    /// Timestamp when the stream started (for timeout detection).
    pub stream_started: Option<std::time::Instant>,
}

impl AgentState {
    /// Create a new agent state with default values.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Mark the stream as started.
    pub fn start_stream(&mut self) {
        self.stream_started = Some(std::time::Instant::now());
    }

    /// Mark the stream as ended.
    pub fn end_stream(&mut self) {
        self.stream_started = None;
    }
}

#[allow(dead_code)] // State machine API used when LLM integration is active
impl AppMode {
    /// Get the name of the current state (for logging).
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::WaitingLLM => "WaitingLLM",
            Self::AwaitingApproval { .. } => "AwaitingApproval",
            Self::AwaitingAnswer { .. } => "AwaitingAnswer",
            Self::ExecutingCommand { .. } => "ExecutingCommand",
        }
    }

    /// Check if a transition to the target state is valid.
    ///
    /// Valid transitions:
    /// - Normal → WaitingLLM (user query)
    /// - WaitingLLM → Normal (LLM completed)
    /// - WaitingLLM → AwaitingApproval (LLM requests approval)
    /// - WaitingLLM → AwaitingAnswer (LLM asks question)
    /// - AwaitingApproval → Normal (user rejected)
    /// - AwaitingApproval → ExecutingCommand (user approved, command sent to PTY)
    /// - AwaitingApproval → WaitingLLM (legacy: resume after approval)
    /// - AwaitingAnswer → Normal (user answered)
    /// - AwaitingAnswer → WaitingLLM (resume with answer)
    /// - ExecutingCommand → WaitingLLM (command finished, output sent to backend)
    /// - ExecutingCommand → Normal (user cancelled)
    /// - Any → Normal (cancel)
    #[must_use]
    pub fn can_transition_to(&self, target: &Self) -> bool {
        match (self, target) {
            // From Normal
            (Self::Normal, Self::WaitingLLM) => true,

            // From WaitingLLM
            (Self::WaitingLLM, Self::Normal) => true,
            (Self::WaitingLLM, Self::AwaitingApproval { .. }) => true,
            (Self::WaitingLLM, Self::AwaitingAnswer { .. }) => true,

            // From AwaitingApproval
            (Self::AwaitingApproval { .. }, Self::Normal) => true,
            (Self::AwaitingApproval { .. }, Self::WaitingLLM) => true,
            (Self::AwaitingApproval { .. }, Self::ExecutingCommand { .. }) => true,

            // From AwaitingAnswer
            (Self::AwaitingAnswer { .. }, Self::Normal) => true,
            (Self::AwaitingAnswer { .. }, Self::WaitingLLM) => true,

            // From ExecutingCommand
            (Self::ExecutingCommand { .. }, Self::WaitingLLM) => true,
            (Self::ExecutingCommand { .. }, Self::Normal) => true,

            // Same state (idempotent transitions are valid)
            (Self::Normal, Self::Normal)
            | (Self::WaitingLLM, Self::WaitingLLM)
            | (Self::AwaitingApproval { .. }, Self::AwaitingApproval { .. })
            | (Self::AwaitingAnswer { .. }, Self::AwaitingAnswer { .. })
            | (Self::ExecutingCommand { .. }, Self::ExecutingCommand { .. }) => true,

            // All others invalid
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        assert_eq!(AppMode::default(), AppMode::Normal);
    }

    #[test]
    fn test_valid_transitions() {
        // Normal → WaitingLLM
        let state = AppMode::Normal;
        assert!(state.can_transition_to(&AppMode::WaitingLLM));

        // WaitingLLM → Normal
        let state = AppMode::WaitingLLM;
        assert!(state.can_transition_to(&AppMode::Normal));

        // WaitingLLM → AwaitingApproval
        assert!(state.can_transition_to(&AppMode::AwaitingApproval {
            command: "test".to_string(),
            message: "msg".to_string(),
            needs_continuation: false,
        }));

        // AwaitingApproval → Normal
        let state = AppMode::AwaitingApproval {
            command: "test".to_string(),
            message: "msg".to_string(),
            needs_continuation: false,
        };
        assert!(state.can_transition_to(&AppMode::Normal));
    }

    #[test]
    fn test_invalid_transitions() {
        // Normal → AwaitingApproval (must go through WaitingLLM)
        let state = AppMode::Normal;
        assert!(!state.can_transition_to(&AppMode::AwaitingApproval {
            command: "test".to_string(),
            message: "msg".to_string(),
            needs_continuation: false,
        }));

        // Normal → AwaitingAnswer (must go through WaitingLLM)
        assert!(!state.can_transition_to(&AppMode::AwaitingAnswer {
            question: "test?".to_string(),
            options: None,
        }));
    }

    #[test]
    fn test_state_names() {
        assert_eq!(AppMode::Normal.name(), "Normal");
        assert_eq!(AppMode::WaitingLLM.name(), "WaitingLLM");
        assert_eq!(
            AppMode::AwaitingApproval {
                command: "test".to_string(),
                message: "msg".to_string(),
                needs_continuation: false,
            }
            .name(),
            "AwaitingApproval"
        );
    }

    // Tests for From<EngineStatus> implementation

    #[test]
    fn test_from_engine_status_ready() {
        let status = EngineStatus::Ready;
        let mode: AppMode = status.into();
        assert_eq!(mode, AppMode::Normal);
    }

    #[test]
    fn test_from_engine_status_thinking() {
        let status = EngineStatus::Thinking;
        let mode: AppMode = status.into();
        assert_eq!(mode, AppMode::WaitingLLM);
    }

    #[test]
    fn test_from_engine_status_interrupted_command() {
        let status = EngineStatus::Interrupted(Interrupt::CommandApproval {
            command: "ls -la".to_string(),
            message: "List files".to_string(),
            needs_continuation: false,
        });
        let mode: AppMode = status.into();
        assert_eq!(
            mode,
            AppMode::AwaitingApproval {
                command: "ls -la".to_string(),
                message: "List files".to_string(),
                needs_continuation: false,
            }
        );
    }

    #[test]
    fn test_from_engine_status_interrupted_question() {
        let status = EngineStatus::Interrupted(Interrupt::Question {
            question: "Which env?".to_string(),
            options: Some(vec!["dev".to_string(), "prod".to_string()]),
        });
        let mode: AppMode = status.into();
        assert_eq!(
            mode,
            AppMode::AwaitingAnswer {
                question: "Which env?".to_string(),
                options: Some(vec!["dev".to_string(), "prod".to_string()]),
            }
        );
    }

    #[test]
    fn test_from_engine_status_question_no_options() {
        let status = EngineStatus::Interrupted(Interrupt::Question {
            question: "What name?".to_string(),
            options: None,
        });
        let mode: AppMode = status.into();
        assert_eq!(
            mode,
            AppMode::AwaitingAnswer {
                question: "What name?".to_string(),
                options: None,
            }
        );
    }
}
