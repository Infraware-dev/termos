//! Shared types for Infraware terminal and agent
//!
//! Contains the API contract types shared between the terminal frontend
//! and the agent.

pub mod events;
pub mod models;
pub mod status;

pub use events::{AgentEvent, IncidentPhase, Interrupt, MessageEvent};
pub use models::{MAX_THREAD_ID_LENGTH, Message, MessageRole, RunInput, ThreadId, ThreadIdError};
pub use status::AgentStatus;
