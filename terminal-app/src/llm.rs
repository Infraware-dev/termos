//! LLM client module for natural language queries.

pub mod client;
pub mod incremental_renderer;
pub mod renderer;

pub use client::{HttpLLMClient, LLMClientTrait, LLMQueryResult, LLMStreamEvent, MockLLMClient};
pub use incremental_renderer::IncrementalRenderer;
pub use renderer::ResponseRenderer;
