//! SSE event types for agent streaming

use serde::{Deserialize, Serialize};

use crate::models::{Message, MessageRole};

/// Events streamed from the agent during a run
///
/// # Examples
///
/// ```
/// use infraware_shared::{AgentEvent, Interrupt};
///
/// // Create different event types
/// let metadata = AgentEvent::metadata("run-123");
/// let error = AgentEvent::error("Something went wrong");
/// let end = AgentEvent::end();
///
/// // Create an interrupt event
/// let interrupt = Interrupt::command_approval("ls -la", "List files", false);
/// let update = AgentEvent::updates_with_interrupt(interrupt);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum AgentEvent {
    /// Run metadata (run_id, etc.)
    Metadata { run_id: String },
    /// Streaming message content
    Message(MessageEvent),
    /// State values update (includes all messages)
    Values { messages: Vec<Message> },
    /// State updates (may include interrupts)
    Updates {
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupts: Option<Vec<Interrupt>>,
    },
    /// Error occurred
    Error { message: String },
    /// Stream ended
    End,
}

impl AgentEvent {
    pub fn metadata(run_id: impl Into<String>) -> Self {
        Self::Metadata {
            run_id: run_id.into(),
        }
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::Error {
            message: message.into(),
        }
    }

    pub fn end() -> Self {
        Self::End
    }

    pub fn updates_with_interrupt(interrupt: Interrupt) -> Self {
        Self::Updates {
            interrupts: Some(vec![interrupt]),
        }
    }
}

/// A streaming message event
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageEvent {
    /// The message role
    pub role: MessageRole,
    /// The message content chunk
    pub content: String,
}

impl MessageEvent {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }
}

/// Human-in-the-loop interrupt types
///
/// # Examples
///
/// ```
/// use infraware_shared::Interrupt;
///
/// // Command requiring user approval (final command, no continuation needed)
/// let cmd = Interrupt::command_approval("ls -la", "List files", false);
///
/// // Command that needs continuation (agent will process output)
/// let cmd = Interrupt::command_approval("uname -s", "Detect OS", true);
///
/// // Question with predefined options
/// let question = Interrupt::question(
///     "Which environment?",
///     Some(vec!["development".into(), "production".into()])
/// );
///
/// // Open-ended question
/// let open = Interrupt::question("What should I name the file?", None);
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Interrupt {
    /// Request approval to execute a command
    CommandApproval {
        /// The command to execute
        command: String,
        /// Explanation of why this command is needed
        message: String,
        /// Whether the agent needs to process the output after execution.
        /// - false: Command output IS the answer (e.g., "list files" → ls output)
        /// - true: Command output is INPUT for agent to continue (e.g., detect OS → then give instructions)
        #[serde(default)]
        needs_continuation: bool,
    },
    /// Ask the user a question
    Question {
        /// The question text
        question: String,
        /// Optional predefined answer choices
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<Vec<String>>,
    },
}

impl Interrupt {
    /// Create a command approval interrupt.
    ///
    /// # Arguments
    /// * `command` - The shell command to execute
    /// * `message` - Explanation of why this command is needed
    /// * `needs_continuation` - Whether agent needs to process output after execution
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

    pub fn question(question: impl Into<String>, options: Option<Vec<String>>) -> Self {
        Self::Question {
            question: question.into(),
            options,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_event_metadata() {
        let event = AgentEvent::metadata("run-123");
        match event {
            AgentEvent::Metadata { run_id } => assert_eq!(run_id, "run-123"),
            _ => panic!("Expected Metadata event"),
        }
    }

    #[test]
    fn test_agent_event_error() {
        let event = AgentEvent::error("Something went wrong");
        match event {
            AgentEvent::Error { message } => assert_eq!(message, "Something went wrong"),
            _ => panic!("Expected Error event"),
        }
    }

    #[test]
    fn test_message_event() {
        let event = MessageEvent::assistant("Hello there");
        assert_eq!(event.role, MessageRole::Assistant);
        assert_eq!(event.content, "Hello there");
    }

    #[test]
    fn test_interrupt_command_approval() {
        let interrupt = Interrupt::command_approval("rm -rf temp/", "Clean up temp files", false);
        match interrupt {
            Interrupt::CommandApproval {
                command,
                message,
                needs_continuation,
            } => {
                assert_eq!(command, "rm -rf temp/");
                assert_eq!(message, "Clean up temp files");
                assert!(!needs_continuation);
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_interrupt_command_approval_with_continuation() {
        let interrupt = Interrupt::command_approval("uname -s", "Detect OS", true);
        match interrupt {
            Interrupt::CommandApproval {
                needs_continuation, ..
            } => {
                assert!(needs_continuation);
            }
            _ => panic!("Expected CommandApproval"),
        }
    }

    #[test]
    fn test_interrupt_question() {
        let interrupt = Interrupt::question(
            "Which environment?",
            Some(vec!["dev".into(), "prod".into()]),
        );
        match interrupt {
            Interrupt::Question { question, options } => {
                assert_eq!(question, "Which environment?");
                assert_eq!(options, Some(vec!["dev".into(), "prod".into()]));
            }
            _ => panic!("Expected Question"),
        }
    }

    #[test]
    fn test_serde_roundtrip() {
        let event =
            AgentEvent::updates_with_interrupt(Interrupt::command_approval("ls", "List", false));
        let json = serde_json::to_string(&event).unwrap();
        let parsed: AgentEvent = serde_json::from_str(&json).unwrap();

        match parsed {
            AgentEvent::Updates { interrupts } => {
                assert!(interrupts.is_some());
                assert_eq!(interrupts.unwrap().len(), 1);
            }
            _ => panic!("Expected Updates event"),
        }
    }
}
