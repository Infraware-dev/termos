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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_not_found() {
        let err = EngineError::thread_not_found("thread-123");
        assert!(err.to_string().contains("thread-123"));
        assert!(err.to_string().contains("not found"));
    }

    #[test]
    fn test_run_not_resumable() {
        let err = EngineError::run_not_resumable("no pending interrupt");
        assert!(err.to_string().contains("no pending interrupt"));
        assert!(err.to_string().contains("not resumable"));
    }

    #[test]
    fn test_unhealthy() {
        let err = EngineError::unhealthy("connection refused");
        assert!(err.to_string().contains("connection refused"));
        assert!(err.to_string().contains("unhealthy"));
    }

    #[test]
    fn test_connection() {
        let err = EngineError::connection("timeout");
        assert!(err.to_string().contains("timeout"));
        assert!(err.to_string().contains("Connection"));
    }

    #[test]
    fn test_timeout() {
        let err = EngineError::timeout("health_check");
        assert!(err.to_string().contains("health_check"));
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_from_serde_error() {
        let json_err = serde_json::from_str::<String>("invalid").unwrap_err();
        let err: EngineError = json_err.into();
        assert!(err.to_string().contains("Serialization"));
    }

    #[test]
    fn test_from_anyhow() {
        let anyhow_err = anyhow::anyhow!("something went wrong");
        let err: EngineError = anyhow_err.into();
        assert!(err.to_string().contains("something went wrong"));
    }

    #[test]
    fn test_debug_impl() {
        let err = EngineError::thread_not_found("test");
        let debug_str = format!("{:?}", err);
        assert!(debug_str.contains("ThreadNotFound"));
    }
}
