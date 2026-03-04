//! Rig agent adapter using rig-core for native Rust LLM integration
//!
//! This adapter implements the `Agent` trait using the rig-core library
//! to provide a native Rust agent backed by Anthropic Claude.

mod agent;
mod config;
pub mod incident;
mod memory;
mod orchestrator;
mod shell;
mod state;
mod tools;

pub use agent::RigAgent;
pub use config::RigAgentConfig;
