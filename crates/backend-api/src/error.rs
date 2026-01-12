//! API error handling
//!
//! Provides structured error responses with error codes for easier client-side handling.

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use serde_json::json;

use infraware_engine::EngineError;

/// Error codes for API responses
///
/// These codes help clients handle errors programmatically without parsing messages.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// Generic internal server error
    InternalError,
    /// Requested resource not found
    NotFound,
    /// Invalid request parameters or body
    BadRequest,
    /// Authentication required or failed
    Unauthorized,
    /// Request rate limit exceeded
    RateLimitExceeded,
    /// Upstream service unavailable
    ServiceUnavailable,
    /// Failed to connect to upstream service
    UpstreamConnectionError,
    /// Thread not found
    ThreadNotFound,
    /// Run cannot be resumed (no pending interrupt)
    RunNotResumable,
    /// Validation error in request
    ValidationError,
}

impl ErrorCode {
    /// Get the HTTP status code for this error code
    fn status_code(self) -> StatusCode {
        match self {
            Self::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            Self::NotFound | Self::ThreadNotFound => StatusCode::NOT_FOUND,
            Self::BadRequest | Self::ValidationError | Self::RunNotResumable => {
                StatusCode::BAD_REQUEST
            }
            Self::Unauthorized => StatusCode::UNAUTHORIZED,
            Self::RateLimitExceeded => StatusCode::TOO_MANY_REQUESTS,
            Self::ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::UpstreamConnectionError => StatusCode::BAD_GATEWAY,
        }
    }
}

/// API error type with structured error response
#[derive(Debug)]
pub struct ApiError {
    code: ErrorCode,
    message: String,
}

impl ApiError {
    /// Create a new API error with the given code and message
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    /// Create an internal server error
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::InternalError, message)
    }

    /// Create a not found error
    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::NotFound, message)
    }

    /// Create a bad request error
    pub fn bad_request(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::BadRequest, message)
    }

    /// Create a validation error
    pub fn validation(message: impl Into<String>) -> Self {
        Self::new(ErrorCode::ValidationError, message)
    }

    /// Create a thread not found error
    pub fn thread_not_found(id: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::ThreadNotFound,
            format!("Thread not found: {}", id.into()),
        )
    }

    /// Create a run not resumable error
    pub fn run_not_resumable(reason: impl Into<String>) -> Self {
        Self::new(
            ErrorCode::RunNotResumable,
            format!("Run not resumable: {}", reason.into()),
        )
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = json!({
            "error": {
                "code": self.code,
                "message": self.message
            }
        });

        (self.code.status_code(), Json(body)).into_response()
    }
}

impl From<EngineError> for ApiError {
    fn from(err: EngineError) -> Self {
        match err {
            EngineError::ThreadNotFound(id) => ApiError::thread_not_found(id),
            EngineError::RunNotResumable(reason) => ApiError::run_not_resumable(reason),
            EngineError::Unhealthy(reason) => ApiError::new(ErrorCode::ServiceUnavailable, reason),
            EngineError::Connection(reason) => {
                ApiError::new(ErrorCode::UpstreamConnectionError, reason)
            }
            _ => ApiError::internal(err.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(err: anyhow::Error) -> Self {
        ApiError::internal(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_code_serialization() {
        let code = ErrorCode::ThreadNotFound;
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"THREAD_NOT_FOUND\"");
    }

    #[test]
    fn test_api_error_response() {
        let error = ApiError::bad_request("Invalid input");
        let response = error.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn test_engine_error_conversion() {
        let engine_error = EngineError::ThreadNotFound("test-123".to_string());
        let api_error: ApiError = engine_error.into();
        assert!(matches!(api_error.code, ErrorCode::ThreadNotFound));
    }
}
