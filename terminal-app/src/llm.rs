//! LLM client module for natural language queries.

pub mod client;
pub mod renderer;

pub use client::{HttpLLMClient, LLMClientTrait, LLMQueryResult, MockLLMClient};
pub use renderer::ResponseRenderer;
