//! Core trait definition for agents

use std::fmt::Debug;
use std::pin::Pin;

use async_trait::async_trait;
use futures::Stream;

use super::error::AgentError;
use super::shared::{AgentEvent, RunInput, ThreadId};
use super::types::{HealthStatus, ResumeResponse};

/// Type alias for the event stream returned by runs
pub type EventStream = Pin<Box<dyn Stream<Item = Result<AgentEvent, AgentError>> + Send>>;

/// Trait for agent implementations
///
/// This trait abstracts over different agent backends:
/// - [`MockAgent`](crate::agent::adapters::MockAgent) - Returns canned responses for testing
/// - [`RigAgent`](crate::agent::adapters::RigAgent) - Native rig-rs agent using Anthropic Claude API
///
/// All implementations must be thread-safe (`Send + Sync`).
#[async_trait]
pub trait Agent: Send + Sync + Debug {
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
    ) -> Result<ThreadId, AgentError>;

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
    ) -> Result<EventStream, AgentError>;

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
    ) -> Result<EventStream, AgentError>;

    /// Check agent health
    ///
    /// # Returns
    /// The health status of the agent
    async fn health_check(&self) -> Result<HealthStatus, AgentError>;
}
