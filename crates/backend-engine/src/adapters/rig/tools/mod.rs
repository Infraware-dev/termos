//! HITL markers for the Rig engine adapter
//!
//! These markers signal when the orchestrator should pause for user input.

use serde::{Deserialize, Serialize};

use infraware_shared::Interrupt;

/// Marker to signal HITL interaction is needed
///
/// When the orchestrator detects a HitlMarker pattern in the LLM response:
/// 1. Stop the current agent execution
/// 2. Emit an interrupt event to the client
/// 3. Store the pending interrupt for resume_run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "hitl_type", rename_all = "snake_case")]
pub enum HitlMarker {
    /// The LLM wants to execute a shell command and needs user approval
    CommandApproval {
        /// The shell command to execute
        command: String,
        /// Explanation of what the command does and why it's needed
        message: String,
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
    pub fn command_approval(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CommandApproval {
            command: command.into(),
            message: message.into(),
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
            HitlMarker::CommandApproval { command, message } => {
                Interrupt::command_approval(command, message)
            }
            HitlMarker::Question { question, options } => Interrupt::question(question, options),
        }
    }
}

/// Prefix used to detect HITL markers in LLM output
pub const HITL_MARKER_PREFIX: &str = "[[HITL:";
pub const HITL_MARKER_SUFFIX: &str = "]]";

/// Try to parse a HitlMarker from LLM output (for use with [[HITL:...]] format)
pub fn parse_hitl_marker(output: &str) -> Option<HitlMarker> {
    let start = output.find(HITL_MARKER_PREFIX)?;
    let json_start = start + HITL_MARKER_PREFIX.len();
    let end = output[json_start..].find(HITL_MARKER_SUFFIX)?;
    let json = &output[json_start..json_start + end];

    serde_json::from_str(json).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hitl_marker_command_approval() {
        let marker = HitlMarker::command_approval("ls -la", "List files");
        assert!(matches!(marker, HitlMarker::CommandApproval { .. }));
    }

    #[test]
    fn test_hitl_marker_question() {
        let marker = HitlMarker::question("Which env?", Some(vec!["dev".into(), "prod".into()]));
        assert!(matches!(marker, HitlMarker::Question { .. }));
    }

    #[test]
    fn test_hitl_marker_to_interrupt() {
        let marker = HitlMarker::command_approval("ls", "List");
        let interrupt: Interrupt = marker.into();
        assert!(matches!(interrupt, Interrupt::CommandApproval { .. }));
    }

    #[test]
    fn test_parse_marker() {
        let json =
            r#"{"hitl_type":"command_approval","command":"docker ps","message":"List containers"}"#;
        let output = format!("{}{}{}", HITL_MARKER_PREFIX, json, HITL_MARKER_SUFFIX);

        let parsed = parse_hitl_marker(&output);
        assert!(parsed.is_some());
        assert!(matches!(
            parsed.unwrap(),
            HitlMarker::CommandApproval { .. }
        ));
    }

    #[test]
    fn test_parse_no_marker() {
        let output = "Just a regular response without any markers.";
        assert!(parse_hitl_marker(output).is_none());
    }
}
