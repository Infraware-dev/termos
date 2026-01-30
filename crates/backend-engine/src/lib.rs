//! Agentic Engine abstraction for Infraware backend
//!
//! This crate provides the `AgenticEngine` trait that allows swapping
//! different agent implementations:
//! - `MockEngine` - For testing
//! - `HttpEngine` - Proxy to LangGraph server (feature: `http`)
//! - `ProcessEngine` - Subprocess via stdio (feature: `process`)
//! - `RigEngine` - Native rig-rs implementation (future)

pub mod adapters;
mod error;
mod traits;
mod types;

#[cfg(feature = "process")]
pub mod ipc;

pub use error::EngineError;
pub use traits::{AgenticEngine, EventStream};
pub use types::{HealthStatus, ResumeResponse};

// Re-export shared types for convenience
pub use infraware_shared::{
    AgentEvent, Interrupt, LLMQueryResult, Message, MessageRole, RunInput, ThreadId,
};
