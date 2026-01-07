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

    /// Resume an interrupted run (e.g. after command approval)
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
