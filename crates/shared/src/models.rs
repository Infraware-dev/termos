//! Core model types for LLM interactions

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Maximum length for thread IDs
pub const MAX_THREAD_ID_LENGTH: usize = 256;

/// Validation error for thread IDs
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ThreadIdError {
    #[error("thread_id cannot be empty")]
    Empty,
    #[error("thread_id too long (max {MAX_THREAD_ID_LENGTH} characters)")]
    TooLong,
    #[error("thread_id contains invalid characters (allowed: alphanumeric, dash, underscore)")]
    InvalidCharacters,
}

/// Thread identifier for conversation continuity
///
/// # Examples
///
/// ```
/// use infraware_shared::ThreadId;
///
/// // Create without validation (for internal use)
/// let id = ThreadId::new("my-thread-123");
/// assert_eq!(id.as_str(), "my-thread-123");
///
/// // Create with validation
/// let validated = ThreadId::try_new("valid_id").expect("valid id");
///
/// // Invalid IDs are rejected
/// assert!(ThreadId::try_new("").is_err()); // empty
/// assert!(ThreadId::try_new("has space").is_err()); // invalid chars
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ThreadId(pub String);

impl ThreadId {
    /// Create a new ThreadId without validation (for deserialization and internal use)
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// Create a new ThreadId with validation
    pub fn try_new(id: impl Into<String>) -> Result<Self, ThreadIdError> {
        let id = id.into();
        Self::validate_str(&id)?;
        Ok(Self(id))
    }

    /// Validate a thread ID string
    pub fn validate_str(id: &str) -> Result<(), ThreadIdError> {
        if id.is_empty() {
            return Err(ThreadIdError::Empty);
        }
        if id.len() > MAX_THREAD_ID_LENGTH {
            return Err(ThreadIdError::TooLong);
        }
        if !id
            .chars()
            .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ThreadIdError::InvalidCharacters);
        }
        Ok(())
    }

    /// Validate this thread ID
    pub fn validate(&self) -> Result<(), ThreadIdError> {
        Self::validate_str(&self.0)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ThreadId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<String> for ThreadId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for ThreadId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

/// Result of an LLM query - complete, command approval, or question
///
/// # Examples
///
/// ```
/// use infraware_shared::LLMQueryResult;
///
/// // Complete response
/// let complete = LLMQueryResult::complete("The answer is 42");
/// assert!(complete.is_complete());
/// assert_eq!(complete.as_complete(), Some("The answer is 42"));
///
/// // Command that needs approval
/// let cmd = LLMQueryResult::command_approval("rm -rf /tmp/cache", "Clean cache");
/// assert!(cmd.is_interrupt());
///
/// // Question with options
/// let question = LLMQueryResult::question("Which option?", Some(vec!["A".into(), "B".into()]));
/// assert!(question.is_interrupt());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LLMQueryResult {
    /// Query completed with a final response
    Complete { response: String },
    /// Query interrupted - LLM wants to execute a command and needs approval (y/n)
    CommandApproval {
        /// The command the LLM wants to execute
        command: String,
        /// Description/reason from the LLM
        message: String,
    },
    /// Query interrupted - LLM is asking a question (free-form text answer)
    Question {
        /// The question being asked
        question: String,
        /// Optional predefined choices
        #[serde(skip_serializing_if = "Option::is_none")]
        options: Option<Vec<String>>,
    },
}

impl LLMQueryResult {
    /// Create a Complete result
    pub fn complete(response: impl Into<String>) -> Self {
        Self::Complete {
            response: response.into(),
        }
    }

    /// Create a CommandApproval result
    pub fn command_approval(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::CommandApproval {
            command: command.into(),
            message: message.into(),
        }
    }

    /// Create a Question result
    pub fn question(question: impl Into<String>, options: Option<Vec<String>>) -> Self {
        Self::Question {
            question: question.into(),
            options,
        }
    }

    /// Returns the response text if Complete, or None otherwise
    pub fn as_complete(&self) -> Option<&str> {
        match self {
            Self::Complete { response } => Some(response),
            Self::CommandApproval { .. } | Self::Question { .. } => None,
        }
    }

    /// Returns true if this is a Complete result
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete { .. })
    }

    /// Returns true if this is an interrupt (CommandApproval or Question)
    pub fn is_interrupt(&self) -> bool {
        !self.is_complete()
    }
}

/// Input for starting a run
///
/// # Examples
///
/// ```
/// use infraware_shared::{RunInput, Message};
///
/// // Single user message (most common case)
/// let input = RunInput::single_user_message("What is Rust?");
/// assert_eq!(input.messages.len(), 1);
///
/// // Multiple messages (conversation history)
/// let messages = vec![
///     Message::user("Hi"),
///     Message::assistant("Hello! How can I help?"),
///     Message::user("What is Rust?"),
/// ];
/// let input = RunInput::new(messages);
/// assert_eq!(input.messages.len(), 3);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunInput {
    pub messages: Vec<Message>,
}

impl RunInput {
    pub fn new(messages: Vec<Message>) -> Self {
        Self { messages }
    }

    pub fn single_user_message(content: impl Into<String>) -> Self {
        Self {
            messages: vec![Message::user(content)],
        }
    }
}

/// A chat message
///
/// # Examples
///
/// ```
/// use infraware_shared::{Message, MessageRole};
///
/// // Create messages with convenience methods
/// let user_msg = Message::user("Hello, how can I help?");
/// let assistant_msg = Message::assistant("I can help with coding tasks.");
/// let system_msg = Message::system("You are a helpful assistant.");
///
/// assert_eq!(user_msg.role, MessageRole::User);
/// assert_eq!(assistant_msg.content, "I can help with coding tasks.");
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: MessageRole,
    pub content: String,
}

impl Message {
    pub fn new(role: MessageRole, content: impl Into<String>) -> Self {
        Self {
            role,
            content: content.into(),
        }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self::new(MessageRole::User, content)
    }

    pub fn assistant(content: impl Into<String>) -> Self {
        Self::new(MessageRole::Assistant, content)
    }

    pub fn system(content: impl Into<String>) -> Self {
        Self::new(MessageRole::System, content)
    }
}

/// Role of a message sender
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_id() {
        let id = ThreadId::new("test-123");
        assert_eq!(id.as_str(), "test-123");
        assert_eq!(format!("{}", id), "test-123");
    }

    #[test]
    fn test_thread_id_validation_valid() {
        assert!(ThreadId::try_new("valid-id").is_ok());
        assert!(ThreadId::try_new("valid_id_123").is_ok());
        assert!(ThreadId::try_new("ValidMixedCase").is_ok());
    }

    #[test]
    fn test_thread_id_validation_empty() {
        assert_eq!(ThreadId::try_new(""), Err(ThreadIdError::Empty));
    }

    #[test]
    fn test_thread_id_validation_too_long() {
        let long_id = "a".repeat(MAX_THREAD_ID_LENGTH + 1);
        assert_eq!(ThreadId::try_new(long_id), Err(ThreadIdError::TooLong));
    }

    #[test]
    fn test_thread_id_validation_invalid_chars() {
        assert_eq!(
            ThreadId::try_new("invalid/id"),
            Err(ThreadIdError::InvalidCharacters)
        );
        assert_eq!(
            ThreadId::try_new("has space"),
            Err(ThreadIdError::InvalidCharacters)
        );
        assert_eq!(
            ThreadId::try_new("has@special"),
            Err(ThreadIdError::InvalidCharacters)
        );
    }

    #[test]
    fn test_llm_query_result_complete() {
        let result = LLMQueryResult::complete("Hello");
        assert!(result.is_complete());
        assert!(!result.is_interrupt());
        assert_eq!(result.as_complete(), Some("Hello"));
    }

    #[test]
    fn test_llm_query_result_command_approval() {
        let result = LLMQueryResult::command_approval("ls -la", "List files");
        assert!(!result.is_complete());
        assert!(result.is_interrupt());
        assert_eq!(result.as_complete(), None);
    }

    #[test]
    fn test_llm_query_result_question() {
        let result = LLMQueryResult::question("Which option?", Some(vec!["A".into(), "B".into()]));
        assert!(!result.is_complete());
        assert!(result.is_interrupt());
    }

    #[test]
    fn test_run_input() {
        let input = RunInput::single_user_message("Hello");
        assert_eq!(input.messages.len(), 1);
        assert_eq!(input.messages[0].content, "Hello");
    }

    #[test]
    fn test_message_roles() {
        let user = Message::user("Hi");
        let assistant = Message::assistant("Hello");
        let system = Message::system("You are helpful");

        assert_eq!(user.role, MessageRole::User);
        assert_eq!(assistant.role, MessageRole::Assistant);
        assert_eq!(system.role, MessageRole::System);
    }
}
