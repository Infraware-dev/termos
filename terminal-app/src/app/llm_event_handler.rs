//! LLM event handling.
//!
//! Provides `LlmEventHandler` which processes background LLM events
//! and updates application state accordingly.

use super::AppBackgroundEvent;
use super::llm_controller::LlmController;
use super::state::AppState;
use crate::state::AppMode;

/// Handles LLM background events.
///
/// Processes results from background LLM queries and updates
/// session state and terminal output.
pub struct LlmEventHandler<'a> {
    /// Application state
    state: &'a mut AppState,
    /// LLM controller (for response renderer and incremental renderer)
    llm: &'a mut LlmController,
}

impl<'a> LlmEventHandler<'a> {
    /// Creates a new LLM event handler.
    pub fn new(state: &'a mut AppState, llm: &'a mut LlmController) -> Self {
        Self { state, llm }
    }

    /// Handles a single LLM background event.
    pub fn handle_event(&mut self, event: AppBackgroundEvent) {
        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        // Log non-chunk events (chunks are too frequent)
        match &event {
            AppBackgroundEvent::LlmChunk(_) => {
                tracing::debug!("Received LLM chunk, mode: {:?}", session.mode.name());
            }
            _ => {
                tracing::info!(
                    "Received background event: {:?}, current mode: {:?}",
                    event,
                    session.mode.name()
                );
            }
        }

        match event {
            // Streaming chunk - render incrementally
            AppBackgroundEvent::LlmChunk(text) => {
                self.handle_chunk(text);
            }
            // Streaming complete
            AppBackgroundEvent::LlmStreamComplete => {
                self.handle_stream_complete();
            }
            // Streaming HITL - command approval
            AppBackgroundEvent::LlmCommandApproval { command, message, .. } => {
                // Finalize any pending output first
                self.finalize_incremental_output();
                let session = match self.state.active_session_mut() {
                    Some(s) => s,
                    None => return,
                };
                session.agent_state.end_stream();
                self.handle_command_approval(command, message);
            }
            // Streaming HITL - question
            AppBackgroundEvent::LlmQuestion { question, options } => {
                // Finalize any pending output first
                self.finalize_incremental_output();
                let session = match self.state.active_session_mut() {
                    Some(s) => s,
                    None => return,
                };
                session.agent_state.end_stream();
                self.handle_question(question, options);
            }
            // Incident investigation phase banner
            AppBackgroundEvent::LlmPhase(phase) => {
                let session = match self.state.active_session_mut() {
                    Some(s) => s,
                    None => return,
                };
                let banner = match phase {
                    infraware_shared::IncidentPhase::Investigating => {
                        "\r\n\x1b[1;36m🔍 Investigating...\x1b[0m\r\n"
                    }
                    infraware_shared::IncidentPhase::Analyzing => {
                        "\r\n\x1b[1;33m🧠 Analyzing findings...\x1b[0m\r\n"
                    }
                    infraware_shared::IncidentPhase::Reporting => {
                        "\r\n\x1b[1;32m📄 Generating post-mortem report...\x1b[0m\r\n"
                    }
                    infraware_shared::IncidentPhase::Completed => {
                        "\r\n\x1b[1;34m✅ Incident pipeline completed.\x1b[0m\r\n"
                    }
                };
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, banner.as_bytes());
            }
            // Error
            AppBackgroundEvent::LlmError(err) => {
                // Finalize any pending output first
                self.finalize_incremental_output();
                let session = match self.state.active_session_mut() {
                    Some(s) => s,
                    None => return,
                };
                session.agent_state.end_stream();
                tracing::error!("LLM query error: {}", err);
                let error_msg = format!("\x1b[31mError: {}\x1b[0m", err);
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, error_msg.as_bytes());
                session.mode = AppMode::Normal;
                session.send_to_pty(b"\x15\n");
            }
        }
    }

    /// Handles a streaming chunk by rendering it incrementally.
    fn handle_chunk(&mut self, text: String) {
        // Process chunk through incremental renderer
        let (complete_lines, partial) = self.llm.incremental_renderer.append(&text);

        // Check if we need to go back up (previous chunk had partial on newline)
        let need_cursor_up = self.llm.incremental_renderer.had_partial_on_newline()
            && (!complete_lines.is_empty() || partial.is_some());

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        // If this is the first chunk, add a leading newline
        if !self.llm.incremental_renderer.has_started() {
            session
                .vte_parser
                .advance(&mut session.terminal_handler, b"\r\n");
            self.llm.incremental_renderer.mark_started();
        }

        // If previous chunk had partial on newline and we have new output, go back up
        if need_cursor_up {
            // Cursor up one line, carriage return, clear line
            session
                .vte_parser
                .advance(&mut session.terminal_handler, b"\x1b[A\r\x1b[K");
        }

        // Output complete lines
        for line in complete_lines.iter() {
            session
                .vte_parser
                .advance(&mut session.terminal_handler, line.as_bytes());
            session
                .vte_parser
                .advance(&mut session.terminal_handler, b"\r\n");
        }

        // Show partial line (will be overwritten by next chunk)
        if let Some(ref partial_text) = partial {
            session
                .vte_parser
                .advance(&mut session.terminal_handler, partial_text.as_bytes());
            // Move to new line so throbber appears below partial content
            session
                .vte_parser
                .advance(&mut session.terminal_handler, b"\r\n");
            self.llm.incremental_renderer.set_partial_on_newline(true);
        } else {
            self.llm.incremental_renderer.set_partial_on_newline(false);
        }
    }

    /// Handles stream completion.
    fn handle_stream_complete(&mut self) {
        tracing::info!("LLM stream completed, finalizing output");

        // If previous chunk had partial on newline, clear it before finalizing
        // (finalize will re-output the content, so we need to avoid duplicate)
        if self.llm.incremental_renderer.had_partial_on_newline()
            && let Some(session) = self.state.active_session_mut()
        {
            // Cursor up one line, carriage return, clear line
            session
                .vte_parser
                .advance(&mut session.terminal_handler, b"\x1b[A\r\x1b[K");
        }

        // Finalize incremental output
        self.finalize_incremental_output();

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        session.agent_state.end_stream();
        session.mode = AppMode::Normal;

        // Clear shell buffer and trigger fresh prompt
        session.send_to_pty(b"\x15\n");
    }

    /// Finalizes incremental renderer output.
    fn finalize_incremental_output(&mut self) {
        let final_lines = self.llm.incremental_renderer.finalize();

        let session = match self.state.active_session_mut() {
            Some(s) => s,
            None => return,
        };

        // Output any remaining buffered content
        for (i, line) in final_lines.iter().enumerate() {
            if i > 0 || self.llm.incremental_renderer.has_started() {
                session
                    .vte_parser
                    .advance(&mut session.terminal_handler, b"\r\n");
            }
            session
                .vte_parser
                .advance(&mut session.terminal_handler, line.as_bytes());
        }

        // Reset for next response
        self.llm.incremental_renderer.reset();
    }

    /// Handles LLM command approval request.
    fn handle_command_approval(&mut self, command: String, message: String) {
        tracing::info!("LLM requested command approval: {}", command);

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
        tracing::info!("LLM asked a question: {}", question);

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
