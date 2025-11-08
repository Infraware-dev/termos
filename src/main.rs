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
pub struct InfrawareTerminal {
    ui: TerminalUI,
    state: TerminalState,
    classifier: InputClassifier,
    event_handler: EventHandler,
    llm_client: Arc<dyn LLMClientTrait>,
    renderer: ResponseRenderer,
}

/// Builder for InfrawareTerminal
///
/// Implements the Builder Pattern to provide flexible, testable construction
/// of the terminal with dependency injection support.
///
/// # Example
/// ```no_run
/// use std::sync::Arc;
/// # use infraware_terminal::llm::MockLLMClient;
/// # use anyhow::Result;
/// # fn main() -> Result<()> {
/// let terminal = InfrawareTerminal::builder()
///     .with_llm_client(Arc::new(MockLLMClient::new()))
///     .build()?;
/// # Ok(())
/// # }
/// ```
pub struct InfrawareTerminalBuilder {
    ui: Option<TerminalUI>,
    state: Option<TerminalState>,
    classifier: Option<InputClassifier>,
    event_handler: Option<EventHandler>,
    llm_client: Option<Arc<dyn LLMClientTrait>>,
    renderer: Option<ResponseRenderer>,
}

impl InfrawareTerminalBuilder {
    /// Create a new builder with all fields set to None
    pub fn new() -> Self {
        Self {
            ui: None,
            state: None,
            classifier: None,
            event_handler: None,
            llm_client: None,
            renderer: None,
        }
    }

    /// Set a custom TerminalUI
    pub fn with_ui(mut self, ui: TerminalUI) -> Self {
        self.ui = Some(ui);
        self
    }

    /// Set a custom TerminalState
    pub fn with_state(mut self, state: TerminalState) -> Self {
        self.state = Some(state);
        self
    }

    /// Set a custom InputClassifier
    pub fn with_classifier(mut self, classifier: InputClassifier) -> Self {
        self.classifier = Some(classifier);
        self
    }

    /// Set a custom EventHandler
    pub fn with_event_handler(mut self, event_handler: EventHandler) -> Self {
        self.event_handler = Some(event_handler);
        self
    }

    /// Set a custom LLM client
    pub fn with_llm_client(mut self, client: Arc<dyn LLMClientTrait>) -> Self {
        self.llm_client = Some(client);
        self
    }

    /// Set a custom ResponseRenderer
    pub fn with_renderer(mut self, renderer: ResponseRenderer) -> Self {
        self.renderer = Some(renderer);
        self
    }

    /// Build the InfrawareTerminal instance
    ///
    /// Any components not explicitly set will use sensible defaults:
    /// - UI: New TerminalUI instance
    /// - State: New TerminalState with empty buffers
    /// - Classifier: Default InputClassifier with standard handlers
    /// - EventHandler: Default EventHandler
    /// - LLM Client: MockLLMClient (for development/testing)
    /// - Renderer: Default ResponseRenderer with syntax highlighting
    pub fn build(self) -> Result<InfrawareTerminal> {
        Ok(InfrawareTerminal {
            ui: match self.ui {
                Some(ui) => ui,
                None => TerminalUI::new()?,
            },
            state: self.state.unwrap_or_default(),
            classifier: self.classifier.unwrap_or_default(),
            event_handler: self.event_handler.unwrap_or_default(),
            llm_client: self
                .llm_client
                .unwrap_or_else(|| Arc::new(MockLLMClient::new())),
            renderer: self.renderer.unwrap_or_default(),
        })
    }
}

impl Default for InfrawareTerminalBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl InfrawareTerminal {
    /// Create a builder for constructing an InfrawareTerminal instance
    ///
    /// This is the recommended way to construct the terminal, especially
    /// for testing and when loading configuration from files.
    pub fn builder() -> InfrawareTerminalBuilder {
        InfrawareTerminalBuilder::new()
    }

    /// Create a new terminal instance with provided LLM client
    ///
    /// This is a convenience method. For more control, use `builder()`.
    #[allow(dead_code)]
    fn new_with_client(llm_client: Arc<dyn LLMClientTrait>) -> Result<Self> {
        Self::builder().with_llm_client(llm_client).build()
    }

    /// Create a new Infraware Terminal instance with mock LLM (for development/testing)
    ///
    /// This is a convenience method. For more control, use `builder()`.
    #[allow(dead_code)]
    fn new() -> Result<Self> {
        Self::builder().build()
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

    // Create terminal using builder pattern
    let mut terminal = InfrawareTerminal::builder()
        .with_llm_client(llm_client)
        .build()?;

    // Run the main loop
    if let Err(e) = terminal.run().await {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}
