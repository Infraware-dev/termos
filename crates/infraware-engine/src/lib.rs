//! Agentic Engine abstraction for Infraware backend
//!
//! This crate provides the `AgenticEngine` trait that allows swapping
//! different agent implementations:
//! - `MockEngine` - For testing
//! - `RigEngine` - Native rig-rs agent using Anthropic Claude API (default)

pub mod adapters;
mod error;
mod traits;
mod types;

pub use error::EngineError;
// Re-export shared types for convenience
pub use infraware_shared::{
    AgentEvent, IncidentPhase, Interrupt, LLMQueryResult, Message, MessageRole, RunInput, ThreadId,
};
pub use traits::{AgenticEngine, EventStream};
pub use types::{HealthStatus, ResumeResponse};
