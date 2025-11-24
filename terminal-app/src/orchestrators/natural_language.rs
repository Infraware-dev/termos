/// Natural language query orchestrator
///
/// This orchestrator is responsible for:
/// - Querying the LLM backend
/// - Rendering the LLM response
/// - Displaying formatted results
use anyhow::Result;
use std::sync::Arc;

use crate::llm::{LLMClientTrait, ResponseRenderer};
use crate::terminal::{TerminalState, TerminalUI};
use crate::utils::MessageFormatter;

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
    pub fn new(llm_client: Arc<dyn LLMClientTrait>, renderer: ResponseRenderer) -> Self {
        Self {
            llm_client,
            renderer,
        }
    }

    /// Handle natural language query with all the necessary logic
    ///
    /// This method encapsulates:
    /// - Showing "waiting" state
    /// - Querying the LLM
    /// - Rendering the response
    /// - Error handling
    pub async fn handle_query(
        &self,
        query: &str,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()> {
        // Show waiting message
        state.add_output(MessageFormatter::info("Querying AI assistant..."));

        // Render to show "waiting" state
        ui.render(state)?;

        // Query the LLM
        match self.llm_client.query(query).await {
            Ok(response) => {
                self.handle_success(response, state);
            }
            Err(e) => {
                self.handle_error(e, state);
            }
        }

        Ok(())
    }

    /// Handle successful LLM response
    fn handle_success(&self, response: String, state: &mut TerminalState) {
        // Remove the "Querying..." message
        state.output.pop();

        // Render the response with formatting
        let formatted_lines = self.renderer.render(&response);
        state.add_output_lines(formatted_lines);
    }

    /// Handle LLM query error
    fn handle_error(&self, error: anyhow::Error, state: &mut TerminalState) {
        // Remove the "Querying..." message
        state.output.pop();

        // Show error message
        state.add_output(MessageFormatter::error(format!(
            "Error querying LLM: {error}"
        )));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::llm::MockLLMClient;
    use crate::terminal::TerminalState;

    #[tokio::test]
    async fn test_handle_query_success() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        orchestrator.handle_success("Test response".to_string(), &mut state);

        // Should have output (the response)
        assert!(!state.output.lines().is_empty());
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Test response")));
    }

    #[tokio::test]
    async fn test_handle_query_error() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();
        let error = anyhow::anyhow!("Test error");

        orchestrator.handle_error(error, &mut state);

        // Should have error message
        assert!(!state.output.lines().is_empty());
        assert!(state.output.lines()[0].contains("Error querying LLM"));
        assert!(state.output.lines()[0].contains("Test error"));
    }

    #[test]
    fn test_orchestrator_new() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let _ = NaturalLanguageOrchestrator::new(llm_client, renderer);
    }

    #[test]
    fn test_orchestrator_debug() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);
        let debug_str = format!("{orchestrator:?}");
        assert!(debug_str.contains("NaturalLanguageOrchestrator"));
    }

    #[tokio::test]
    async fn test_handle_success_with_markdown() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        // Test with markdown response
        orchestrator.handle_success("**Bold** text".to_string(), &mut state);

        assert!(!state.output.lines().is_empty());
    }

    #[tokio::test]
    async fn test_handle_success_removes_waiting_message() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();
        state.add_output("Querying AI assistant...".to_string());

        orchestrator.handle_success("Response".to_string(), &mut state);

        // The "Querying..." message should be removed
        assert!(!state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Querying AI assistant")));
    }

    #[tokio::test]
    async fn test_handle_error_removes_waiting_message() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();
        state.add_output("Querying AI assistant...".to_string());

        let error = anyhow::anyhow!("Network error");
        orchestrator.handle_error(error, &mut state);

        // The "Querying..." message should be removed
        assert!(!state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Querying AI assistant") && !line.contains("Error")));
    }

    #[tokio::test]
    async fn test_handle_success_with_multiline_response() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        orchestrator.handle_success("Line 1\nLine 2\nLine 3".to_string(), &mut state);

        // Should have multiple lines of output
        assert!(state.output.lines().len() >= 3);
    }
}
