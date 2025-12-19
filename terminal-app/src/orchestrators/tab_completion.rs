/// Tab completion handler
///
/// This handler is responsible for:
/// - Getting tab completions from TabCompletion service
/// - Auto-completing when single match
/// - Showing multiple completions
/// - Finding common prefix for partial completion
use crate::executor::TabCompletion;
use crate::terminal::TerminalState;
use crate::utils::MessageFormatter;

/// Handles tab completion workflow
#[derive(Debug, Default)]
pub struct TabCompletionHandler;

impl TabCompletionHandler {
    /// Create a new tab completion handler
    pub const fn new() -> Self {
        Self
    }

    /// Handle tab completion for the current input
    ///
    /// This method encapsulates:
    /// - Getting completions from TabCompletion service
    /// - Auto-completing single matches
    /// - Displaying multiple matches
    /// - Finding common prefix for partial completion
    pub fn handle_tab_completion(&self, state: &mut TerminalState) {
        let input = state.input.text().to_string();
        let completions = TabCompletion::get_completions(&input);

        if completions.is_empty() {
            return;
        }

        if completions.len() == 1 {
            self.handle_single_completion(&completions[0], state);
        } else {
            self.handle_multiple_completions(&completions, &input, state);
        }
    }

    /// Handle case when there's exactly one completion
    fn handle_single_completion(&self, completion: &str, state: &mut TerminalState) {
        // Single completion - auto-complete
        state.input.set_text(completion.to_string());
    }

    /// Handle case when there are multiple completions
    fn handle_multiple_completions(
        &self,
        completions: &[String],
        input: &str,
        state: &mut TerminalState,
    ) {
        // Show all completions
        state.add_output(MessageFormatter::info("Possible completions:"));
        for completion in completions {
            state.add_output(format!("  {completion}"));
        }

        // Auto-complete to common prefix
        let common_prefix = TabCompletion::get_common_prefix(completions);
        if common_prefix.len() > input.len() {
            state.input.set_text(common_prefix);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_completion() {
        let handler = TabCompletionHandler::new();
        let mut state = TerminalState::new();
        state.input.set_text("test".to_string());

        handler.handle_single_completion("test_completion", &mut state);

        assert_eq!(state.input.text(), "test_completion");
        assert_eq!(state.input.cursor_position(), "test_completion".len());
    }

    #[test]
    fn test_multiple_completions() {
        let handler = TabCompletionHandler::new();
        let mut state = TerminalState::new();
        state.input.set_text("tes".to_string());

        let completions = vec!["test1".to_string(), "test2".to_string()];

        handler.handle_multiple_completions(&completions, "tes", &mut state);

        // Should have added completion messages
        assert!(!state.output.lines().is_empty());
        assert!(state.output.lines()[0].contains("Possible completions"));
    }
}
