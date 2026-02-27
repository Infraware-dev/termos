//! Agentic Engine abstraction
//!
//! This module provides the `AgenticEngine` trait that allows swapping
//! different agent implementations:
//! - `MockEngine` - For testing
//! - `RigEngine` - Native rig-rs agent using Anthropic Claude API (default)

// Engine was a standalone library crate — some public API items and
// re-exports are used only by tests or will be used by future consumers.
#![expect(
    dead_code,
    reason = "engine exposes public API surface used by tests and future consumers"
)]
#![expect(unused_imports, reason = "re-exports for engine public API surface")]

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
    AgentEvent, EngineStatus, IncidentPhase, Interrupt, Message, MessageEvent, MessageRole,
    RunInput, ThreadId,
};
pub use traits::{AgenticEngine, EventStream};
pub use types::{HealthStatus, ResumeResponse};
