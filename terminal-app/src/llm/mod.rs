//! LLM client module for natural language queries.

pub mod client;
pub mod renderer;

pub use client::{LLMClientTrait, HttpLLMClient, MockLLMClient, LLMQueryResult};
pub use renderer::ResponseRenderer;