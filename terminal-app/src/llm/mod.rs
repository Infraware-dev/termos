//! LLM client module for natural language queries.
//!
//! This module provides an LLM client for sending failed commands to the backend
//! when "command not found" is detected.

mod client;

pub use client::{LLMClient, LLMQueryResult};
