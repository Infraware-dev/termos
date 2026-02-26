//! Keyboard input processing and action classification.
//!
//! Provides `InputHandler` which transforms raw keyboard actions into high-level
//! application actions. This module is fully testable without egui dependencies.

use crate::input::{InputClassifier, InputType, KeyboardAction};

/// High-level actions resulting from keyboard input processing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputAction {
    /// Send bytes to PTY
    SendToPty(Vec<u8>),
    /// Start an LLM query with the given text
    StartLlmQuery(String),
    /// Cancel active LLM query (Ctrl+C)
    CancelLlm,
    /// Copy selection to clipboard
    Copy,
    /// Paste from clipboard
    Paste,
    /// Split pane horizontally
    SplitHorizontal,
    /// Split pane vertically
    SplitVertical,
    /// Create new tab
    NewTab,
    /// Close current tab
    CloseTab,
    /// Switch to next tab
    NextTab,
    /// Switch to previous tab
    PrevTab,
    /// Enter LLM query mode (Ctrl+?)
    EnterLlmMode,
}

/// Transforms keyboard actions into application actions.
///
/// Handles command buffer tracking and input classification (command vs NLP).
#[derive(Debug)]
pub struct InputHandler {
    /// Input classifier for command vs natural language detection
    input_classifier: InputClassifier,
}

impl InputHandler {
    /// Creates a new input handler.
    pub fn new() -> Self {
        Self {
            input_classifier: InputClassifier::new(),
        }
    }

    /// Processes keyboard actions and returns high-level application actions.
    ///
    /// Updates the command buffer and classifies input on Enter.
    pub fn process_actions(
        &mut self,
        actions: Vec<KeyboardAction>,
        command_buffer: &mut String,
    ) -> Vec<InputAction> {
        let mut result = Vec::with_capacity(actions.len());

        for action in actions {
            match action {
                KeyboardAction::SendBytes(bytes) => {
                    if let Some(input_action) = self.process_bytes(&bytes, command_buffer) {
                        result.push(input_action);
                    }
                }
                KeyboardAction::SendSigInt => {
                    result.push(InputAction::CancelLlm);
                }
                KeyboardAction::Copy => {
                    result.push(InputAction::Copy);
                }
                KeyboardAction::Paste => {
                    result.push(InputAction::Paste);
                }
                KeyboardAction::SplitHorizontal => {
                    result.push(InputAction::SplitHorizontal);
                }
                KeyboardAction::SplitVertical => {
                    result.push(InputAction::SplitVertical);
                }
                KeyboardAction::NewTab => {
                    result.push(InputAction::NewTab);
                }
                KeyboardAction::CloseTab => {
                    result.push(InputAction::CloseTab);
                }
                KeyboardAction::NextTab => {
                    result.push(InputAction::NextTab);
                }
                KeyboardAction::PrevTab => {
                    result.push(InputAction::PrevTab);
                }
                KeyboardAction::EnterLLMMode => {
                    result.push(InputAction::EnterLlmMode);
                }
            }
        }

        result
    }

    /// Updates the command buffer with pasted text.
    ///
    /// Filters control characters and appends printable text to the buffer.
    /// This should be called when handling paste operations to ensure pasted
    /// text is available for classification on Enter.
    pub fn update_buffer_with_pasted_text(&self, text: &str, command_buffer: &mut String) {
        for c in text.chars().filter(|c| !c.is_control()) {
            command_buffer.push(c);
        }
    }

    /// Processes byte input, updating command buffer and classifying on Enter.
    fn process_bytes(&mut self, bytes: &[u8], command_buffer: &mut String) -> Option<InputAction> {
        let text = String::from_utf8_lossy(bytes);

        for c in text.chars() {
            if c == '\r' || c == '\n' {
                // Classify the input before sending
                let input = command_buffer.trim().to_string();
                match self.input_classifier.classify(&input) {
                    InputType::NaturalLanguage(query) => {
                        tracing::info!("Input classified as NaturalLanguage: {}", query);
                        command_buffer.clear();
                        return Some(InputAction::StartLlmQuery(query));
                    }
                    InputType::Command(_) | InputType::Empty => {
                        command_buffer.clear();
                        // Fall through to send bytes to PTY
                    }
                }
            } else if c == '\x7f' || c == '\x08' {
                // Backspace
                command_buffer.pop();
            } else if !c.is_control() {
                command_buffer.push(c);
            }
        }

        // Return the bytes to send to PTY
        Some(InputAction::SendToPty(bytes.to_vec()))
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_simple_text() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(
            vec![KeyboardAction::SendBytes(b"hello".to_vec())],
            &mut buffer,
        );

        assert_eq!(actions.len(), 1);
        assert!(matches!(actions[0], InputAction::SendToPty(_)));
        assert_eq!(buffer, "hello");
    }

    #[test]
    fn test_process_backspace() {
        let mut handler = InputHandler::new();
        let mut buffer = "hell".to_string();

        let actions = handler.process_actions(
            vec![KeyboardAction::SendBytes(b"\x7f".to_vec())],
            &mut buffer,
        );

        assert_eq!(actions.len(), 1);
        assert_eq!(buffer, "hel");
    }

    #[test]
    fn test_process_enter_clears_buffer() {
        let mut handler = InputHandler::new();
        let mut buffer = "ls -la".to_string();

        let actions =
            handler.process_actions(vec![KeyboardAction::SendBytes(b"\r".to_vec())], &mut buffer);

        assert_eq!(actions.len(), 1);
        // Buffer should be cleared after Enter for regular commands
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_process_nlp_query() {
        let mut handler = InputHandler::new();
        let mut buffer = "? how do I list files".to_string();

        let actions =
            handler.process_actions(vec![KeyboardAction::SendBytes(b"\r".to_vec())], &mut buffer);

        assert_eq!(actions.len(), 1);
        match &actions[0] {
            InputAction::StartLlmQuery(query) => {
                assert!(query.contains("list files"));
            }
            _ => panic!("Expected StartLlmQuery"),
        }
        assert!(buffer.is_empty());
    }

    #[test]
    fn test_process_sigint() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(vec![KeyboardAction::SendSigInt], &mut buffer);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], InputAction::CancelLlm);
    }

    #[test]
    fn test_process_copy_paste() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(
            vec![KeyboardAction::Copy, KeyboardAction::Paste],
            &mut buffer,
        );

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], InputAction::Copy);
        assert_eq!(actions[1], InputAction::Paste);
    }

    #[test]
    fn test_process_tab_navigation() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(
            vec![
                KeyboardAction::NewTab,
                KeyboardAction::NextTab,
                KeyboardAction::PrevTab,
                KeyboardAction::CloseTab,
            ],
            &mut buffer,
        );

        assert_eq!(actions.len(), 4);
        assert_eq!(actions[0], InputAction::NewTab);
        assert_eq!(actions[1], InputAction::NextTab);
        assert_eq!(actions[2], InputAction::PrevTab);
        assert_eq!(actions[3], InputAction::CloseTab);
    }

    #[test]
    fn test_process_split() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(
            vec![
                KeyboardAction::SplitHorizontal,
                KeyboardAction::SplitVertical,
            ],
            &mut buffer,
        );

        assert_eq!(actions.len(), 2);
        assert_eq!(actions[0], InputAction::SplitHorizontal);
        assert_eq!(actions[1], InputAction::SplitVertical);
    }

    #[test]
    fn test_process_enter_llm_mode() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        let actions = handler.process_actions(vec![KeyboardAction::EnterLLMMode], &mut buffer);

        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0], InputAction::EnterLlmMode);
    }

    #[test]
    fn test_control_chars_not_added_to_buffer() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        // Ctrl+C is 0x03, should not be added to buffer
        handler.process_actions(vec![KeyboardAction::SendBytes(vec![0x03])], &mut buffer);

        assert!(buffer.is_empty());
    }

    #[test]
    fn test_update_buffer_with_pasted_text() {
        let handler = InputHandler::new();
        let mut buffer = String::new();

        handler.update_buffer_with_pasted_text("hello world", &mut buffer);

        assert_eq!(buffer, "hello world");
    }

    #[test]
    fn test_update_buffer_with_pasted_text_appends() {
        let handler = InputHandler::new();
        let mut buffer = "existing ".to_string();

        handler.update_buffer_with_pasted_text("pasted", &mut buffer);

        assert_eq!(buffer, "existing pasted");
    }

    #[test]
    fn test_pasted_nlp_query_classified_correctly() {
        let mut handler = InputHandler::new();
        let mut buffer = String::new();

        // Simulate pasting "? how do I list files"
        handler.update_buffer_with_pasted_text("? how do I list files", &mut buffer);

        // Now press Enter
        let actions =
            handler.process_actions(vec![KeyboardAction::SendBytes(b"\r".to_vec())], &mut buffer);

        // Should be classified as NLP query
        assert_eq!(actions.len(), 1);
        match &actions[0] {
            InputAction::StartLlmQuery(query) => {
                assert!(query.contains("list files"));
            }
            _ => panic!("Expected StartLlmQuery, got {:?}", actions[0]),
        }
    }

    #[test]
    fn test_pasted_text_filters_control_chars() {
        let handler = InputHandler::new();
        let mut buffer = String::new();

        // Pasted text with control characters (e.g., from bracketed paste)
        handler.update_buffer_with_pasted_text("hello\x03world", &mut buffer);

        // Control chars should be filtered
        assert_eq!(buffer, "helloworld");
    }
}
