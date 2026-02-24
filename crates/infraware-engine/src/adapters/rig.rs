//! Rig engine adapter using rig-core for native Rust LLM integration
//!
//! This adapter implements the `AgenticEngine` trait using the rig-core library
//! to provide a native Rust agent backed by Anthropic Claude.

mod config;
mod engine;
pub mod incident;
mod memory;
mod orchestrator;
mod shell;
mod state;
mod tools;

pub use config::RigEngineConfig;
pub use engine::RigEngine;
