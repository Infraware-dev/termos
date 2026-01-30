//! LLM query management and background event handling.
//!
//! Provides `LlmController` which manages LLM client lifecycle, query execution,
//! response rendering, and background event channels.

use super::AppBackgroundEvent;
use crate::auth::{AuthConfig, Authenticator, HttpAuthenticator};
use crate::llm::{HttpLLMClient, LLMClientTrait, ResponseRenderer};
use std::sync::Arc;
use std::sync::mpsc;
use tokio::runtime::Runtime;
use tokio_util::sync::CancellationToken;

/// LLM client, response renderer, and background event channels.
pub struct LlmController {
    /// LLM client for natural language queries
    pub client: Arc<dyn LLMClientTrait>,
    /// Response renderer (markdown → ANSI)
    pub response_renderer: ResponseRenderer,
    /// Channel for background events (sender)
    bg_event_tx: mpsc::Sender<AppBackgroundEvent>,
    /// Channel for background events (receiver)
    bg_event_rx: mpsc::Receiver<AppBackgroundEvent>,
    /// Cancellation token for active LLM queries
    cancel_token: Option<CancellationToken>,
}

impl std::fmt::Debug for LlmController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LlmController")
            .field("client", &self.client)
            .field("cancel_token", &self.cancel_token.is_some())
            .finish()
    }
}

impl LlmController {
    /// Creates a new LLM controller with authentication.
    pub fn new(runtime: &Runtime) -> Self {
        let (bg_event_tx, bg_event_rx) = mpsc::channel();

        let auth_config = AuthConfig::from_env();
        let llm_client: Arc<dyn LLMClientTrait> = Self::create_client(&auth_config, runtime);

        Self {
            client: llm_client,
            response_renderer: ResponseRenderer::new(),
            bg_event_tx,
            bg_event_rx,
            cancel_token: None,
        }
    }

    /// Creates an LLM client based on authentication config.
    fn create_client(auth_config: &AuthConfig, runtime: &Runtime) -> Arc<dyn LLMClientTrait> {
        if auth_config.is_configured() {
            let backend_url = auth_config
                .backend_url
                .clone()
                .expect("backend_url must be Some when is_configured() returns true");
            let api_key = auth_config
                .api_key
                .clone()
                .expect("api_key must be Some when is_configured() returns true");

            let authenticator = HttpAuthenticator::new(backend_url.clone());
            let auth_result =
                runtime.block_on(async { authenticator.authenticate(&api_key).await });

            match auth_result {
                Ok(response) if response.success => {
                    log::info!("Authentication successful: {}", response.message);
                    Arc::new(HttpLLMClient::new(backend_url, api_key))
                }
                Ok(response) => {
                    log::warn!("Authentication rejected: {}", response.message);
                    Arc::new(crate::llm::MockLLMClient::new())
                }
                Err(e) => {
                    log::error!("Authentication failed: {}", e);
                    Arc::new(crate::llm::MockLLMClient::new())
                }
            }
        } else {
            log::warn!("No API key configured, using Mock LLM Client");
            Arc::new(crate::llm::MockLLMClient::new())
        }
    }

    /// Starts an LLM query in a background task.
    pub fn start_query(&mut self, runtime: &Runtime, query: String) {
        log::info!("Starting LLM query: {}", query);

        let llm_client = self.client.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        runtime.spawn(async move {
            log::info!("Background task started for query: {}", query);

            match llm_client.query_cancellable(&query, cancel_token).await {
                Ok(result) => {
                    log::info!("LLM query succeeded, sending result to channel");
                    if let Err(e) = tx.send(AppBackgroundEvent::LlmResult(result)) {
                        log::error!("Failed to send LLM result to channel: {}", e);
                    }
                }
                Err(e) => {
                    log::error!("LLM query failed: {}", e);
                    if let Err(send_err) = tx.send(AppBackgroundEvent::LlmError(e.to_string())) {
                        log::error!("Failed to send error to channel: {}", send_err);
                    }
                }
            }
            log::info!("Background task completed");
        });
    }

    /// Resumes LLM run with a text answer.
    pub fn resume_with_answer(&mut self, runtime: &Runtime, answer: String) {
        let llm_client = self.client.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM answer run cancelled");
                }
                result = llm_client.resume_with_answer(&answer) => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Resumes LLM run with command output from PTY execution.
    pub fn resume_with_command_output(
        &mut self,
        runtime: &Runtime,
        command: String,
        output: String,
    ) {
        let llm_client = self.client.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM command output run cancelled");
                }
                result = llm_client.resume_with_command_output(&command, &output) => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Resumes LLM run with rejection (user rejected the command).
    pub fn resume_rejected(&mut self, runtime: &Runtime) {
        let llm_client = self.client.clone();
        let tx = self.bg_event_tx.clone();

        let cancel_token = CancellationToken::new();
        self.cancel_token = Some(cancel_token.clone());

        runtime.spawn(async move {
            tokio::select! {
                _ = cancel_token.cancelled() => {
                    log::info!("Background LLM rejected run cancelled");
                }
                result = llm_client.resume_rejected() => {
                    match result {
                        Ok(r) => { let _ = tx.send(AppBackgroundEvent::LlmResult(r)); }
                        Err(e) => { let _ = tx.send(AppBackgroundEvent::LlmError(e.to_string())); }
                    }
                }
            }
        });
    }

    /// Polls and returns pending background events (non-blocking).
    pub fn poll_events(&mut self) -> Vec<AppBackgroundEvent> {
        let mut events = Vec::new();
        while let Ok(event) = self.bg_event_rx.try_recv() {
            events.push(event);
        }
        events
    }

    /// Cancels the active LLM query if one exists.
    pub fn cancel(&mut self) {
        if let Some(token) = self.cancel_token.take() {
            log::info!("Cancelling active LLM stream");
            token.cancel();
        }
    }
}
