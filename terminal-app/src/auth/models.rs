//! Authentication data models
//!
//! This module contains the request and response structures for the
//! backend authentication API.

use serde::{Deserialize, Serialize};

/// Request payload for POST /api/auth
#[derive(Debug, Serialize)]
pub struct AuthRequest {
    pub api_key: String,
}

/// Response from POST /api/auth on success
#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub success: bool,
    pub message: String,
}

/// Response from GET /api/get-auth
#[derive(Debug, Deserialize)]
#[allow(dead_code)] // Used by check_status() which is part of the API
pub struct AuthStatus {
    pub authenticated: bool,
    /// Whether the backend has an API key configured
    pub has_api_key: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_request_serialization() {
        let request = AuthRequest {
            api_key: "test-key".to_string(),
        };
        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("api_key"));
        assert!(json.contains("test-key"));
    }

    #[test]
    fn test_auth_response_deserialization() {
        let json = r#"{"success": true, "message": "API key validated"}"#;
        let response: AuthResponse = serde_json::from_str(json).unwrap();
        assert!(response.success);
        assert_eq!(response.message, "API key validated");
    }

    #[test]
    fn test_auth_status_deserialization() {
        let json = r#"{"authenticated": true, "has_api_key": true}"#;
        let status: AuthStatus = serde_json::from_str(json).unwrap();
        assert!(status.authenticated);
        assert!(status.has_api_key);
    }
}
