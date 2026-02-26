//! Application state

use std::sync::Arc;

use infraware_engine::AgenticEngine;

use crate::auth_middleware::AuthConfig;

/// Shared application state
#[derive(Clone)]
pub struct AppState {
    /// The agentic engine for LLM interactions
    pub engine: Arc<dyn AgenticEngine>,
    /// Authentication configuration
    pub auth_config: AuthConfig,
}

impl AppState {
    /// Create a new AppState with the given engine and auth config
    pub fn new(engine: Arc<dyn AgenticEngine>, auth_config: AuthConfig) -> Self {
        Self {
            engine,
            auth_config,
        }
    }
}
