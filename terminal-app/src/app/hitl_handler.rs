//! Human-in-the-loop interaction handling.
//!
//! Provides `HitlHandler` for processing keyboard input during HITL interactions
//! (command approval and question answering). This module is fully testable.

use crate::input::KeyboardAction;
use crate::state::AppMode;

/// Actions resulting from HITL keyboard processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitlAction {
    /// Echo bytes to the terminal display
    Echo(Vec<u8>),
    /// Submit the current input
    Submit(HitlSubmission),
    /// Cancel the HITL interaction
    Cancel,
    /// Handle backspace (erase character)
    Backspace,
    /// No action needed
    None,
}

/// Submission types for HITL interactions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HitlSubmission {
    /// User responded to command approval (y/n)
    Approval {
        /// The command being approved/rejected
        command: String,
        /// Whether the command was approved
        approved: bool,
    },
    /// User provided an answer to a question
    Answer {
        /// The user's answer text
        answer: String,
    },
}

/// Handles HITL keyboard input processing.
///
/// Provides static methods for processing keyboard actions during
/// AwaitingApproval and AwaitingAnswer modes.
pub struct HitlHandler;

impl HitlHandler {
    /// Processes a keyboard action in HITL mode.
    ///
    /// Returns the appropriate HITL action based on the input and current mode.
    pub fn process_keyboard_action(
        action: KeyboardAction,
        input_buffer: &mut String,
        mode: &AppMode,
    ) -> HitlAction {
        match action {
            KeyboardAction::SendBytes(bytes) => Self::process_bytes(&bytes, input_buffer, mode),
            KeyboardAction::SendSigInt => {
                input_buffer.clear();
                HitlAction::Cancel
            }
            _ => HitlAction::None,
        }
    }

    /// Processes byte input in HITL mode.
    fn process_bytes(bytes: &[u8], input_buffer: &mut String, mode: &AppMode) -> HitlAction {
        let text = String::from_utf8_lossy(bytes);

        for c in text.chars() {
            if c == '\r' || c == '\n' {
                // Submit the input
                let input = std::mem::take(input_buffer);
                return Self::create_submission(input, mode);
            } else if c == '\x7f' || c == '\x08' {
                // Backspace
                if input_buffer.pop().is_some() {
                    return HitlAction::Backspace;
                }
            } else if !c.is_control() {
                // Echo character and add to buffer
                input_buffer.push(c);
                return HitlAction::Echo(c.to_string().into_bytes());
            }
        }

        HitlAction::None
    }

    /// Creates a submission based on the current mode.
    fn create_submission(input: String, mode: &AppMode) -> HitlAction {
        match mode {
            AppMode::AwaitingApproval { command, .. } => {
                let approved = crate::orchestrators::parse_approval(&input);
                HitlAction::Submit(HitlSubmission::Approval {
                    command: command.clone(),
                    approved,
                })
            }
            AppMode::AwaitingAnswer { .. } => {
                HitlAction::Submit(HitlSubmission::Answer { answer: input })
            }
            _ => {
                // If we're not in a HITL mode, just echo the newline
                HitlAction::Echo(b"\r\n".to_vec())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_regular_char() {
        let mut buffer = String::new();
        let mode = AppMode::AwaitingAnswer {
            question: "Test?".to_string(),
            options: None,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"a".to_vec()),
            &mut buffer,
            &mode,
        );

        assert_eq!(buffer, "a");
        assert!(matches!(action, HitlAction::Echo(_)));
    }

    #[test]
    fn test_process_backspace() {
        let mut buffer = "ab".to_string();
        let mode = AppMode::AwaitingAnswer {
            question: "Test?".to_string(),
            options: None,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"\x7f".to_vec()),
            &mut buffer,
            &mode,
        );

        assert_eq!(buffer, "a");
        assert_eq!(action, HitlAction::Backspace);
    }

    #[test]
    fn test_process_backspace_empty_buffer() {
        let mut buffer = String::new();
        let mode = AppMode::AwaitingAnswer {
            question: "Test?".to_string(),
            options: None,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"\x7f".to_vec()),
            &mut buffer,
            &mode,
        );

        assert!(buffer.is_empty());
        assert_eq!(action, HitlAction::None);
    }

    #[test]
    fn test_process_sigint_cancels() {
        let mut buffer = "some input".to_string();
        let mode = AppMode::AwaitingAnswer {
            question: "Test?".to_string(),
            options: None,
        };

        let action =
            HitlHandler::process_keyboard_action(KeyboardAction::SendSigInt, &mut buffer, &mode);

        assert!(buffer.is_empty());
        assert_eq!(action, HitlAction::Cancel);
    }

    #[test]
    fn test_submit_approval_yes() {
        let mut buffer = "y".to_string();
        let mode = AppMode::AwaitingApproval {
            command: "ls -la".to_string(),
            message: "Run this?".to_string(),
            needs_continuation: false,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"\r".to_vec()),
            &mut buffer,
            &mode,
        );

        match action {
            HitlAction::Submit(HitlSubmission::Approval { command, approved }) => {
                assert_eq!(command, "ls -la");
                assert!(approved);
            }
            _ => panic!("Expected Approval submission"),
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_submit_approval_no() {
        let mut buffer = "n".to_string();
        let mode = AppMode::AwaitingApproval {
            command: "rm -rf /".to_string(),
            message: "Run this?".to_string(),
            needs_continuation: false,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"\r".to_vec()),
            &mut buffer,
            &mode,
        );

        match action {
            HitlAction::Submit(HitlSubmission::Approval { command, approved }) => {
                assert_eq!(command, "rm -rf /");
                assert!(!approved);
            }
            _ => panic!("Expected Approval submission"),
        }
    }

    #[test]
    fn test_submit_answer() {
        let mut buffer = "my answer".to_string();
        let mode = AppMode::AwaitingAnswer {
            question: "What is your name?".to_string(),
            options: None,
        };

        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(b"\r".to_vec()),
            &mut buffer,
            &mode,
        );

        match action {
            HitlAction::Submit(HitlSubmission::Answer { answer }) => {
                assert_eq!(answer, "my answer");
            }
            _ => panic!("Expected Answer submission"),
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_control_chars_ignored() {
        let mut buffer = String::new();
        let mode = AppMode::AwaitingAnswer {
            question: "Test?".to_string(),
            options: None,
        };

        // Ctrl+A is 0x01
        let action = HitlHandler::process_keyboard_action(
            KeyboardAction::SendBytes(vec![0x01]),
            &mut buffer,
            &mode,
        );

        assert!(buffer.is_empty());
        assert_eq!(action, HitlAction::None);
    }

    #[test]
    fn test_parse_approval_variants() {
        use crate::orchestrators::parse_approval;

        // Yes variants
        assert!(parse_approval("y"));
        assert!(parse_approval("Y"));
        assert!(parse_approval("yes"));
        assert!(parse_approval("YES"));
        assert!(parse_approval("Yes"));
        // Empty string = approve (Enter key, like Python backend)
        assert!(parse_approval(""));

        // No variants
        assert!(!parse_approval("n"));
        assert!(!parse_approval("N"));
        assert!(!parse_approval("no"));
        assert!(!parse_approval("NO"));
        assert!(!parse_approval("maybe"));
    }
}
