//! Engine status types for state machine synchronization
//!
//! This module defines the `EngineStatus` enum that represents the current
//! state of the Engine. Terminal-app uses this to derive `AppMode` and
//! control the throbber.

use serde::{Deserialize, Serialize};

use crate::Interrupt;

/// Engine status representing the current workflow state
///
/// This enum is the single source of truth for Engine state.
/// Terminal-app derives `AppMode` from this.
///
/// # State Transitions
///
/// ```text
/// Ready → Thinking → Interrupted → (resume) → Thinking → ... → Ready
///                         ↑                         │
///                         └─────────────────────────┘
/// ```
///
/// # Examples
///
/// ```
/// use infraware_shared::{EngineStatus, Interrupt};
///
/// // Engine starts ready
/// let status = EngineStatus::Ready;
///
/// // User sends query, engine starts thinking
/// let status = EngineStatus::Thinking;
///
/// // Engine needs user approval
/// let status = EngineStatus::Interrupted(
///     Interrupt::command_approval("ls -la", "List files", false)
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EngineStatus {
    /// Workflow complete, ready for new query
    #[default]
    Ready,
    /// Actively processing (throbber should be ON)
    Thinking,
    /// Suspended, waiting for user input (HITL)
    Interrupted(Interrupt),
}

impl EngineStatus {
    /// Create a Ready status
    pub fn ready() -> Self {
        Self::Ready
    }

    /// Create a Thinking status
    pub fn thinking() -> Self {
        Self::Thinking
    }

    /// Create an Interrupted status with the given interrupt
    pub fn interrupted(interrupt: Interrupt) -> Self {
        Self::Interrupted(interrupt)
    }

    /// Check if the engine is ready for a new query
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Check if the engine is actively thinking (throbber ON)
    #[must_use]
    pub fn is_thinking(&self) -> bool {
        matches!(self, Self::Thinking)
    }

    /// Check if the engine is waiting for user input
    #[must_use]
    pub fn is_interrupted(&self) -> bool {
        matches!(self, Self::Interrupted(_))
    }

    /// Get the name of the current status (for logging)
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Thinking => "Thinking",
            Self::Interrupted(_) => "Interrupted",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_is_ready() {
        assert_eq!(EngineStatus::default(), EngineStatus::Ready);
    }

    #[test]
    fn test_status_constructors() {
        assert!(EngineStatus::ready().is_ready());
        assert!(EngineStatus::thinking().is_thinking());

        let interrupt = Interrupt::command_approval("ls", "list", false);
        assert!(EngineStatus::interrupted(interrupt).is_interrupted());
    }

    #[test]
    fn test_status_names() {
        assert_eq!(EngineStatus::Ready.name(), "Ready");
        assert_eq!(EngineStatus::Thinking.name(), "Thinking");

        let interrupt = Interrupt::question("test?", None);
        assert_eq!(EngineStatus::Interrupted(interrupt).name(), "Interrupted");
    }

    #[test]
    fn test_serde_roundtrip() {
        let statuses = vec![
            EngineStatus::Ready,
            EngineStatus::Thinking,
            EngineStatus::Interrupted(Interrupt::command_approval("rm -rf", "clean", false)),
            EngineStatus::Interrupted(Interrupt::question("which?", Some(vec!["a".into()]))),
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: EngineStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_serde_format() {
        let status = EngineStatus::Thinking;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"thinking\""));

        let status = EngineStatus::Interrupted(Interrupt::command_approval("ls", "list", false));
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"interrupted\""));
        assert!(json.contains("\"command\":\"ls\""));
    }
}
