//! Additional types for engine operations

use serde::{Deserialize, Serialize};

/// Health status of the engine
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    /// Whether the engine is healthy
    pub healthy: bool,
    /// Human-readable status message
    pub message: String,
    /// Optional details (e.g., latency, version)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl HealthStatus {
    pub fn healthy() -> Self {
        Self {
            healthy: true,
            message: "OK".to_string(),
            details: None,
        }
    }

    pub fn unhealthy(message: impl Into<String>) -> Self {
        Self {
            healthy: false,
            message: message.into(),
            details: None,
        }
    }

    pub fn with_details(mut self, details: serde_json::Value) -> Self {
        self.details = Some(details);
        self
    }
}

/// Response for resuming an interrupted run
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ResumeResponse {
    /// User approved the command
    Approved,
    /// User rejected the command
    Rejected,
    /// User provided an answer to a question
    Answer { text: String },
}

impl ResumeResponse {
    pub fn approved() -> Self {
        Self::Approved
    }

    pub fn rejected() -> Self {
        Self::Rejected
    }

    pub fn answer(text: impl Into<String>) -> Self {
        Self::Answer { text: text.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_health_status_healthy() {
        let status = HealthStatus::healthy();
        assert!(status.healthy);
        assert_eq!(status.message, "OK");
    }

    #[test]
    fn test_health_status_unhealthy() {
        let status = HealthStatus::unhealthy("Connection failed");
        assert!(!status.healthy);
        assert_eq!(status.message, "Connection failed");
    }

    #[test]
    fn test_resume_response() {
        let approved = ResumeResponse::approved();
        assert!(matches!(approved, ResumeResponse::Approved));

        let answer = ResumeResponse::answer("Option A");
        match answer {
            ResumeResponse::Answer { text } => assert_eq!(text, "Option A"),
            _ => panic!("Expected Answer"),
        }
    }
}
