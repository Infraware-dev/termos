/// Infraware Terminal - Hybrid Command Interpreter with AI Assistance
///
/// This is a TUI-based terminal that accepts user input and intelligently
/// routes it to either:
/// 1. Shell command execution (with auto-install for missing commands)
/// 2. LLM backend for natural language queries
///
/// Target use case: DevOps operations in cloud environments (AWS/Azure) with AI assistance

mod terminal;
mod input;
mod executor;
mod llm;
mod utils;

use anyhow::Result;
use std::time::Duration;

use terminal::{TerminalUI, TerminalState, TerminalMode, EventHandler, TerminalEvent};
use input::{InputClassifier, InputType};
use executor::{CommandExecutor, PackageInstaller, TabCompletion};
use llm::{MockLLMClient, ResponseRenderer};
use utils::AnsiColor;

/// Main application structure
struct InfrawareTerminal {
    ui: TerminalUI,
    state: TerminalState,
    classifier: InputClassifier,
    event_handler: EventHandler,
    llm_client: MockLLMClient,
    renderer: ResponseRenderer,
}

impl InfrawareTerminal {
    /// Create a new Infraware Terminal instance
    fn new() -> Result<Self> {
        Ok(Self {
            ui: TerminalUI::new()?,
            state: TerminalState::new(),
            classifier: InputClassifier::new(),
            event_handler: EventHandler::new(),
            llm_client: MockLLMClient,
            renderer: ResponseRenderer::new(),
        })
    }

    /// Run the main event loop
    async fn run(&mut self) -> Result<()> {
        // Display welcome message
        self.state.add_output(AnsiColor::Cyan.colorize("╔══════════════════════════════════════════════════════════════╗"));
        self.state.add_output(AnsiColor::Cyan.colorize("║       Infraware Terminal - AI-Assisted DevOps Shell        ║"));
        self.state.add_output(AnsiColor::Cyan.colorize("╚══════════════════════════════════════════════════════════════╝"));
        self.state.add_output(String::new());
        self.state.add_output("Type a command to execute or ask a question in natural language.".to_string());
        self.state.add_output(format!("{}", AnsiColor::BrightBlack.colorize("Press Ctrl+C to quit")));
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
        self.state.add_output(format!(
            "{} {}",
            AnsiColor::Cyan.colorize("❯"),
            input
        ));

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

        // Check if command exists
        if !CommandExecutor::command_exists(cmd) {
            self.state.add_output(format!(
                "{} Command '{}' not found",
                AnsiColor::Red.colorize("✗"),
                cmd
            ));

            // Offer to install
            if PackageInstaller::is_available() {
                self.state.add_output(format!(
                    "  {} Would you like to install it? (Feature coming in next version)",
                    AnsiColor::Yellow.colorize("→")
                ));
            }

            return Ok(());
        }

        // Execute the command
        match CommandExecutor::execute(cmd, args).await {
            Ok(output) => {
                if !output.stdout.is_empty() {
                    for line in output.stdout.lines() {
                        self.state.add_output(line.to_string());
                    }
                }
                if !output.stderr.is_empty() {
                    for line in output.stderr.lines() {
                        self.state.add_output(AnsiColor::Red.colorize(line));
                    }
                }
                if !output.is_success() {
                    self.state.add_output(format!(
                        "{} Command exited with code {}",
                        AnsiColor::Red.colorize("✗"),
                        output.exit_code
                    ));
                }
            }
            Err(e) => {
                self.state.add_output(format!(
                    "{} Error executing command: {}",
                    AnsiColor::Red.colorize("✗"),
                    e
                ));
            }
        }

        Ok(())
    }

    /// Handle natural language query
    async fn handle_natural_language(&mut self, query: &str) -> Result<()> {
        self.state.mode = TerminalMode::WaitingLLM;
        self.state.add_output(format!(
            "{} Querying AI assistant...",
            AnsiColor::Blue.colorize("⟳")
        ));

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
                self.state.add_output(format!(
                    "{} Error querying LLM: {}",
                    AnsiColor::Red.colorize("✗"),
                    e
                ));
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
            self.state.add_output(format!(
                "{} Possible completions:",
                AnsiColor::Yellow.colorize("→")
            ));
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
    // Create and run the terminal
    let mut terminal = InfrawareTerminal::new()?;

    // Run the main loop
    if let Err(e) = terminal.run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
