//! LLM event handling.
//!
//! Provides `LlmEventHandler` which processes background LLM events
//! and updates application state accordingly.

use super::AppBackgroundEvent;
use super::llm_controller::LlmController;
use super::state::AppState;
use crate::llm::LLMQueryResult;
use crate::state::AppMode;

/// Handles LLM background events.
///
/// Processes results from background LLM queries and updates
/// session state and terminal output.
pub struct LlmEventHandler<'a> {
    /// Application state
    state: &'a mut AppState,
    /// LLM controller (for response renderer)
    llm: &'a LlmController,
}

impl<'a> LlmEventHandler<'a> {
    /// Creates a new LLM event handler.
    pub fn new(state: &'a mut AppState, llm: &'a LlmController) -> Self {
        Self { state, llm }
    }

    /// Handles a single LLM background event.
    pub fn handle_event(&mut self, event: AppBackgroundEvent) {
        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        log::info!(
            "Received background event: {:?}, current mode: {:?}",
            event,
            session.mode.name()
        );

        match event {
            AppBackgroundEvent::LlmResult(result) => {
                session.agent_state.end_stream();
                match result {
                    LLMQueryResult::Complete(text) => {
                        self.handle_complete(text);
                    }
                    LLMQueryResult::CommandApproval { command, message } => {
                        self.handle_command_approval(command, message);
                    }
                    LLMQueryResult::Question { question, options } => {
                        self.handle_question(question, options);
                    }
                }
            }
            AppBackgroundEvent::LlmError(err) => {
                session.agent_state.end_stream();
                log::error!("LLM query error: {}", err);
                let error_msg = format!("\x1b[31mError: {}\x1b[0m", err);
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, error_msg.as_bytes());
                session.mode = AppMode::Normal;
                session.send_to_pty(b"\x15\n");
            }
        }
    }

    /// Handles LLM complete response.
    fn handle_complete(&mut self, text: String) {
        log::info!(
            "LLM query complete, response length: {} chars, transitioning to Normal",
            text.len()
        );

        // Set mode FIRST to stop throbber immediately
        if let Some(session) = self.state.active_session_mut() {
            session.mode = AppMode::Normal;
        }

        if text.is_empty() {
            log::debug!("Empty response, no output to render");
            return;
        }

        // Render response lines (markdown → ANSI)
        let lines = self.llm.response_renderer.render(&text);
        log::debug!("Rendered {} lines to display", lines.len());

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        // Start with newline to avoid overwriting current prompt
        session
            .vte_parser
            .advance(&mut session.terminal_handler, b"\r\n");

        let last_idx = lines.len().saturating_sub(1);
        for (i, line) in lines.iter().enumerate() {
            session
                .vte_parser
                .advance(&mut session.terminal_handler, line.as_bytes());
            if i < last_idx {
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, b"\r\n");
            }
        }

        // Clear shell buffer and trigger fresh prompt
        session.send_to_pty(b"\x15\n");
    }

    /// Handles LLM command approval request.
    fn handle_command_approval(&mut self, command: String, message: String) {
        log::info!("LLM requested command approval: {}", command);

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        session.mode = AppMode::AwaitingApproval {
            command: command.clone(),
            message: message.clone(),
        };

        let message_formatted = message.replace('\n', "\r\n");
        let prompt = format!(
            "\r\n\x1b[1;33m{}\x1b[0m\r\n\r\n\x1b[1;36mCommand:\x1b[0m \x1b[1m{}\x1b[0m\r\n\r\n\x1b[90mType 'y' to approve, 'n' to reject:\x1b[0m ",
            message_formatted, command
        );
        session
            .vte_parser
            .advance(&mut session.terminal_handler, prompt.as_bytes());
    }

    /// Handles LLM question.
    fn handle_question(&mut self, question: String, options: Option<Vec<String>>) {
        log::info!("LLM asked a question: {}", question);

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        session.mode = AppMode::AwaitingAnswer {
            question: question.clone(),
            options: options.clone(),
        };

        let question_formatted = question.replace('\n', "\r\n");
        let mut prompt = format!(
            "\r\n\x1b[1;33mAgent Question:\x1b[0m\r\n  {}\r\n",
            question_formatted
        );

        if let Some(ref opts) = options {
            prompt.push_str("\x1b[90m  Options:\x1b[0m\r\n");
            for (i, opt) in opts.iter().enumerate() {
                let opt_formatted = opt.replace('\n', "\r\n");
                prompt.push_str(&format!("    {}. {}\r\n", i + 1, opt_formatted));
            }
        }

        prompt.push_str("\r\n\x1b[90mType your answer:\x1b[0m ");
        session
            .vte_parser
            .advance(&mut session.terminal_handler, prompt.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    // Note: Full tests require TerminalSession which has PTY dependencies.
    // Testing this module properly would require mock sessions.

    #[test]
    fn test_llm_event_handler_compiles() {
        // Verifies the struct and methods compile correctly
        // (compilation success is the test)
    }
}
