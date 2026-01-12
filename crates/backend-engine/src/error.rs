//! Engine error types

use thiserror::Error;

/// Errors that can occur in the agentic engine
#[derive(Debug, Error)]
pub enum EngineError {
    /// Thread not found
    #[error("Thread not found: {0}")]
    ThreadNotFound(String),

    /// Run not found or not resumable
    #[error("Run not resumable: {0}")]
    RunNotResumable(String),

    /// Engine is not healthy
    #[error("Engine unhealthy: {0}")]
    Unhealthy(String),

    /// Connection error to underlying engine
    #[error("Connection error: {0}")]
    Connection(String),

    /// Operation timed out
    #[error("Operation timed out: {0}")]
    Timeout(String),

    /// Serialization/deserialization error
    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// Generic engine error
    #[error("Engine error: {0}")]
    Other(#[from] anyhow::Error),
}

impl EngineError {
    pub fn thread_not_found(id: impl Into<String>) -> Self {
        Self::ThreadNotFound(id.into())
    }

    pub fn run_not_resumable(reason: impl Into<String>) -> Self {
        Self::RunNotResumable(reason.into())
    }

    pub fn unhealthy(reason: impl Into<String>) -> Self {
        Self::Unhealthy(reason.into())
    }

    pub fn connection(reason: impl Into<String>) -> Self {
        Self::Connection(reason.into())
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout(operation.into())
    }
}
