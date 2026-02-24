//! RigEngine implementation of the AgenticEngine trait

use std::sync::Arc;

use async_trait::async_trait;
use infraware_shared::{RunInput, ThreadId};
use rig::providers::anthropic;
use tokio::sync::RwLock;

use super::config::RigEngineConfig;
use super::orchestrator::{create_resume_stream, create_run_stream};
use super::state::StateStore;
use crate::adapters::rig::memory::session::MemoryStore;
use crate::error::EngineError;
use crate::traits::{AgenticEngine, EventStream};
use crate::types::{HealthStatus, ResumeResponse};

/// Rig-based agentic engine using rig-core
///
/// This engine provides a native Rust implementation backed by
/// the rig-core library and Anthropic's Claude API.
///
/// The Anthropic client is cached to avoid recreating it for each request.
pub struct RigEngine {
    /// Engine configuration
    config: Arc<RigEngineConfig>,
    /// Cached Anthropic client
    client: Arc<anthropic::Client>,
    /// Store for context memory
    memory_store: Arc<RwLock<MemoryStore>>,
    /// State store for threads and runs
    state: Arc<StateStore>,
}

impl std::fmt::Debug for RigEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RigEngine")
            .field("config", &self.config)
            .field("state", &self.state)
            .field("client", &"<anthropic::Client>")
            .finish()
    }
}

impl RigEngine {
    /// Create a new RigEngine with the given configuration
    ///
    /// This creates and caches the Anthropic client for reuse across requests.
    pub fn new(config: RigEngineConfig) -> Result<Self, EngineError> {
        // Validate config
        if config.api_key.is_empty() {
            return Err(EngineError::Other(anyhow::anyhow!(
                "ANTHROPIC_API_KEY is required"
            )));
        }

        // load memory store
        tracing::debug!("Loading memory store");
        let memory_store = Arc::new(RwLock::new(MemoryStore::load_or_create(
            &config.memory.path,
            config.memory.limit,
        )?));

        // Create and cache the Anthropic client
        let client = anthropic::Client::new(&config.api_key).map_err(|e| {
            EngineError::Other(anyhow::anyhow!("Failed to create Anthropic client: {}", e))
        })?;

        tracing::info!(
            model = %config.model,
            max_tokens = %config.max_tokens,
            timeout_secs = %config.timeout_secs,
            "Creating RigEngine with cached Anthropic client"
        );

        Ok(Self {
            config: Arc::new(config),
            client: Arc::new(client),
            memory_store,
            state: Arc::new(StateStore::new()),
        })
    }

    /// Create a RigEngine from environment variables
    pub fn from_env() -> Result<Self, EngineError> {
        let config = RigEngineConfig::from_env()
            .map_err(|e| EngineError::Other(anyhow::anyhow!("Config error: {}", e)))?;
        Self::new(config)
    }
}

#[async_trait]
impl AgenticEngine for RigEngine {
    async fn create_thread(
        &self,
        _metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError> {
        let thread_id = self.state.create_thread().await;
        tracing::info!(thread_id = %thread_id, "Created rig thread");
        Ok(thread_id)
    }

    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError> {
        // Verify thread exists
        if !self.state.thread_exists(thread_id).await {
            return Err(EngineError::thread_not_found(thread_id.as_str()));
        }

        // Generate run ID
        let run_id = format!("rig-run-{}", uuid::Uuid::new_v4());

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            message_count = input.messages.len(),
            "Starting rig run"
        );

        Ok(create_run_stream(
            Arc::clone(&self.config),
            Arc::clone(&self.client),
            Arc::clone(&self.memory_store),
            Arc::clone(&self.state),
            thread_id.clone(),
            input,
            run_id,
        ))
    }

    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError> {
        // Verify thread exists
        if !self.state.thread_exists(thread_id).await {
            return Err(EngineError::thread_not_found(thread_id.as_str()));
        }

        // Generate new run ID for resumed run
        let run_id = format!("rig-run-{}", uuid::Uuid::new_v4());

        tracing::info!(
            thread_id = %thread_id,
            run_id = %run_id,
            response = ?response,
            "Resuming rig run"
        );

        Ok(create_resume_stream(
            Arc::clone(&self.config),
            Arc::clone(&self.client),
            Arc::clone(&self.memory_store),
            Arc::clone(&self.state),
            thread_id.clone(),
            response,
            run_id,
        ))
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        // Basic health check - verify we can create an agent
        // In a production system, we might want to make a lightweight API call
        let thread_count = self.state.thread_count().await;

        Ok(HealthStatus::healthy().with_details(serde_json::json!({
            "engine": "rig",
            "provider": format!("anthropic/{}", self.config.model),
            "threads": thread_count
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RigEngineConfig {
        RigEngineConfig::new("test-api-key")
    }

    #[test]
    fn test_engine_creation() {
        let engine = RigEngine::new(test_config());
        assert!(engine.is_ok());
    }

    #[test]
    fn test_engine_empty_api_key() {
        let config = RigEngineConfig::new("");
        let engine = RigEngine::new(config);
        assert!(engine.is_err());
    }

    #[tokio::test]
    async fn test_create_thread() {
        let engine = RigEngine::new(test_config()).unwrap();
        let thread_id = engine.create_thread(None).await.unwrap();
        assert!(thread_id.as_str().starts_with("rig-thread-"));
    }

    #[tokio::test]
    async fn test_thread_not_found() {
        let engine = RigEngine::new(test_config()).unwrap();
        let fake_id = ThreadId::new("nonexistent");
        let input = RunInput::single_user_message("Hello");

        let result = engine.stream_run(&fake_id, input).await;
        assert!(matches!(result, Err(EngineError::ThreadNotFound(_))));
    }

    #[tokio::test]
    async fn test_health_check() {
        let engine = RigEngine::new(test_config()).unwrap();
        let status = engine.health_check().await.unwrap();
        assert!(status.healthy);
    }
}
