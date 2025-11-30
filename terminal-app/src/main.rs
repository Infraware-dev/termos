/// Infraware Terminal - Hybrid Command Interpreter with AI Assistance
///
/// This is a TUI-based terminal that accepts user input and intelligently
/// routes it to either:
/// 1. Shell command execution (with auto-install for missing commands)
/// 2. LLM backend for natural language queries
///
/// Target use case: DevOps operations in cloud environments (AWS/Azure) with AI assistance
use infraware_terminal::{auth, input, llm, logging, orchestrators, terminal, utils};

use anyhow::Result;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use auth::{AuthConfig, Authenticator, HttpAuthenticator};
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
    /// Optional authenticator for backend auth status checks
    authenticator: Option<Arc<dyn Authenticator>>,
    /// Whether using mock LLM client (for display purposes)
    using_mock_llm: bool,
    /// Cancellation token for interrupting long-running operations
    cancellation_token: CancellationToken,
}

impl std::fmt::Debug for InfrawareTerminal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InfrawareTerminal")
            .field("ui", &self.ui)
            .field("state", &self.state)
            .field("classifier", &self.classifier)
            .field("event_handler", &self.event_handler)
            .field("command_orchestrator", &self.command_orchestrator)
            .field("nl_orchestrator", &self.nl_orchestrator)
            .field("tab_completion_handler", &self.tab_completion_handler)
            .field("history_arc", &"<Arc<RwLock<Vec<String>>>>")
            .field("authenticator", &self.authenticator.is_some())
            .field("using_mock_llm", &self.using_mock_llm)
            .field(
                "cancellation_token",
                &format!(
                    "CancellationToken(cancelled={})",
                    self.cancellation_token.is_cancelled()
                ),
            )
            .finish()
    }
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
    authenticator: Option<Arc<dyn Authenticator>>,
    using_mock_llm: bool,
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
            .field("authenticator", &self.authenticator.is_some())
            .field("using_mock_llm", &self.using_mock_llm)
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
            authenticator: None,
            using_mock_llm: true, // Default to mock until explicitly set
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

    /// Set an authenticator for backend auth status checks
    pub fn with_authenticator(mut self, authenticator: Arc<dyn Authenticator>) -> Self {
        self.authenticator = Some(authenticator);
        self
    }

    /// Set whether using mock LLM client (for display purposes)
    pub const fn with_using_mock_llm(mut self, using_mock: bool) -> Self {
        self.using_mock_llm = using_mock;
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
            authenticator: self.authenticator,
            using_mock_llm: self.using_mock_llm,
            cancellation_token: CancellationToken::new(),
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
    #[expect(
        dead_code,
        reason = "Convenience method for tests, builder pattern is preferred"
    )]
    fn new_with_client(llm_client: Arc<dyn LLMClientTrait>) -> Result<Self> {
        Self::builder().with_llm_client(llm_client).build()
    }

    /// Create a new Infraware Terminal instance with mock LLM (for development/testing)
    ///
    /// This is a convenience method. For more control, use `builder()`.
    #[expect(
        dead_code,
        reason = "Convenience method for tests, builder pattern is preferred"
    )]
    fn new() -> Result<Self> {
        Self::builder().build()
    }

    /// Update the LLM client (used for deferred initialization after splash)
    fn set_llm_client(&mut self, client: Arc<dyn LLMClientTrait>) {
        self.nl_orchestrator.set_llm_client(client);
    }

    /// Set the authenticator (used for deferred initialization after splash)
    fn set_authenticator(&mut self, auth: Option<Arc<dyn Authenticator>>) {
        self.authenticator = auth;
    }

    /// Set whether using mock LLM (used for deferred initialization after splash)
    fn set_using_mock_llm(&mut self, using_mock: bool) {
        self.using_mock_llm = using_mock;
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

        // Show LLM client status
        if self.using_mock_llm {
            self.state.add_output(MessageFormatter::info(
                "LLM: Mock mode (backend not available)",
            ));
        } else {
            self.state
                .add_output(MessageFormatter::success("LLM: Connected to backend"));
        }
        self.state.add_output(String::new());

        // Initial render
        self.ui.render(&self.state)?;

        // Create channel for events from background polling task
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TerminalEvent>(32);
        let cancel_token_for_poller = self.cancellation_token.clone();

        // Spawn background task for event polling
        // This task runs independently and can detect Ctrl+C even during LLM queries
        let poll_handle = tokio::task::spawn_blocking(move || {
            let event_handler = EventHandler::new();
            loop {
                // Check if we should stop
                if cancel_token_for_poller.is_cancelled() {
                    log::info!("Event poller: cancellation detected, stopping");
                    break;
                }

                // Poll with short timeout
                match event_handler.poll_event(Duration::from_millis(50)) {
                    Ok(Some(event)) => {
                        // For Quit events, we need to signal cancellation immediately
                        if matches!(event, TerminalEvent::Quit) {
                            log::info!("Event poller: Quit detected, cancelling token");
                            cancel_token_for_poller.cancel();
                        }
                        // Send event to main loop (ignore error if channel closed)
                        if event_tx.blocking_send(event).is_err() {
                            break;
                        }
                    }
                    Ok(None) => {
                        // No event, continue polling
                    }
                    Err(e) => {
                        log::error!("Event polling error: {}", e);
                        break;
                    }
                }
            }
        });

        // Main event loop - receives events from background poller
        loop {
            // Check cancellation first
            if self.cancellation_token.is_cancelled() {
                log::info!("Main loop: cancellation detected, exiting");
                break;
            }

            // Wait for event with timeout, also checking cancellation
            let event = tokio::select! {
                maybe_event = event_rx.recv() => {
                    match maybe_event {
                        Some(event) => event,
                        None => {
                            log::info!("Event channel closed");
                            break;
                        }
                    }
                }
                _ = self.cancellation_token.cancelled() => {
                    log::info!("Main loop: cancelled via token during recv");
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Periodic check for cancellation
                    continue;
                }
            };

            // Handle the event
            if !self.handle_event(event).await? {
                break; // Quit requested
            }

            // Re-render after handling event
            self.ui.render(&self.state)?;
        }

        // Clean up polling task
        drop(event_rx);
        let _ = poll_handle.await;

        Ok(())
    }

    /// Handle a terminal event
    async fn handle_event(&mut self, event: TerminalEvent) -> Result<bool> {
        match event {
            TerminalEvent::Quit => {
                log::info!("Quit signal received, cancelling ongoing operations");
                self.cancellation_token.cancel();
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

        // Handle human-in-the-loop command approval mode (y/n)
        if self.state.mode == TerminalMode::AwaitingCommandApproval {
            let trimmed = input.trim().to_lowercase();
            let approved = trimmed == "y" || trimmed == "yes";
            self.state.add_output(MessageFormatter::command(&input));

            // Delegate to orchestrator for approval handling
            self.nl_orchestrator
                .handle_approval(approved, &mut self.state, &mut self.ui)
                .await?;

            return Ok(true);
        }

        // Handle human-in-the-loop answer mode (free-form text)
        if self.state.mode == TerminalMode::AwaitingAnswer {
            self.state.add_output(MessageFormatter::command(&input));

            // Delegate to orchestrator for answer handling
            self.nl_orchestrator
                .handle_answer(input, &mut self.state, &mut self.ui)
                .await?;

            return Ok(true);
        }

        if input.trim().is_empty() {
            return Ok(true);
        }

        // Sync history with Arc for history expansion
        {
            let mut history_guard = self
                .history_arc
                .write()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *history_guard = self.state.history.all().to_vec();
        }

        // Classify the input
        match self.classifier.classify(&input)? {
            InputType::Command {
                command,
                args,
                original_input,
            } => {
                // Handle exit/quit builtins - exit immediately
                if command == "exit" || command == "quit" {
                    self.state.add_output(MessageFormatter::info("Goodbye!"));
                    self.cancellation_token.cancel(); // Signal poller to stop
                    return Ok(false);
                }

                // Don't echo input for clear command (it clears the output)
                if command != "clear" {
                    self.state.add_output(MessageFormatter::command(&input));
                }
                self.handle_command(&command, &args, original_input.as_deref())
                    .await?;
            }
            InputType::NaturalLanguage(query) => {
                self.state.add_output(MessageFormatter::command(&input));

                // Clone token (cheap Arc increment)
                let token = self.cancellation_token.clone();
                self.handle_natural_language(&query, token).await?;

                // Reset token for next operation if it was cancelled
                if self.cancellation_token.is_cancelled() {
                    self.cancellation_token = CancellationToken::new();
                }
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

        // Handle auth-status builtin command
        if cmd == "auth-status" {
            return self.handle_auth_status_command().await;
        }

        self.command_orchestrator
            .handle_command(cmd, args, original_input, &mut self.state, &mut self.ui)
            .await
    }

    /// Handle the built-in "auth-status" command
    ///
    /// Checks backend authentication status via GET /api/get-auth
    async fn handle_auth_status_command(&mut self) -> Result<()> {
        match &self.authenticator {
            Some(auth) => {
                self.state
                    .add_output(MessageFormatter::info("Checking authentication status..."));

                match auth.check_status().await {
                    Ok(authenticated) => {
                        if authenticated {
                            self.state.add_output(MessageFormatter::success(
                                "Backend authentication: Active",
                            ));
                        } else {
                            self.state.add_output(MessageFormatter::error(
                                "Backend authentication: Not authenticated",
                            ));
                        }
                    }
                    Err(e) => {
                        self.state.add_output(MessageFormatter::error(format!(
                            "Failed to check auth status: {e}"
                        )));
                    }
                }
            }
            None => {
                self.state.add_output(MessageFormatter::error(
                    "No authenticator configured. Backend authentication not available.",
                ));
            }
        }

        Ok(())
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
    async fn handle_natural_language(
        &mut self,
        query: &str,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        log::info!("Natural language query: {}", query);
        self.state.mode = TerminalMode::WaitingLLM;

        self.nl_orchestrator
            .handle_query(query, &mut self.state, &mut self.ui, cancel_token)
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

/// Authenticate with backend and determine LLM client (with silent fallback)
///
/// This function is designed to run in parallel with the splash screen.
/// Returns: (llm_client, authenticator, using_mock_llm)
async fn authenticate_backend(
    backend_url: String,
    api_key: String,
) -> (
    Arc<dyn LLMClientTrait>,
    Option<Arc<dyn Authenticator>>,
    bool,
) {
    log::info!("Backend URL configured: {}", backend_url);

    let auth = Arc::new(HttpAuthenticator::new(backend_url.clone()));
    match auth.authenticate(&api_key).await {
        Ok(_) => {
            log::info!("Backend authentication successful - using HttpLLMClient");
            // Use backend_url directly for LLM (threads API is at root, not /api/llm)
            let llm_url = std::env::var("INFRAWARE_LLM_URL").unwrap_or(backend_url);
            (
                Arc::new(HttpLLMClient::new(llm_url, api_key)) as Arc<dyn LLMClientTrait>,
                Some(auth as Arc<dyn Authenticator>),
                false,
            )
        }
        Err(e) => {
            // Silent fallback to MockLLMClient
            log::warn!(
                "Authentication failed: {} - falling back to MockLLMClient",
                e
            );
            (Arc::new(MockLLMClient::new()), None, true)
        }
    }
}

/// Main entry point
#[tokio::main]
async fn main() -> Result<()> {
    // Load environment variables from .env file (if present)
    dotenvy::dotenv().ok();

    // Load secrets from .env.secrets file (if present)
    dotenvy::from_filename(".env.secrets").ok();

    // Initialize logging system
    logging::init()?;

    // Set up panic hook to log panics before they crash the app
    std::panic::set_hook(Box::new(|panic_info| {
        let location = panic_info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
            .unwrap_or_else(|| "unknown location".to_string());

        let message = if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
            s.to_string()
        } else if let Some(s) = panic_info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };

        log::error!("PANIC at {}: {}", location, message);
        eprintln!("\n!!! PANIC at {}: {}", location, message);
    }));

    log::info!("Infraware Terminal starting...");

    // Load auth config
    let auth_config = AuthConfig::from_env();

    // Create terminal with defaults (MockLLMClient) - will be updated after auth
    log::debug!("Building terminal UI...");
    let mut terminal = InfrawareTerminal::builder()
        .with_llm_client(Arc::new(MockLLMClient::new()))
        .with_using_mock_llm(true)
        .build()?;

    // Launch auth in background (runs in parallel with splash)
    let auth_handle = match (auth_config.backend_url, auth_config.api_key) {
        (Some(backend_url), Some(api_key)) => Some(tokio::spawn(async move {
            authenticate_backend(backend_url, api_key).await
        })),
        (Some(_), None) => {
            log::warn!("ANTHROPIC_API_KEY not found in .env.secrets");
            log::warn!("Backend not configured - using MockLLMClient");
            None
        }
        _ => {
            log::warn!("Backend not configured - using MockLLMClient");
            None
        }
    };

    // Show animated splash screen (5s) - auth runs in parallel
    log::debug!("Showing splash screen");
    SplashScreen::run(terminal.ui.inner_terminal())?;

    // Wait for auth result and update terminal
    if let Some(handle) = auth_handle {
        match handle.await {
            Ok((llm_client, authenticator, using_mock_llm)) => {
                terminal.set_llm_client(llm_client);
                terminal.set_authenticator(authenticator);
                terminal.set_using_mock_llm(using_mock_llm);
            }
            Err(e) => {
                log::error!("Auth task panicked: {} - using MockLLMClient", e);
            }
        }
    }

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
