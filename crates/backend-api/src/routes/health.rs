//! Health check endpoint

use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use serde::Serialize;
use utoipa::ToSchema;

use crate::state::AppState;
use infraware_engine::HealthStatus;

/// Health check response
#[derive(Debug, Serialize, ToSchema)]
pub struct HealthResponse {
    /// Whether the service is healthy
    pub healthy: bool,
    /// Status message
    pub message: String,
    /// Optional additional details
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

impl From<HealthStatus> for HealthResponse {
    fn from(status: HealthStatus) -> Self {
        Self {
            healthy: status.healthy,
            message: status.message,
            details: status.details,
        }
    }
}

/// Check backend and engine health
#[utoipa::path(
    get,
    path = "/health",
    tag = "health",
    responses(
        (status = 200, description = "Service is healthy", body = HealthResponse),
        (status = 503, description = "Service is unhealthy", body = HealthResponse),
    )
)]
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    match state.engine.health_check().await {
        Ok(status) if status.healthy => (StatusCode::OK, Json(status)),
        Ok(status) => (StatusCode::SERVICE_UNAVAILABLE, Json(status)),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthStatus::unhealthy(e.to_string())),
        ),
    }
}
