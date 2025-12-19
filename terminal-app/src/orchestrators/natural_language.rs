/// Natural language query orchestrator
///
/// This orchestrator is responsible for:
/// - Querying the LLM backend
/// - Rendering the LLM response
/// - Displaying formatted results
/// - Human-in-the-loop interactions (command approval and questions)
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

/// Render interval for throbber animation during LLM queries (~10 FPS)
///
/// Why 100ms:
/// - Provides smooth visual feedback (10 FPS is fluid for simple loading animations)
/// - Balances responsiveness vs CPU overhead during potentially long LLM queries
/// - Consistent with `ANIMATION_INTERVAL_MS` in `throbber.rs`
///
/// Trade-offs: Higher values reduce CPU usage but make animation choppier.
/// Lower values give smoother animation but increase render overhead.
const RENDER_INTERVAL_MS: u64 = 100;

use crate::llm::{LLMClientTrait, LLMQueryResult, ResponseRenderer};
use crate::terminal::{PendingInteraction, TerminalMode, TerminalState, TerminalUI};
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

    /// Update the LLM client (used for deferred initialization)
    pub fn set_llm_client(&mut self, client: Arc<dyn LLMClientTrait>) {
        self.llm_client = client;
    }

    /// Handle natural language query with all the necessary logic
    ///
    /// This method encapsulates:
    /// - Showing "waiting" state with animated throbber
    /// - Querying the LLM
    /// - Rendering the response
    /// - Human-in-the-loop command approval flow
    /// - Error handling
    /// - Cancellation support (via CancellationToken)
    ///
    /// The render loop runs at ~10 FPS (100ms intervals) to show the throbber animation
    /// while waiting for the LLM response.
    pub async fn handle_query(
        &self,
        query: &str,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        log::info!("Orchestrator handling query: {}", query);

        // Initial render to show throbber animation in prompt
        ui.render(state)?;

        log::info!("Calling LLM client...");

        // Pin the future so we can poll it multiple times in the render loop
        let mut llm_future = std::pin::pin!(self
            .llm_client
            .query_cancellable(query, cancel_token.clone()));

        // Render loop with 100ms timeout for ~10 FPS throbber animation
        // This ensures the throbber is visible during LLM queries
        loop {
            tokio::select! {
                // Prioritize completion and cancellation over render timeout
                biased;

                result = &mut llm_future => {
                    match result {
                        Ok(llm_result) => {
                            log::debug!("LLM query completed successfully");
                            self.handle_query_result(llm_result, state);
                            state.stop_throbber();
                        }
                        Err(e) if e.to_string().contains("cancelled") => {
                            log::info!("LLM query cancelled by user");
                            state.add_output(MessageFormatter::info("Query cancelled by user"));
                            state.mode = TerminalMode::Normal;
                            state.stop_throbber();
                        }
                        Err(e) => {
                            log::error!("LLM query failed: {}", e);
                            self.handle_error(e, state);
                            state.stop_throbber();
                        }
                    }
                    break; // Exit loop after LLM completes
                }
                _ = cancel_token.cancelled() => {
                    log::info!("LLM query cancelled via token");
                    state.add_output(MessageFormatter::info("Query cancelled by user"));
                    state.mode = TerminalMode::Normal;
                    state.stop_throbber();
                    break; // Exit loop on cancellation
                }
                _ = tokio::time::sleep(Duration::from_millis(RENDER_INTERVAL_MS)) => {
                    // Periodic render for throbber animation
                    ui.render(state)?;
                }
            }
        }

        // Final render to ensure UI reflects stopped throbber immediately
        ui.render(state)?;

        Ok(())
    }

    /// Handle the result of an LLM query (complete, command approval, or question)
    fn handle_query_result(&self, result: LLMQueryResult, state: &mut TerminalState) {
        match result {
            LLMQueryResult::Complete(response) => {
                // Render the response with formatting
                let formatted_lines = self.renderer.render(&response);
                state.add_output_lines(formatted_lines);
                // Return to normal mode after complete response
                state.mode = TerminalMode::Normal;
            }
            LLMQueryResult::CommandApproval { command, message } => {
                // Human-in-the-loop: show command for approval
                // The prompt "Do you want to execute this command (y/n)?" is shown by tui.rs

                // Save pending interaction and change mode
                // confirmation_type is None for LLM-originated approvals
                state.pending_interaction = Some(PendingInteraction::CommandApproval {
                    command: command.clone(),
                    message,
                    confirmation_type: None,
                });
                state.mode = TerminalMode::AwaitingCommandApproval;

                log::info!("Awaiting user approval for command: {}", command);
            }
            LLMQueryResult::Question { question, options } => {
                // Human-in-the-loop: show question from agent
                state.add_output(String::new());
                state.add_output(MessageFormatter::info("Agent question:"));
                state.add_output(format!("  {}", question));

                if let Some(ref opts) = options {
                    state.add_output("  Options:".to_string());
                    for (i, opt) in opts.iter().enumerate() {
                        state.add_output(format!("    {}. {}", i + 1, opt));
                    }
                }
                state.add_output(String::new());
                state.add_output("Type your answer:".to_string());

                // Save pending interaction and change mode
                state.pending_interaction = Some(PendingInteraction::Question {
                    question: question.clone(),
                    options,
                });
                state.mode = TerminalMode::AwaitingAnswer;

                log::info!("Awaiting user answer for question: {}", question);
            }
        }
    }

    /// Handle user approval/rejection of a pending command
    ///
    /// Called when user submits 'y' or 'n' while in AwaitingCommandApproval mode
    pub async fn handle_approval(
        &self,
        approved: bool,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()> {
        // Clear pending interaction
        let pending = state.pending_interaction.take();

        if approved {
            if let Some(PendingInteraction::CommandApproval { ref command, .. }) = pending {
                log::info!("User approved command: {}", command);
            }

            // Show throbber animation while waiting for LLM response
            state.mode = TerminalMode::WaitingLLM;
            state.start_throbber();
            ui.render(state)?;

            // Pin the future so we can poll it multiple times in the render loop
            let mut resume_future = std::pin::pin!(self.llm_client.resume_run());

            // Render loop with 100ms timeout for ~10 FPS throbber animation
            loop {
                tokio::select! {
                    biased;

                    result = &mut resume_future => {
                        match result {
                            Ok(llm_result) => {
                                state.stop_throbber();
                                self.handle_query_result(llm_result, state);
                            }
                            Err(e) => {
                                state.stop_throbber();
                                state.add_output(MessageFormatter::error(format!("Failed to resume: {e}")));
                                state.mode = TerminalMode::Normal;
                            }
                        }
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_millis(RENDER_INTERVAL_MS)) => {
                        // Periodic render for throbber animation
                        ui.render(state)?;
                    }
                }
            }

            // Final render to ensure UI reflects stopped throbber immediately
            ui.render(state)?;
        } else {
            if let Some(PendingInteraction::CommandApproval { ref command, .. }) = pending {
                log::info!("User rejected command: {}", command);
            }
            state.mode = TerminalMode::Normal;
        }

        // If we're not in another waiting state, return to Normal
        if !state.is_in_hitl_mode() {
            state.mode = TerminalMode::Normal;
        }

        Ok(())
    }

    /// Handle user's text answer to a question from the agent
    ///
    /// Called when user submits text while in AwaitingAnswer mode
    pub async fn handle_answer(
        &self,
        answer: String,
        state: &mut TerminalState,
        ui: &mut TerminalUI,
    ) -> Result<()> {
        // Clear pending interaction
        let pending = state.pending_interaction.take();

        if let Some(PendingInteraction::Question { ref question, .. }) = pending {
            log::info!("User answered question '{}' with: {}", question, answer);
        }

        state.add_output(MessageFormatter::info(format!("Your answer: {}", answer)));
        state.add_output(MessageFormatter::info("Sending to agent..."));
        ui.render(state)?;

        // Resume the LLM run with the user's answer
        match self.llm_client.resume_with_answer(&answer).await {
            Ok(result) => {
                self.handle_query_result(result, state);
            }
            Err(e) => {
                state.add_output(MessageFormatter::error(format!(
                    "Failed to send answer: {e}"
                )));
                state.mode = TerminalMode::Normal;
            }
        }

        // If we're not in another waiting state, return to Normal
        if !state.is_in_hitl_mode() {
            state.mode = TerminalMode::Normal;
        }

        Ok(())
    }

    /// Handle successful LLM response (legacy helper for tests)
    #[cfg(test)]
    fn handle_success(&self, response: String, state: &mut TerminalState) {
        // Remove the "Querying..." message
        state.output.pop();

        // Render the response with formatting
        let formatted_lines = self.renderer.render(&response);
        state.add_output_lines(formatted_lines);
    }

    /// Handle LLM query error
    fn handle_error(&self, error: anyhow::Error, state: &mut TerminalState) {
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
    async fn test_handle_success_adds_response() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        orchestrator.handle_success("Response".to_string(), &mut state);

        // Response should be in output
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Response")));
    }

    #[tokio::test]
    async fn test_handle_error_adds_error_message() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        let error = anyhow::anyhow!("Network error");
        orchestrator.handle_error(error, &mut state);

        // Error message should be in output
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Error") && line.contains("Network error")));
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

    #[test]
    fn test_handle_query_result_complete() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        let result = LLMQueryResult::Complete("Test response from LLM".to_string());
        orchestrator.handle_query_result(result, &mut state);

        // Response should be in output
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Test response from LLM")));
    }

    #[test]
    fn test_handle_query_result_command_approval() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        let result = LLMQueryResult::CommandApproval {
            command: "rm -rf /tmp/test".to_string(),
            message: "Delete test files".to_string(),
        };
        orchestrator.handle_query_result(result, &mut state);

        // Mode should change to await approval
        assert_eq!(state.mode, TerminalMode::AwaitingCommandApproval);

        // Pending interaction should be set with correct command and message
        // (The actual display is handled by TUI rendering, not output buffer)
        match &state.pending_interaction {
            Some(PendingInteraction::CommandApproval {
                command, message, ..
            }) => {
                assert_eq!(command, "rm -rf /tmp/test");
                assert_eq!(message, "Delete test files");
            }
            _ => panic!("Expected CommandApproval pending interaction"),
        }
    }

    #[test]
    fn test_handle_query_result_question() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        let result = LLMQueryResult::Question {
            question: "Which database would you prefer?".to_string(),
            options: Some(vec![
                "PostgreSQL".to_string(),
                "MySQL".to_string(),
                "SQLite".to_string(),
            ]),
        };
        orchestrator.handle_query_result(result, &mut state);

        // Should show question
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Agent question")));
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("Which database would you prefer")));

        // Should show options
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("PostgreSQL")));

        // Mode should change
        assert_eq!(state.mode, TerminalMode::AwaitingAnswer);

        // Pending interaction should be set
        assert!(matches!(
            state.pending_interaction,
            Some(PendingInteraction::Question { .. })
        ));
    }

    #[test]
    fn test_handle_query_result_question_no_options() {
        let llm_client = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let orchestrator = NaturalLanguageOrchestrator::new(llm_client, renderer);

        let mut state = TerminalState::new();

        let result = LLMQueryResult::Question {
            question: "What is your project name?".to_string(),
            options: None,
        };
        orchestrator.handle_query_result(result, &mut state);

        // Should show question
        assert!(state
            .output
            .lines()
            .iter()
            .any(|line| line.contains("What is your project name")));

        // Mode should change
        assert_eq!(state.mode, TerminalMode::AwaitingAnswer);
    }

    #[test]
    fn test_set_llm_client() {
        let llm_client1 = Arc::new(MockLLMClient::new());
        let llm_client2 = Arc::new(MockLLMClient::new());
        let renderer = ResponseRenderer::new();
        let mut orchestrator = NaturalLanguageOrchestrator::new(llm_client1, renderer);

        // Should be able to update the LLM client
        orchestrator.set_llm_client(llm_client2);

        // Just verify it doesn't panic
        let debug_str = format!("{orchestrator:?}");
        assert!(debug_str.contains("NaturalLanguageOrchestrator"));
    }
}
