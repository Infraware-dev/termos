/// Infraware Terminal - Hybrid Command Interpreter with AI Assistance
///
/// This is a TUI-based terminal that accepts user input and intelligently
/// routes it to either:
/// 1. Shell command execution (with auto-install for missing commands)
/// 2. LLM backend for natural language queries
///
/// Target use case: DevOps operations in cloud environments (AWS/Azure) with AI assistance
use infraware_terminal::{auth, executor, input, llm, logging, orchestrators, terminal, utils};

use anyhow::Result;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

/// Interval between background job completion checks.
///
/// Why 250ms: Balances responsiveness vs overhead. Users perceive <300ms as
/// "instant", so job completion notifications feel immediate. This avoids
/// acquiring write locks on every keystroke event.
///
/// Trade-offs: Higher values delay "Done" notifications. Lower values increase
/// lock contention and CPU usage with many background jobs.
const JOB_CHECK_INTERVAL: Duration = Duration::from_millis(250);

use auth::{AuthConfig, Authenticator, HttpAuthenticator};
use executor::{create_shared_job_manager, SharedJobManager};
use input::{InputClassifier, InputType};
use llm::{HttpLLMClient, LLMClientTrait, MockLLMClient, ResponseRenderer};
use orchestrators::{
    CommandOrchestrator, HitlOrchestrator, NaturalLanguageOrchestrator, TabCompletionHandler,
};
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
    /// Watch channel sender for cancellation token - allows sharing current token with poller
    /// Main loop can send new token to reset, poller can read current token to cancel
    cancellation_token_tx: watch::Sender<CancellationToken>,
    /// Shared job manager for background processes
    job_manager: SharedJobManager,
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
                "cancellation_token_tx",
                &format!(
                    "watch::Sender<CancellationToken>(cancelled={})",
                    self.cancellation_token_tx.borrow().is_cancelled()
                ),
            )
            .field("job_manager", &"<SharedJobManager>")
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
            // Create watch channel for cancellation token sharing with poller
            cancellation_token_tx: watch::channel(CancellationToken::new()).0,
            // Create shared job manager for background processes
            job_manager: create_shared_job_manager(),
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

        // Set initial window title
        let title = self.state.get_window_title();
        let _ = self.ui.set_window_title(&title);

        // Initial render
        self.ui.render(&mut self.state)?;

        // Create channel for events from background polling task
        let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<TerminalEvent>(32);

        // Subscribe to cancellation token via watch channel
        // This allows poller to always see the current token (even after resets)
        let cancel_token_rx = self.cancellation_token_tx.subscribe();

        // Spawn background task for event polling
        // This task runs independently and can cancel async operations on Ctrl+C
        let poll_handle = tokio::task::spawn_blocking(move || {
            let event_handler = EventHandler::new();
            loop {
                // Check if main loop has exited (receiver dropped)
                if event_tx.is_closed() {
                    log::info!("Event poller: channel closed, stopping");
                    break;
                }

                // Poll with short timeout
                match event_handler.poll_event(Duration::from_millis(50)) {
                    Ok(Some(event)) => {
                        // For CtrlC, cancel the current token immediately
                        // This interrupts any ongoing async operation (e.g., LLM query)
                        // The main loop will handle the event and clear input after
                        if matches!(event, TerminalEvent::CtrlC) {
                            log::info!("Event poller: CtrlC detected, cancelling current token");
                            cancel_token_rx.borrow().cancel();
                        }
                        // Send event to main loop (exit if channel closed)
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

        // Track last job check time for periodic checking (reduces lock contention)
        let mut last_job_check = Instant::now();

        // Main event loop - receives events from background poller
        loop {
            // Wait for event with timeout
            // Note: We don't check cancellation_token here because it's used
            // to cancel async operations (LLM queries), not to exit the app.
            // App exit is handled by the Quit event handler returning Ok(false).
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
                _ = tokio::time::sleep(Duration::from_millis(100)) => {
                    // Timeout - check jobs periodically even without events
                    if last_job_check.elapsed() >= JOB_CHECK_INTERVAL {
                        self.check_completed_jobs();
                        last_job_check = Instant::now();
                        self.ui.render(&mut self.state)?;
                    }
                    continue;
                }
            };

            // Handle the event
            if !self.handle_event(event).await? {
                break; // Quit requested
            }

            // Check for completed background jobs periodically (not on every event)
            // This reduces lock contention during rapid typing
            if last_job_check.elapsed() >= JOB_CHECK_INTERVAL {
                self.check_completed_jobs();
                last_job_check = Instant::now();
            }

            // Re-render after handling event
            self.ui.render(&mut self.state)?;
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
                self.cancellation_token_tx.borrow().cancel();
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
            TerminalEvent::CtrlC => {
                // Poller already cancelled the token (for async interruption)
                // Here we just clear input and conditionally reset token
                log::info!("Ctrl+C: Clearing input (mode={:?})", self.state.mode);
                self.state.clear_input();

                // Cancel multiline input if in multiline mode
                if self.state.is_in_multiline_mode() {
                    log::info!("Ctrl+C: Cancelling multiline input");
                    self.state.cancel_multiline();
                    self.state
                        .add_output(MessageFormatter::info("Multiline input cancelled"));
                }

                // Reset token only if:
                // 1. Token was cancelled (by poller)
                // 2. We're NOT in WaitingLLM mode (avoid race with async operation checking token)
                // If in WaitingLLM, the async handler will reset the token after it sees cancellation
                if self.cancellation_token_tx.borrow().is_cancelled()
                    && self.state.mode != TerminalMode::WaitingLLM
                {
                    log::info!("Ctrl+C: Resetting cancellation token");
                    let _ = self.cancellation_token_tx.send(CancellationToken::new());
                }
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
        use crate::input::multiline::{is_incomplete, is_multiline_complete, join_lines};

        let input = self.state.submit_input();

        // Handle human-in-the-loop command approval mode (y/n)
        if self.state.mode == TerminalMode::AwaitingCommandApproval {
            let approved = HitlOrchestrator::parse_approval(&input);
            self.state.add_output(MessageFormatter::command(&input));

            // Check if this is a shell confirmation (rm on write-protected files, etc.)
            if CommandOrchestrator::is_shell_confirmation(&self.state) {
                // Delegate to command orchestrator for shell confirmations
                self.command_orchestrator
                    .handle_shell_confirmation(approved, &mut self.state)
                    .await?;
                return Ok(true);
            }

            // Delegate to NL orchestrator for LLM approval handling
            self.nl_orchestrator
                .handle_approval(approved, &mut self.state, &mut self.ui)
                .await?;

            return Ok(true);
        }

        // Handle human-in-the-loop answer mode (free-form text)
        if self.state.mode == TerminalMode::AwaitingAnswer {
            // Check if this is a sudo password prompt
            if CommandOrchestrator::is_waiting_for_sudo_password(&self.state) {
                // Don't echo the password!
                self.state.add_output("Verifying...".to_string());

                // Clear pending interaction
                self.state.pending_interaction = None;
                self.state.mode = TerminalMode::Normal;

                // Force render to show "Verifying..." before blocking on sudo
                self.ui.render(&mut self.state)?;

                // Validate password (sudo has ~2s delay on wrong password for security)
                self.command_orchestrator
                    .validate_sudo_password(input, &mut self.state)
                    .await?;

                return Ok(true);
            }

            self.state.add_output(MessageFormatter::command(&input));

            // Delegate to orchestrator for answer handling
            self.nl_orchestrator
                .handle_answer(input, &mut self.state, &mut self.ui)
                .await?;

            return Ok(true);
        }

        // Handle multiline input mode
        if self.state.is_in_multiline_mode() {
            // Add current line to multiline buffer
            self.state.multiline_buffer.push(input.clone());

            // Check if we're now complete
            if let Some(reason) = is_multiline_complete(&self.state.multiline_buffer) {
                // Still incomplete, update the reason and wait for more input
                self.state.mode = TerminalMode::AwaitingMoreInput(reason);
                return Ok(true);
            }

            // Complete! Join lines and process as single input
            let full_input = join_lines(&self.state.multiline_buffer);
            self.state.multiline_buffer.clear();
            self.state.pending_heredoc = None;
            self.state.mode = TerminalMode::Normal;

            // Process the full multiline input (recursive call with joined input)
            return self.handle_submit_with_input(full_input).await;
        }

        // Check if this single line is incomplete (needs more input)
        if let Some(reason) = is_incomplete(&input, self.state.pending_heredoc.as_deref()) {
            // Track heredoc delimiter if starting one
            if let crate::input::IncompleteReason::HeredocPending { ref delimiter } = reason {
                self.state.pending_heredoc = Some(delimiter.clone());
            }

            // Start multiline mode
            self.state.multiline_buffer.push(input);
            self.state.mode = TerminalMode::AwaitingMoreInput(reason);
            return Ok(true);
        }

        if input.trim().is_empty() {
            return Ok(true);
        }

        // Process complete single-line input
        self.handle_submit_with_input(input).await
    }

    /// Handle input submission with a specific input string
    /// This is used both for single-line and joined multiline input
    async fn handle_submit_with_input(&mut self, input: String) -> Result<bool> {
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
                // Handle exit builtin
                if command == "exit" {
                    if self.state.is_root_mode() {
                        // In root mode, exit returns to normal user
                        self.state.exit_root_mode();
                        self.state
                            .add_output(MessageFormatter::success("Exited root mode."));
                        return Ok(true);
                    } else {
                        // In normal mode, exit quits the terminal
                        self.state.add_output(MessageFormatter::info("Goodbye!"));
                        self.cancellation_token_tx.borrow().cancel(); // Signal any async operations
                        return Ok(false);
                    }
                }

                // Handle cd builtin - must be handled by parent process
                if command == "cd" {
                    self.state.add_output(MessageFormatter::command(&input));
                    self.handle_cd_command(&args);
                    return Ok(true);
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

                // Set mode BEFORE awaiting to prevent race condition with Ctrl+C
                // If mode is set inside handle_natural_language, Ctrl+C pressed immediately
                // after Enter will see mode=Normal and clear input instead of cancelling
                self.state.mode = TerminalMode::WaitingLLM;

                // Clone current token (cheap Arc increment) for this operation
                let token = self.cancellation_token_tx.borrow().clone();
                self.handle_natural_language(&query, token.clone()).await?;

                // Reset token if THIS specific token was cancelled (not checking channel's current token)
                // This avoids TOCTOU: we check the exact token used for this operation
                if token.is_cancelled() {
                    log::info!("LLM query was cancelled, resetting token for next operation");
                    let _ = self.cancellation_token_tx.send(CancellationToken::new());
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

        // Only reset to Normal if not in a HITL waiting state
        // (handle_query_result may have set AwaitingCommandApproval or AwaitingAnswer)
        if !self.state.is_in_hitl_mode() {
            self.state.mode = TerminalMode::Normal;
        }

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
            .handle_command(
                cmd,
                args,
                original_input,
                &mut self.state,
                &mut self.ui,
                &self.job_manager,
            )
            .await
    }

    /// Handle the built-in "cd" command
    ///
    /// Changes the working directory of the terminal process.
    /// Unlike shell builtins, this must be handled directly by the parent process.
    ///
    /// Supported forms:
    /// - `cd` or `cd ~` → home directory
    /// - `cd ..` → parent directory
    /// - `cd /path` → absolute path
    /// - `cd path` → relative path
    fn handle_cd_command(&mut self, args: &[String]) {
        let target = args.first().map(|s| s.as_str()).unwrap_or("");

        let path = if target.is_empty() || target == "~" {
            // cd or cd ~ -> home directory
            dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"))
        } else if let Some(suffix) = target.strip_prefix("~/") {
            // cd ~/path -> expand ~ to home
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
            home.join(suffix)
        } else {
            // cd path (relative or absolute)
            PathBuf::from(target)
        };

        match std::env::set_current_dir(&path) {
            Ok(()) => {
                // Show the new directory
                if let Ok(cwd) = std::env::current_dir() {
                    self.state
                        .add_output(MessageFormatter::success(cwd.display().to_string()));
                }
                // Update prompt cache and window title
                self.state.refresh_prompt();
                let title = self.state.get_window_title();
                let _ = self.ui.set_window_title(&title);
            }
            Err(e) => {
                self.state.add_output(MessageFormatter::error(format!(
                    "cd: {}: {}",
                    path.display(),
                    e
                )));
            }
        }
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

    /// Check for completed background jobs and display notifications
    ///
    /// Called periodically (not on every event) to provide timely feedback
    /// when background processes complete while minimizing lock contention.
    fn check_completed_jobs(&mut self) {
        // Fast path: check if there are any jobs with read lock first
        let has_jobs = {
            match self.job_manager.read() {
                Ok(guard) => guard.has_running_jobs(),
                Err(_poisoned) => {
                    // Lock poisoning indicates previous panic violated invariants.
                    // Log error and skip this check - don't recover with corrupted state.
                    log::error!(
                        "JobManager lock poisoned during check_completed_jobs (read). \
                         Skipping job check to avoid potential state corruption."
                    );
                    return;
                }
            }
        };

        if !has_jobs {
            return; // No jobs, skip expensive write lock
        }

        // Jobs exist, acquire write lock to check completion
        let completed: Vec<executor::JobInfo> = {
            let mut mgr = match self.job_manager.write() {
                Ok(guard) => guard,
                Err(_poisoned) => {
                    // Lock poisoning indicates previous panic violated invariants.
                    // Log error and skip - don't recover with corrupted state.
                    log::error!(
                        "JobManager lock poisoned during check_completed_jobs (write). \
                         Skipping job check to avoid potential state corruption."
                    );
                    return;
                }
            };
            mgr.check_completed()
        };

        for job in completed {
            let exit_info = match job.status {
                executor::JobStatus::Done(code) => format!("exit: {}", code),
                executor::JobStatus::Terminated => "terminated".to_string(),
                executor::JobStatus::Running => continue, // Should not happen
            };
            self.state
                .add_output(format!("[{}] Done ({}) {}", job.id, exit_info, job.command));
        }
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
                    .handle_command(
                        &command,
                        &args,
                        None,
                        &mut self.state,
                        &mut self.ui,
                        &self.job_manager,
                    )
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
    /// Note: state.mode is set to WaitingLLM in handle_submit() BEFORE calling this,
    /// to prevent race condition with Ctrl+C cancellation detection
    async fn handle_natural_language(
        &mut self,
        query: &str,
        cancel_token: CancellationToken,
    ) -> Result<()> {
        log::info!("Natural language query: {}", query);

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
