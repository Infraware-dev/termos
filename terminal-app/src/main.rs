mod executor;
mod input;
mod llm;
mod logging;
mod orchestrators;
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

use input::{InputClassifier, InputType};
use llm::{HttpLLMClient, LLMClientTrait, MockLLMClient, ResponseRenderer};
use orchestrators::{CommandOrchestrator, NaturalLanguageOrchestrator, TabCompletionHandler};
use std::sync::Arc;
use terminal::events::TerminalEvent;
use terminal::{EventHandler, SplashScreen, TerminalMode, TerminalState, TerminalUI};
use utils::MessageFormatter;

/// Main application structure
///
/// Following Single Responsibility Principle (SRP), this struct now delegates
/// specific workflows to specialized orchestrators:
/// - CommandOrchestrator: Handles command execution workflow
/// - NaturalLanguageOrchestrator: Handles LLM query workflow
/// - TabCompletionHandler: Handles tab completion workflow
///
/// InfrawareTerminal's single responsibility is to:
/// - Manage the event loop
/// - Route events to appropriate handlers
/// - Coordinate between UI, state, and orchestrators
pub struct InfrawareTerminal {
    /// Terminal UI - public for splash screen access
    pub ui: TerminalUI,
    state: TerminalState,
    classifier: InputClassifier,
    event_handler: EventHandler,
    command_orchestrator: CommandOrchestrator,
    nl_orchestrator: NaturalLanguageOrchestrator,
    tab_completion_handler: TabCompletionHandler,
    /// Shared history for history expansion (synchronized with state.history)
    history_arc: Arc<std::sync::RwLock<Vec<String>>>,
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
#[derive(Default)]
pub struct InfrawareTerminalBuilder {
    ui: Option<TerminalUI>,
    state: Option<TerminalState>,
    classifier: Option<InputClassifier>,
    event_handler: Option<EventHandler>,
    llm_client: Option<Arc<dyn LLMClientTrait>>,
    renderer: Option<ResponseRenderer>,
    command_orchestrator: Option<CommandOrchestrator>,
    tab_completion_handler: Option<TabCompletionHandler>,
}

impl std::fmt::Debug for InfrawareTerminalBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareTerminalBuilder")
            .field("ui", &self.ui.is_some())
            .field("state", &self.state.is_some())
            .field("classifier", &self.classifier.is_some())
            .field("event_handler", &self.event_handler.is_some())
            .field("llm_client", &self.llm_client.is_some())
            .field("renderer", &self.renderer.is_some())
            .field("command_orchestrator", &self.command_orchestrator.is_some())
            .field(
                "tab_completion_handler",
                &self.tab_completion_handler.is_some(),
            )
            .finish()
    }
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
            command_orchestrator: None,
            tab_completion_handler: None,
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
    pub const fn with_event_handler(mut self, event_handler: EventHandler) -> Self {
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
    /// - Orchestrators: Default instances
    ///
    /// # Errors
    ///
    /// Returns an error if TerminalUI initialization fails. This can occur when
    /// the terminal backend cannot be initialized or when entering raw mode fails.
    pub fn build(self) -> Result<InfrawareTerminal> {
        let llm_client = self
            .llm_client
            .unwrap_or_else(|| Arc::new(MockLLMClient::new()));
        let renderer = self.renderer.unwrap_or_default();

        // Create state
        let state = self.state.unwrap_or_default();

        // Create a shared reference to the history for the classifier
        // The history is owned by state, but we create an Arc<RwLock> wrapper
        // that the classifier can use for history expansion
        let history_vec = Arc::new(std::sync::RwLock::new(state.history.all().to_vec()));

        // Create classifier with history support
        let classifier = match self.classifier {
            Some(c) => c,
            None => InputClassifier::new().with_history(history_vec.clone()),
        };

        Ok(InfrawareTerminal {
            ui: match self.ui {
                Some(ui) => ui,
                None => TerminalUI::new()?,
            },
            state,
            classifier,
            event_handler: self.event_handler.unwrap_or_default(),
            command_orchestrator: self.command_orchestrator.unwrap_or_default(),
            nl_orchestrator: NaturalLanguageOrchestrator::new(llm_client, renderer),
            tab_completion_handler: self.tab_completion_handler.unwrap_or_default(),
            history_arc: history_vec,
        })
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
        // Load aliases at startup (blocking to ensure they're available before first command)
        // Use spawn_blocking for file I/O to avoid blocking the executor
        let alias_load_result = tokio::task::spawn_blocking(|| {
            use input::discovery::CommandCache;

            // Load system aliases first
            if let Err(e) = CommandCache::load_system_aliases() {
                log::warn!("Failed to load system aliases: {}", e);
            }

            // Load user aliases (these override system aliases)
            CommandCache::load_user_aliases();
        })
        .await;

        // Log if alias loading task panicked
        if let Err(e) = alias_load_result {
            log::error!("Alias loading task panicked: {}", e);
        }

        // Display welcome message
        self.state.add_output(MessageFormatter::banner_line(
            "╔══════════════════════════════════════════════════════════════╗",
        ));
        self.state.add_output(MessageFormatter::banner_line(
            "║   Infraware Terminal - AI-Assisted DevOps Shell              ║",
        ));
        self.state.add_output(MessageFormatter::banner_line(
            "╚══════════════════════════════════════════════════════════════╝",
        ));
        self.state.add_output(String::new());
        self.state.add_output(
            "Type a command to execute or ask a question in natural language.".to_string(),
        );
        self.state
            .add_output(MessageFormatter::banner_hint("Press Ctrl+C to quit"));
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
                if !self.handle_submit().await? {
                    return Ok(false); // Exit requested
                }
            }
            TerminalEvent::TabComplete => {
                self.handle_tab_completion();
            }
            TerminalEvent::ClearScreen => {
                self.state.output.clear();
            }
            TerminalEvent::Resize(_, _) => {
                // Terminal resized - re-render will handle it
            }
            TerminalEvent::Unknown => {}
        }

        Ok(true)
    }

    /// Handle input submission
    /// Returns false if the terminal should exit
    async fn handle_submit(&mut self) -> Result<bool> {
        let input = self.state.submit_input();

        if input.trim().is_empty() {
            return Ok(true);
        }

        // Handle built-in exit command
        let trimmed = input.trim().to_lowercase();
        if trimmed == "exit" || trimmed == "quit" {
            self.state.add_output(MessageFormatter::info("Goodbye!"));
            return Ok(false); // Signal to exit
        }

        // Sync history with Arc for history expansion
        {
            let mut history_guard = match self.history_arc.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            *history_guard = self.state.history.all().to_vec();
        }

        // Classify the input
        match self.classifier.classify(&input)? {
            InputType::Command {
                command,
                args,
                original_input,
            } => {
                // Don't echo input for clear command (it clears the output)
                if command != "clear" {
                    self.state.add_output(MessageFormatter::command(&input));
                }
                self.handle_command(&command, &args, original_input.as_deref())
                    .await?;
            }
            InputType::NaturalLanguage(query) => {
                self.state.add_output(MessageFormatter::command(&input));
                self.handle_natural_language(&query).await?;
            }
            InputType::CommandTypo {
                input: typo_input,
                suggestion,
                distance,
            } => {
                self.state.add_output(MessageFormatter::command(&input));
                self.handle_command_typo(&typo_input, &suggestion, distance)
                    .await?;
            }
            InputType::Empty => {}
        }

        self.state.add_output(String::new()); // Empty line for spacing
        self.state.mode = TerminalMode::Normal;

        Ok(true)
    }

    /// Handle command execution
    ///
    /// Delegates to CommandOrchestrator (SRP compliance)
    async fn handle_command(
        &mut self,
        cmd: &str,
        args: &[String],
        original_input: Option<&str>,
    ) -> Result<()> {
        self.state.mode = TerminalMode::ExecutingCommand;

        self.command_orchestrator
            .handle_command(cmd, args, original_input, &mut self.state, &mut self.ui)
            .await
    }

    /// Handle command typo
    ///
    /// Auto-corrects the typo and executes the suggested command
    async fn handle_command_typo(
        &mut self,
        input: &str,
        suggestion: &str,
        distance: usize,
    ) -> Result<()> {
        // Extract the mistyped first word and get the rest of the input
        let parts: Vec<&str> = input.split_whitespace().collect();
        let mistyped = parts.first().copied().unwrap_or(input);

        // Show correction message
        self.state.add_output(MessageFormatter::suggestion(format!(
            "Correcting '{}' → '{}' (Levenshtein distance: {})",
            mistyped, suggestion, distance
        )));

        // Reconstruct command with corrected first word
        let corrected_input = if parts.len() > 1 {
            format!("{} {}", suggestion, parts[1..].join(" "))
        } else {
            suggestion.to_string()
        };

        // Parse and execute the corrected command
        use crate::input::parser::CommandParser;
        match CommandParser::parse(&corrected_input) {
            Ok((command, args)) => {
                self.command_orchestrator
                    .handle_command(&command, &args, None, &mut self.state, &mut self.ui)
                    .await
            }
            Err(e) => {
                self.state.add_output(MessageFormatter::error(format!(
                    "Failed to parse command: {e}"
                )));
                Ok(())
            }
        }
    }

    /// Handle natural language query
    ///
    /// Delegates to NaturalLanguageOrchestrator (SRP compliance)
    async fn handle_natural_language(&mut self, query: &str) -> Result<()> {
        self.state.mode = TerminalMode::WaitingLLM;

        self.nl_orchestrator
            .handle_query(query, &mut self.state, &mut self.ui)
            .await
    }

    /// Handle tab completion
    ///
    /// Delegates to TabCompletionHandler (SRP compliance)
    fn handle_tab_completion(&mut self) {
        self.tab_completion_handler
            .handle_tab_completion(&mut self.state);
    }
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file (if present)
    dotenvy::dotenv().ok();

    // Initialize logging system
    logging::init()?;

    log::info!("Infraware Terminal starting...");

    // Configure LLM client based on environment variable
    let llm_client: Arc<dyn LLMClientTrait> = if let Ok(url) = std::env::var("INFRAWARE_LLM_URL") {
        log::info!("Using HTTP LLM client: {}", url);
        Arc::new(HttpLLMClient::new(url))
    } else {
        log::info!("Using Mock LLM client (set INFRAWARE_LLM_URL to use real LLM)");
        Arc::new(MockLLMClient::new())
    };

    // Create terminal using builder pattern
    log::debug!("Building terminal UI...");
    let mut terminal = InfrawareTerminal::builder()
        .with_llm_client(llm_client)
        .build()?;

    // Show animated splash screen
    log::debug!("Showing splash screen");
    SplashScreen::run(terminal.ui.inner_terminal())?;

    // Run the main loop
    log::debug!("Starting main event loop");
    if let Err(e) = terminal.run().await {
        log::error!("Fatal error: {}", e);
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    log::info!("Infraware Terminal shutting down");
    Ok(())
}
