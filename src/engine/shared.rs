//! Shared types for Infraware terminal and engine
//!
//! Contains the API contract types shared between the terminal frontend
//! and the agentic engine.

pub mod events;
pub mod models;
pub mod status;

pub use events::{AgentEvent, IncidentPhase, Interrupt, MessageEvent};
pub use models::{
    LLMQueryResult, MAX_THREAD_ID_LENGTH, Message, MessageRole, RunInput, ThreadId, ThreadIdError,
};
pub use status::EngineStatus;
