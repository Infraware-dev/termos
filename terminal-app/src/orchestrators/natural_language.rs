/// Natural language query orchestrator
///
/// This orchestrator is responsible for:
/// - Querying the LLM backend
/// - Rendering the LLM response
/// - Human-in-the-loop interactions helpers
use anyhow::Result;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::llm::{LLMClientTrait, LLMQueryResult, ResponseRenderer};

/// Orchestrates natural language query workflow
pub struct NaturalLanguageOrchestrator {
    llm_client: Arc<dyn LLMClientTrait>,
    renderer: ResponseRenderer,
}

impl std::fmt::Debug for NaturalLanguageOrchestrator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NaturalLanguageOrchestrator")
            .field("llm_client", &"<Arc<dyn LLMClientTrait>>")
            .field("renderer", &self.renderer)
            .finish()
    }
}

impl NaturalLanguageOrchestrator {
    /// Create a new natural language orchestrator
    pub fn new(llm_client: Arc<dyn LLMClientTrait>) -> Self {
        Self {
            llm_client,
            renderer: ResponseRenderer::new(),
        }
    }

    /// Query the LLM with cancellation support
    pub async fn query(
        &self,
        text: &str,
        cancel_token: CancellationToken,
    ) -> Result<LLMQueryResult> {
        self.llm_client.query_cancellable(text, cancel_token).await
    }

    /// Resume an interrupted run (e.g. after command approval).
    /// Reserved for future multi-turn HITL flow where LLM continues after command execution.
    #[allow(dead_code)]
    pub async fn resume_run(&self) -> Result<LLMQueryResult> {
        self.llm_client.resume_run().await
    }

    /// Resume with an answer to a question
    pub async fn resume_with_answer(&self, answer: &str) -> Result<LLMQueryResult> {
        self.llm_client.resume_with_answer(answer).await
    }

    /// Render the response text (markdown -> ANSI)
    pub fn render_response(&self, text: &str) -> Vec<String> {
        self.renderer.render(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::MockLLMClient;

    fn create_orchestrator() -> NaturalLanguageOrchestrator {
        NaturalLanguageOrchestrator::new(Arc::new(MockLLMClient::new()))
    }

    #[test]
    fn test_orchestrator_creation() {
        let orchestrator = create_orchestrator();
        let debug_str = format!("{:?}", orchestrator);
        assert!(debug_str.contains("NaturalLanguageOrchestrator"));
    }

    #[test]
    fn test_render_response_plain_text() {
        let orchestrator = create_orchestrator();
        let lines = orchestrator.render_response("Hello, world!");
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("Hello, world!"));
    }

    #[test]
    fn test_render_response_multiline() {
        let orchestrator = create_orchestrator();
        let lines = orchestrator.render_response("Line 1\nLine 2\nLine 3");
        assert_eq!(lines.len(), 3);
        assert!(lines[0].contains("Line 1"));
        assert!(lines[1].contains("Line 2"));
        assert!(lines[2].contains("Line 3"));
    }

    #[test]
    fn test_render_response_with_bold() {
        let orchestrator = create_orchestrator();
        let lines = orchestrator.render_response("This is **bold** text");
        assert_eq!(lines.len(), 1);
        // Bold should be rendered with ANSI escape codes
        assert!(lines[0].contains("\x1b[1m")); // Bold start
        assert!(lines[0].contains("bold"));
    }

    #[test]
    fn test_render_response_with_inline_code() {
        let orchestrator = create_orchestrator();
        let lines = orchestrator.render_response("Use `ls -la` command");
        assert_eq!(lines.len(), 1);
        // Inline code should be rendered with cyan color
        assert!(lines[0].contains("\x1b[36m")); // Cyan
        assert!(lines[0].contains("ls -la"));
    }

    #[test]
    fn test_render_response_with_code_block() {
        let orchestrator = create_orchestrator();
        let text = "Example:\n```bash\nls -la\necho hello\n```\nDone.";
        let lines = orchestrator.render_response(text);

        // Should have: Example, code block header, 2 code lines, code block footer, Done
        assert!(lines.len() >= 4);

        // First line should be "Example:"
        assert!(lines[0].contains("Example:"));

        // Should contain code block markers
        let joined = lines.join("\n");
        assert!(joined.contains("bash")); // Language marker

        // Code lines should have the pipe character prefix (syntect adds escape codes to content)
        let code_lines: Vec<_> = lines.iter().filter(|l| l.contains("│")).collect();
        assert_eq!(code_lines.len(), 2);
    }

    #[tokio::test]
    async fn test_query_returns_result() {
        let orchestrator = create_orchestrator();
        let cancel_token = CancellationToken::new();

        let result = orchestrator.query("list files", cancel_token).await;
        assert!(result.is_ok());

        match result.unwrap() {
            LLMQueryResult::Complete(text) => {
                assert!(text.contains("ls"));
            }
            _ => panic!("Expected Complete result"),
        }
    }

    #[tokio::test]
    async fn test_query_docker() {
        let orchestrator = create_orchestrator();
        let cancel_token = CancellationToken::new();

        let result = orchestrator.query("docker commands", cancel_token).await;
        assert!(result.is_ok());

        match result.unwrap() {
            LLMQueryResult::Complete(text) => {
                assert!(text.contains("docker"));
            }
            _ => panic!("Expected Complete result"),
        }
    }

    #[tokio::test]
    async fn test_query_unknown_returns_default() {
        let orchestrator = create_orchestrator();
        let cancel_token = CancellationToken::new();

        let result = orchestrator
            .query("something random xyz", cancel_token)
            .await;
        assert!(result.is_ok());

        // MockLLMClient returns default response for unknown queries
        match result.unwrap() {
            LLMQueryResult::Complete(text) => {
                assert!(text.contains("mock LLM"));
            }
            _ => panic!("Expected Complete result"),
        }
    }
}
