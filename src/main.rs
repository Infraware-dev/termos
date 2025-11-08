mod executor;
mod input;
mod llm;
/// Infraware Terminal - Hybrid Command Interpreter with AI Assistance
///
/// This is a TUI-based terminal that accepts user input and intelligently
/// routes it to either:
/// 1. Shell command execution (with auto-install for missing commands)
/// 2. LLM backend for natural language queries
///
/// Target use case: DevOps operations in cloud environments (AWS/Azure) with AI assistance
mod terminal;
mod utils;

use anyhow::Result;
use std::time::Duration;

use executor::{CommandExecutor, PackageInstaller, TabCompletion};
use input::{InputClassifier, InputType};
use llm::{HttpLLMClient, LLMClientTrait, MockLLMClient, ResponseRenderer};
use std::sync::Arc;
use terminal::events::TerminalEvent;
use terminal::{EventHandler, TerminalMode, TerminalState, TerminalUI};
use utils::{AnsiColor, MessageFormatter};

/// Main application structure
struct InfrawareTerminal {
    ui: TerminalUI,
    state: TerminalState,
    classifier: InputClassifier,
    event_handler: EventHandler,
    llm_client: Arc<dyn LLMClientTrait>,
    renderer: ResponseRenderer,
}

impl InfrawareTerminal {
    /// Create a new terminal instance with provided LLM client
    fn new_with_client(llm_client: Arc<dyn LLMClientTrait>) -> Result<Self> {
        Ok(Self {
            ui: TerminalUI::new()?,
            state: TerminalState::new(),
            classifier: InputClassifier::new(),
            event_handler: EventHandler::new(),
            llm_client,
            renderer: ResponseRenderer::new(),
        })
    }

    /// Create a new Infraware Terminal instance with mock LLM (for development/testing)
    #[allow(dead_code)]
    fn new() -> Result<Self> {
        Self::new_with_client(Arc::new(MockLLMClient::new()))
    }

    /// Run the main event loop
    async fn run(&mut self) -> Result<()> {
        // Display welcome message
        self.state.add_output(
            AnsiColor::Cyan
                .colorize("╔══════════════════════════════════════════════════════════════╗"),
        );
        self.state.add_output(
            AnsiColor::Cyan.colorize("║   Infraware Terminal - AI-Assisted DevOps Shell         ║"),
        );
        self.state.add_output(
            AnsiColor::Cyan
                .colorize("╚══════════════════════════════════════════════════════════════╝"),
        );
        self.state.add_output(String::new());
        self.state.add_output(
            "Type a command to execute or ask a question in natural language.".to_string(),
        );
        self.state.add_output(
            AnsiColor::BrightBlack
                .colorize("Press Ctrl+C to quit")
                .to_string(),
        );
        self.state.add_output(String::new());

        // Initial render
        self.ui.render(&self.state)?;

        // Main event loop
        loop {
            // Poll for events with a short timeout
            if let Some(event) = self.event_handler.poll_event(Duration::from_millis(100))? {
                if !self.handle_event(event).await? {
                    break; // Quit requested
                }

                // Re-render after handling event
                self.ui.render(&self.state)?;
            }
        }

        Ok(())
    }

    /// Handle a terminal event
    async fn handle_event(&mut self, event: TerminalEvent) -> Result<bool> {
        match event {
            TerminalEvent::Quit => {
                return Ok(false);
            }
            TerminalEvent::InputChar(c) => {
                self.state.insert_char(c);
            }
            TerminalEvent::DeleteChar => {
                self.state.delete_char();
            }
            TerminalEvent::MoveCursorLeft => {
                self.state.move_cursor_left();
            }
            TerminalEvent::MoveCursorRight => {
                self.state.move_cursor_right();
            }
            TerminalEvent::HistoryPrevious => {
                self.state.history_previous();
            }
            TerminalEvent::HistoryNext => {
                self.state.history_next();
            }
            TerminalEvent::ScrollUp => {
                self.state.scroll_up();
            }
            TerminalEvent::ScrollDown => {
                self.state.scroll_down();
            }
            TerminalEvent::Submit => {
                self.handle_submit().await?;
            }
            TerminalEvent::TabComplete => {
                self.handle_tab_completion();
            }
            TerminalEvent::ClearScreen => {
                self.state.output_buffer.clear();
                self.state.scroll_position = 0;
            }
            TerminalEvent::Resize(_, _) => {
                // Terminal resized - re-render will handle it
            }
            TerminalEvent::Unknown => {}
        }

        Ok(true)
    }

    /// Handle input submission
    async fn handle_submit(&mut self) -> Result<()> {
        let input = self.state.submit_input();

        if input.trim().is_empty() {
            return Ok(());
        }

        // Echo the input
        self.state.add_output(MessageFormatter::command(&input));

        // Classify the input
        match self.classifier.classify(&input)? {
            InputType::Command(cmd, args) => {
                self.handle_command(&cmd, &args).await?;
            }
            InputType::NaturalLanguage(query) => {
                self.handle_natural_language(&query).await?;
            }
            InputType::Empty => {}
        }

        self.state.add_output(String::new()); // Empty line for spacing
        self.state.mode = TerminalMode::Normal;

        Ok(())
    }

    /// Handle command execution
    async fn handle_command(&mut self, cmd: &str, args: &[String]) -> Result<()> {
        self.state.mode = TerminalMode::ExecutingCommand;

        // Handle special built-in commands that would interfere with TUI
        if cmd == "clear" {
            // Clear the output buffer instead of executing the system clear command
            self.state.output_buffer.clear();
            self.state.scroll_position = 0;
            // Force a complete terminal clear to prevent spurious characters
            self.ui.clear()?;
            return Ok(());
        }

        // Check if command exists
        if !CommandExecutor::command_exists(cmd) {
            self.state
                .add_output(MessageFormatter::command_not_found(cmd));
            self.state.add_output(MessageFormatter::install_suggestion(
                PackageInstaller::is_available(),
            ));
            return Ok(());
        }

        // Execute the command
        match CommandExecutor::execute(cmd, args).await {
            Ok(output) => {
                // Show stdout as-is
                if !output.stdout.is_empty() {
                    for line in output.stdout.lines() {
                        self.state.add_output(line.to_string());
                    }
                }

                // Show stderr - only colorize red if command failed
                if !output.stderr.is_empty() {
                    for line in output.stderr.lines() {
                        if output.is_success() {
                            // Command succeeded, stderr is just informational
                            self.state.add_output(line.to_string());
                        } else {
                            // Command failed, highlight stderr in red
                            self.state.add_output(AnsiColor::Red.colorize(line));
                        }
                    }
                }

                if !output.is_success() {
                    self.state
                        .add_output(MessageFormatter::command_failed(output.exit_code));
                }
            }
            Err(e) => {
                self.state
                    .add_output(MessageFormatter::execution_error(e.to_string()));
            }
        }

        Ok(())
    }

    /// Handle natural language query
    async fn handle_natural_language(&mut self, query: &str) -> Result<()> {
        self.state.mode = TerminalMode::WaitingLLM;
        self.state
            .add_output(MessageFormatter::info("Querying AI assistant..."));

        // Render to show "waiting" state
        self.ui.render(&self.state)?;

        // Query the LLM (using mock for now)
        match self.llm_client.query(query).await {
            Ok(response) => {
                // Remove the "Querying..." message
                self.state.output_buffer.pop();

                // Render the response with formatting
                let formatted_lines = self.renderer.render(&response);
                self.state.add_output_lines(formatted_lines);
            }
            Err(e) => {
                self.state.output_buffer.pop();
                self.state.add_output(MessageFormatter::error(format!(
                    "Error querying LLM: {}",
                    e
                )));
            }
        }

        Ok(())
    }

    /// Handle tab completion
    fn handle_tab_completion(&mut self) {
        let input = self.state.input_buffer.clone();
        let completions = TabCompletion::get_completions(&input);

        if completions.is_empty() {
            return;
        }

        if completions.len() == 1 {
            // Single completion - auto-complete
            self.state.input_buffer = completions[0].clone();
            self.state.cursor_position = self.state.input_buffer.len();
        } else {
            // Multiple completions - show them
            self.state
                .add_output(MessageFormatter::info("Possible completions:"));
            for completion in &completions {
                self.state.add_output(format!("  {}", completion));
            }

            // Auto-complete to common prefix
            let common_prefix = TabCompletion::get_common_prefix(&completions);
            if common_prefix.len() > input.len() {
                self.state.input_buffer = common_prefix;
                self.state.cursor_position = self.state.input_buffer.len();
            }
        }
    }
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Configure LLM client based on environment variable
    let llm_client: Arc<dyn LLMClientTrait> = match std::env::var("INFRAWARE_LLM_URL") {
        Ok(url) => {
            eprintln!("Using HTTP LLM client: {}", url);
            Arc::new(HttpLLMClient::new(url))
        }
        Err(_) => {
            eprintln!("Using Mock LLM client (set INFRAWARE_LLM_URL to use real LLM)");
            Arc::new(MockLLMClient::new())
        }
    };

    // Create and run the terminal
    let mut terminal = InfrawareTerminal::new_with_client(llm_client)?;

    // Run the main loop
    if let Err(e) = terminal.run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
