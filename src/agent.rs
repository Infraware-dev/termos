//! Agent abstraction
//!
//! This module provides the `Agent` trait that allows swapping
//! different agent implementations:
//! - `MockAgent` - For testing
//! - `RigAgent` - Native rig-rs agent using Anthropic Claude API (default)

// Agent was a standalone library crate — some public API items and
// re-exports are used only by tests or will be used by future consumers.
#![expect(
    dead_code,
    reason = "agent exposes public API surface used by tests and future consumers"
)]
#![expect(unused_imports, reason = "re-exports for agent public API surface")]

pub mod adapters;
mod error;
pub mod shared;
mod traits;
mod types;

pub use adapters::MockAgent;
#[cfg(feature = "rig")]
pub use adapters::RigAgent;
pub use error::AgentError;
pub use shared::{
    AgentEvent, AgentStatus, IncidentPhase, Interrupt, Message, MessageEvent, MessageRole,
    RunInput, ThreadId,
};
pub use traits::{Agent, EventStream};
pub use types::{HealthStatus, ResumeResponse};
