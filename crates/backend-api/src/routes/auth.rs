//! Authentication endpoint

use axum::{Json, extract::State, http::HeaderMap};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::error::ApiError;
use crate::state::AppState;

/// Authentication request body (optional, key can also come from header)
#[derive(Debug, Deserialize, ToSchema)]
pub struct AuthRequest {
    /// API key for authentication (alternative to X-Api-Key header)
    pub api_key: Option<String>,
}

/// Authentication response
#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    /// Whether authentication was successful
    pub success: bool,
    /// Human-readable status message
    pub message: String,
    /// Whether authentication is enabled on this server
    pub auth_enabled: bool,
}

/// Authenticate client
///
/// Validates the provided API key against the server's configured key.
/// The API key can be provided either in the X-Api-Key header or in the request body.
/// If authentication is disabled on the server (no API_KEY configured), always returns success.
#[utoipa::path(
    post,
    path = "/api/auth",
    tag = "auth",
    request_body = AuthRequest,
    responses(
        (status = 200, description = "Authentication result", body = AuthResponse),
        (status = 400, description = "Invalid request"),
    ),
    params(
        ("X-Api-Key" = Option<String>, Header, description = "API key for authentication"),
    )
)]
pub async fn authenticate(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Option<Json<AuthRequest>>,
) -> Result<Json<AuthResponse>, ApiError> {
    // If auth is disabled, always succeed
    if !state.auth_config.is_enabled() {
        return Ok(Json(AuthResponse {
            success: true,
            message: "Authentication disabled on server".to_string(),
            auth_enabled: false,
        }));
    }

    // Try to get API key from header first, then body
    let api_key = headers
        .get("X-Api-Key")
        .and_then(|v| v.to_str().ok())
        .map(String::from)
        .or_else(|| body.and_then(|b| b.api_key.clone()));

    match api_key {
        Some(key) if state.auth_config.validate(&key) => Ok(Json(AuthResponse {
            success: true,
            message: "Authentication successful".to_string(),
            auth_enabled: true,
        })),
        Some(_) => Ok(Json(AuthResponse {
            success: false,
            message: "Invalid API key".to_string(),
            auth_enabled: true,
        })),
        None => Ok(Json(AuthResponse {
            success: false,
            message: "No API key provided".to_string(),
            auth_enabled: true,
        })),
    }
}
