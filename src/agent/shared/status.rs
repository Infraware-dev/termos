//! Agent status types for state machine synchronization
//!
//! This module defines the `AgentStatus` enum that represents the current
//! state of the Agent. Terminal-app uses this to derive `AppMode` and
//! control the throbber.

use serde::{Deserialize, Serialize};

use super::events::Interrupt;

/// Agent status representing the current workflow state
///
/// This enum is the single source of truth for Agent state.
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
/// use crate::agent::shared::{AgentStatus, Interrupt};
///
/// // Agent starts ready
/// let status = AgentStatus::Ready;
///
/// // User sends query, agent starts thinking
/// let status = AgentStatus::Thinking;
///
/// // Agent needs user approval
/// let status = AgentStatus::Interrupted(
///     Interrupt::command_approval("ls -la", "List files", false)
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AgentStatus {
    /// Workflow complete, ready for new query
    #[default]
    Ready,
    /// Actively processing (throbber should be ON)
    Thinking,
    /// Suspended, waiting for user input (HITL)
    Interrupted(Interrupt),
}

impl AgentStatus {
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

    /// Check if the agent is ready for a new query
    #[must_use]
    pub fn is_ready(&self) -> bool {
        matches!(self, Self::Ready)
    }

    /// Check if the agent is actively thinking (throbber ON)
    #[must_use]
    pub fn is_thinking(&self) -> bool {
        matches!(self, Self::Thinking)
    }

    /// Check if the agent is waiting for user input
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
        assert_eq!(AgentStatus::default(), AgentStatus::Ready);
    }

    #[test]
    fn test_status_constructors() {
        assert!(AgentStatus::ready().is_ready());
        assert!(AgentStatus::thinking().is_thinking());

        let interrupt = Interrupt::command_approval("ls", "list", false);
        assert!(AgentStatus::interrupted(interrupt).is_interrupted());
    }

    #[test]
    fn test_status_names() {
        assert_eq!(AgentStatus::Ready.name(), "Ready");
        assert_eq!(AgentStatus::Thinking.name(), "Thinking");

        let interrupt = Interrupt::question("test?", None);
        assert_eq!(AgentStatus::Interrupted(interrupt).name(), "Interrupted");
    }

    #[test]
    fn test_serde_roundtrip() {
        let statuses = vec![
            AgentStatus::Ready,
            AgentStatus::Thinking,
            AgentStatus::Interrupted(Interrupt::command_approval("rm -rf", "clean", false)),
            AgentStatus::Interrupted(Interrupt::question("which?", Some(vec!["a".into()]))),
        ];

        for status in statuses {
            let json = serde_json::to_string(&status).unwrap();
            let parsed: AgentStatus = serde_json::from_str(&json).unwrap();
            assert_eq!(status, parsed);
        }
    }

    #[test]
    fn test_serde_format() {
        let status = AgentStatus::Thinking;
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"thinking\""));

        let status = AgentStatus::Interrupted(Interrupt::command_approval("ls", "list", false));
        let json = serde_json::to_string(&status).unwrap();
        assert!(json.contains("\"status\":\"interrupted\""));
        assert!(json.contains("\"command\":\"ls\""));
    }
}
