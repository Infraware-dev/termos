//! Ask user tool for rig-rs
//!
//! This module implements a rig-rs Tool for asking the user questions
//! when the LLM needs clarification or additional information.
//!
//! Note: These tools are currently not integrated with rig-rs's automatic
//! tool execution due to complex type handling in rig 0.31. They are kept
//! as reference implementations for future integration.

// Ask user tool integrated with rig-rs native function calling

use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::{JsonSchema, schema_for};
use serde::{Deserialize, Serialize};

/// Arguments for the ask user tool
#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct AskUserArgs {
    /// The question to ask the user
    #[schemars(description = "The question to ask the user")]
    pub question: String,

    /// Optional list of predefined choices for the user to select from
    #[schemars(
        description = "Optional list of predefined answer choices. If provided, the user can select from these options."
    )]
    pub options: Option<Vec<String>>,
}

/// Result of asking the user a question
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum AskUserResult {
    /// Question is pending user response
    Pending {
        /// The question that was asked
        question: String,
        /// Optional choices provided
        options: Option<Vec<String>>,
    },
    /// User has answered the question
    Answered {
        /// The original question
        question: String,
        /// The user's answer
        answer: String,
    },
}

/// Maximum length for a question
const MAX_QUESTION_LENGTH: usize = 500;

/// Error type for ask user tool
#[derive(Debug, thiserror::Error)]
pub enum AskUserError {
    /// Question was empty
    #[error("Question cannot be empty")]
    EmptyQuestion,

    /// Question exceeds maximum length
    #[error("Question too long: {0} characters (max {MAX_QUESTION_LENGTH})")]
    QuestionTooLong(usize),

    /// Question contains invalid control characters
    #[error("Question contains invalid control characters")]
    InvalidCharacters,
}

/// Tool for asking the user questions
///
/// This tool allows the LLM to request information from the user
/// when it needs clarification or additional context to complete a task.
/// The tool returns a `Pending` status, which the orchestrator converts
/// into an interrupt event.
#[derive(Debug, Clone, Default)]
pub struct AskUserTool;

impl AskUserTool {
    /// Create a new ask user tool
    pub fn new() -> Self {
        Self
    }
}

impl Tool for AskUserTool {
    const NAME: &'static str = "ask_user";

    type Error = AskUserError;
    type Args = AskUserArgs;
    type Output = AskUserResult;

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn definition(&self, _prompt: String) -> impl Future<Output = ToolDefinition> + Send + Sync {
        async {
            ToolDefinition {
                name: Self::NAME.to_string(),
                description: "Ask the user a question when you need more information or \
                    clarification to complete a task. You can optionally provide a list of \
                    choices for the user to select from. Use this when you're unsure about \
                    preferences, need specific details, or want to confirm an action."
                    .to_string(),
                parameters: serde_json::to_value(schema_for!(AskUserArgs))
                    .expect("Failed to generate JSON schema for AskUserArgs"),
            }
        }
    }

    #[expect(
        clippy::manual_async_fn,
        reason = "rig-rs Tool trait requires impl Future return type"
    )]
    fn call(
        &self,
        args: Self::Args,
    ) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
        async move {
            let question = args.question.trim();

            // Validate: not empty
            if question.is_empty() {
                return Err(AskUserError::EmptyQuestion);
            }

            // Validate: length limit
            if question.len() > MAX_QUESTION_LENGTH {
                return Err(AskUserError::QuestionTooLong(question.len()));
            }

            // Validate: no control characters (except newlines)
            if question.chars().any(|c| c.is_control() && c != '\n') {
                return Err(AskUserError::InvalidCharacters);
            }

            Ok(AskUserResult::Pending {
                question: question.to_string(),
                options: args.options,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ask_user_returns_pending() {
        let tool = AskUserTool::new();
        let args = AskUserArgs {
            question: "Which environment?".to_string(),
            options: Some(vec!["dev".to_string(), "prod".to_string()]),
        };

        let result = tool.call(args).await.unwrap();

        match result {
            AskUserResult::Pending { question, options } => {
                assert_eq!(question, "Which environment?");
                assert!(options.is_some());
                assert_eq!(options.unwrap().len(), 2);
            }
            _ => panic!("Expected Pending"),
        }
    }

    #[tokio::test]
    async fn test_ask_user_without_options() {
        let tool = AskUserTool::new();
        let args = AskUserArgs {
            question: "What is the file path?".to_string(),
            options: None,
        };

        let result = tool.call(args).await.unwrap();

        match result {
            AskUserResult::Pending { question, options } => {
                assert_eq!(question, "What is the file path?");
                assert!(options.is_none());
            }
            _ => panic!("Expected Pending"),
        }
    }

    #[tokio::test]
    async fn test_ask_user_empty_question() {
        let tool = AskUserTool::new();
        let args = AskUserArgs {
            question: "   ".to_string(),
            options: None,
        };

        let result = tool.call(args).await;
        assert!(matches!(result, Err(AskUserError::EmptyQuestion)));
    }

    #[tokio::test]
    async fn test_tool_definition() {
        let tool = AskUserTool::new();
        let def = tool.definition(String::new()).await;

        assert_eq!(def.name, "ask_user");
        assert!(def.description.contains("question"));
        assert!(def.parameters.is_object());
    }

    #[test]
    fn test_ask_user_args_schema() {
        let schema = schema_for!(AskUserArgs);
        let json = serde_json::to_string_pretty(&schema).unwrap();
        assert!(json.contains("question"));
        assert!(json.contains("options"));
    }
}
