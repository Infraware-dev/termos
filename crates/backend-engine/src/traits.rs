//! Core trait definition for agentic engines

use std::fmt::Debug;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use crate::error::EngineError;
use crate::types::{HealthStatus, ResumeResponse};
use infraware_shared::{AgentEvent, RunInput, ThreadId};

/// Type alias for the event stream returned by runs
pub type EventStream = Pin<Box<dyn Stream<Item = Result<AgentEvent, EngineError>> + Send>>;

/// Trait for agentic engine implementations
///
/// This trait abstracts over different agent backends:
/// - [`MockEngine`](crate::adapters::MockEngine) - Returns canned responses for testing
/// - [`HttpEngine`](crate::adapters::HttpEngine) - Proxies to a LangGraph server via HTTP
/// - [`ProcessEngine`](crate::adapters::ProcessEngine) - Communicates with a subprocess via stdio
/// - [`RigEngine`](crate::adapters::RigEngine) - Native rig-rs implementation (future)
///
/// All implementations must be thread-safe (`Send + Sync`).
#[async_trait]
pub trait AgenticEngine: Send + Sync + Debug {
    /// Create a new conversation thread
    ///
    /// # Arguments
    /// * `metadata` - Optional metadata to attach to the thread
    ///
    /// # Returns
    /// The ID of the newly created thread
    async fn create_thread(
        &self,
        metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError>;

    /// Start a run and stream events
    ///
    /// # Arguments
    /// * `thread_id` - The thread to run on
    /// * `input` - The input messages
    ///
    /// # Returns
    /// A stream of agent events
    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError>;

    /// Resume an interrupted run after HITL response
    ///
    /// # Arguments
    /// * `thread_id` - The thread with the interrupted run
    /// * `response` - The user's response to the interrupt
    ///
    /// # Returns
    /// A stream of agent events continuing from the interrupt
    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError>;

    /// Check engine health
    ///
    /// # Returns
    /// The health status of the engine
    async fn health_check(&self) -> Result<HealthStatus, EngineError>;
}
