//! Shared types for Infraware terminal and backend
//!
//! This crate contains the API contract types shared between:
//! - `terminal-app`: The egui terminal client
//! - `backend-api`: The axum backend server

pub mod events;
pub mod models;
pub mod status;

pub use events::{AgentEvent, Interrupt, MessageEvent};
pub use models::{
    LLMQueryResult, MAX_THREAD_ID_LENGTH, Message, MessageRole, RunInput, ThreadId, ThreadIdError,
};
pub use status::EngineStatus;
