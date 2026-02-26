//! Agentic Engine abstraction
//!
//! This module provides the `AgenticEngine` trait that allows swapping
//! different agent implementations:
//! - `MockEngine` - For testing
//! - `RigEngine` - Native rig-rs agent using Anthropic Claude API (default)

// Engine was a standalone library crate — allow unused public API items
// and re-exports that are used by tests and will be used by future consumers.
#![allow(dead_code, unused_imports)]

pub mod adapters;
mod error;
pub mod shared;
mod traits;
mod types;

pub use adapters::MockEngine;
#[cfg(feature = "rig")]
pub use adapters::RigEngine;
pub use error::EngineError;
pub use shared::{
    AgentEvent, EngineStatus, IncidentPhase, Interrupt, LLMQueryResult, Message, MessageEvent,
    MessageRole, RunInput, ThreadId,
};
pub use traits::{AgenticEngine, EventStream};
pub use types::{HealthStatus, ResumeResponse};
