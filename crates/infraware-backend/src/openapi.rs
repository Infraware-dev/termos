//! OpenAPI documentation configuration

use utoipa::OpenApi;

use crate::routes::{auth, health, threads};

/// OpenAPI documentation for the Infraware Backend API
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Infraware Backend API",
        version = "0.2.0",
        description = "REST/SSE API for the Infraware terminal client",
        license(name = "MIT"),
    ),
    paths(
        health::health_check,
        auth::authenticate,
        threads::create_thread,
        threads::stream_run,
    ),
    components(
        schemas(
            health::HealthResponse,
            auth::AuthRequest,
            auth::AuthResponse,
            threads::CreateThreadRequest,
            threads::CreateThreadResponse,
            threads::StreamRunRequest,
            threads::InputContainer,
            threads::MessageInput,
            threads::CommandContainer,
        )
    ),
    tags(
        (name = "health", description = "Health check endpoints"),
        (name = "auth", description = "Authentication endpoints"),
        (name = "threads", description = "Thread and run management"),
    )
)]
pub struct ApiDoc;
