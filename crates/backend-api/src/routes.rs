//! API route handlers

pub mod auth;
pub mod health;
pub mod threads;

use axum::Router;
use axum::routing::post;

use crate::state::AppState;

/// Routes under /api
pub fn api_routes() -> Router<AppState> {
    Router::new().route("/auth", post(auth::authenticate))
}

/// Routes under /threads
pub fn thread_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(threads::create_thread))
        .route("/{thread_id}/runs/stream", post(threads::stream_run))
}
