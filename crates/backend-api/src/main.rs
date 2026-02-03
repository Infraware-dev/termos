//! Infraware Backend API Server
//!
//! An axum-based backend that exposes REST/SSE endpoints for the terminal client.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use axum::Router;
use axum::http::Method;
use axum::response::IntoResponse;
use axum::routing::get;
use infraware_engine::AgenticEngine;
use infraware_engine::adapters::{
    HttpEngine, HttpEngineConfig, MockEngine, ProcessEngine, ProcessEngineConfig, Workflow,
};
#[cfg(feature = "rig")]
use infraware_engine::adapters::{RigEngine, RigEngineConfig};
use metrics_exporter_prometheus::{Matcher, PrometheusBuilder, PrometheusHandle};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use utoipa::OpenApi;

mod auth_middleware;

mod error;
mod openapi;
mod routes;
mod state;

use openapi::ApiDoc;
use state::AppState;

/// Serve OpenAPI JSON spec
async fn serve_openapi() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}

/// Setup Prometheus metrics exporter
fn setup_metrics() -> PrometheusHandle {
    /// Histogram bucket boundaries for HTTP request duration metrics.
    /// Based on typical web service latencies: 5ms to 10s in exponential steps.
    /// - Fast requests: 5-100ms (cache hits, health checks)
    /// - Normal requests: 100ms-1s (API calls)
    /// - Slow requests: 1-10s (LLM streaming, complex queries)
    const EXPONENTIAL_SECONDS: &[f64] = &[
        0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];

    PrometheusBuilder::new()
        .set_buckets_for_metric(
            Matcher::Full("http_request_duration_seconds".to_string()),
            EXPONENTIAL_SECONDS,
        )
        .expect("valid bucket configuration")
        .install_recorder()
        .expect("failed to install Prometheus recorder")
}

/// Serve Prometheus metrics
async fn serve_metrics(
    axum::extract::State(handle): axum::extract::State<PrometheusHandle>,
) -> impl IntoResponse {
    handle.render()
}

/// Middleware to track HTTP request metrics
async fn metrics_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let start = Instant::now();
    let method = request.method().as_str().to_string();
    let path = request.uri().path().to_string();

    let response = next.run(request).await;

    let duration = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    // Record metrics
    metrics::counter!("http_requests_total", "method" => method.clone(), "path" => path.clone(), "status" => status.clone()).increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method, "path" => path, "status" => status).record(duration);

    response
}

/// Middleware to add security headers to all responses
///
/// Adds the following headers:
/// - X-Content-Type-Options: nosniff (prevent MIME type sniffing)
/// - X-Frame-Options: DENY (prevent clickjacking)
/// - X-XSS-Protection: 1; mode=block (legacy XSS protection)
/// - Content-Security-Policy: default-src 'self' (restrict resource loading)
/// - Referrer-Policy: strict-origin-when-cross-origin
async fn security_headers_middleware(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();

    // Prevent MIME type sniffing
    headers.insert(
        axum::http::header::X_CONTENT_TYPE_OPTIONS,
        axum::http::HeaderValue::from_static("nosniff"),
    );

    // Prevent clickjacking
    headers.insert(
        axum::http::header::X_FRAME_OPTIONS,
        axum::http::HeaderValue::from_static("DENY"),
    );

    // Legacy XSS protection (for older browsers)
    headers.insert(
        axum::http::HeaderName::from_static("x-xss-protection"),
        axum::http::HeaderValue::from_static("1; mode=block"),
    );

    // Content Security Policy - restrict to same origin
    // Note: Relaxed for API-only service; tighten for web apps
    headers.insert(
        axum::http::header::CONTENT_SECURITY_POLICY,
        axum::http::HeaderValue::from_static("default-src 'self'"),
    );

    // Control referrer information
    headers.insert(
        axum::http::header::REFERRER_POLICY,
        axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"),
    );

    response
}

/// Configuration from environment variables
struct Config {
    /// Engine type: "mock", "http", or "process"
    engine_type: String,
    /// LangGraph server URL (for http engine)
    langgraph_url: String,
    /// Port to bind to
    port: u16,
    /// Bridge command (for process engine)
    bridge_command: String,
    /// Bridge script path (for process engine)
    bridge_script: String,
    /// Bridge working directory (for process engine)
    bridge_working_dir: Option<String>,
    /// Allowed CORS origins (comma-separated, or "*" for any)
    allowed_origins: String,
    /// API key for authentication (if empty, auth is disabled)
    api_key: Option<String>,
    /// Rate limit: requests per minute (0 = disabled)
    rate_limit_rpm: u64,
    /// Mock workflow file (for mock engine)
    mock_workflow_file: Option<PathBuf>,
}

impl Config {
    fn from_env() -> Self {
        Self {
            engine_type: std::env::var("ENGINE_TYPE").unwrap_or_else(|_| "mock".to_string()),
            langgraph_url: std::env::var("LANGGRAPH_URL")
                .unwrap_or_else(|_| "http://localhost:2024".to_string()),
            port: std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(8080),
            bridge_command: std::env::var("BRIDGE_COMMAND")
                .unwrap_or_else(|_| "python3".to_string()),
            bridge_script: std::env::var("BRIDGE_SCRIPT")
                .unwrap_or_else(|_| "bin/engine-bridge/main.py".to_string()),
            bridge_working_dir: std::env::var("BRIDGE_WORKING_DIR").ok(),
            allowed_origins: std::env::var("ALLOWED_ORIGINS")
                .unwrap_or_else(|_| "http://localhost:3000,http://localhost:8080".to_string()),
            api_key: std::env::var("API_KEY").ok().filter(|k| !k.is_empty()),
            rate_limit_rpm: std::env::var("RATE_LIMIT_RPM")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(100), // Default: 100 requests per minute
            mock_workflow_file: std::env::var("MOCK_WORKFLOW_FILE").ok().map(PathBuf::from),
        }
    }
}

/// Build CORS layer from configuration
fn build_cors_layer(config: &Config) -> CorsLayer {
    let origins = &config.allowed_origins;

    if origins == "*" {
        tracing::warn!("CORS configured to allow ANY origin - not recommended for production");
        CorsLayer::very_permissive()
    } else {
        let allowed: Vec<_> = origins
            .split(',')
            .filter_map(|s| {
                let trimmed = s.trim();
                match trimmed.parse() {
                    Ok(origin) => Some(origin),
                    Err(e) => {
                        tracing::warn!(origin = %trimmed, error = %e, "Invalid CORS origin, skipping");
                        None
                    }
                }
            })
            .collect();

        tracing::info!(origins = ?allowed, "CORS configured with allowed origins");

        CorsLayer::new()
            .allow_origin(AllowOrigin::list(allowed))
            .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
            .allow_headers([
                axum::http::header::CONTENT_TYPE,
                axum::http::header::AUTHORIZATION,
                axum::http::header::HeaderName::from_static("x-api-key"),
            ])
    }
}

fn create_engine(config: &Config) -> anyhow::Result<Arc<dyn AgenticEngine>> {
    match config.engine_type.as_str() {
        #[cfg(feature = "rig")]
        "rig" => {
            tracing::info!("Creating RigEngine");
            let engine_config = RigEngineConfig::from_env()?;
            let engine = RigEngine::new(engine_config)?;
            Ok(Arc::new(engine))
        }
        "http" => {
            tracing::info!(
                langgraph_url = %config.langgraph_url,
                "Creating HttpEngine"
            );
            let engine_config = HttpEngineConfig::new(&config.langgraph_url);
            let engine = HttpEngine::new(engine_config)?;
            Ok(Arc::new(engine))
        }
        "process" => {
            tracing::info!(
                command = %config.bridge_command,
                script = %config.bridge_script,
                "Creating ProcessEngine"
            );
            let mut engine_config = ProcessEngineConfig::new(&config.bridge_command)
                .with_arg(&config.bridge_script)
                .with_env("LANGGRAPH_URL", &config.langgraph_url);

            if let Some(ref dir) = config.bridge_working_dir {
                engine_config = engine_config.with_working_dir(dir);
            }

            let engine = ProcessEngine::new(engine_config);
            Ok(Arc::new(engine))
        }
        _ => {
            tracing::info!("Creating MockEngine (default)");

            let workflow = if let Some(ref path) = config.mock_workflow_file {
                tracing::info!(file = %path.display(), "Loading mock workflow from file");
                // read and parse json
                let data = std::fs::read_to_string(path)?;
                let workflow: Workflow = serde_json::from_str(&data)?;
                Some(workflow)
            } else {
                None
            };

            Ok(Arc::new(MockEngine::new(workflow)))
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load environment variables from .env file (if present)
    dotenvy::dotenv().ok();
    // Load secrets from .env.secrets file (if present)
    dotenvy::from_filename(".env.secrets").ok();

    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "infraware_backend=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Initialize Prometheus metrics
    let metrics_handle = setup_metrics();
    tracing::info!("Prometheus metrics initialized");

    // Load configuration
    let config = Config::from_env();

    // Create engine based on configuration
    let engine = create_engine(&config)?;

    // Create auth config
    let auth_config = auth_middleware::AuthConfig::new(config.api_key.clone());
    if auth_config.is_enabled() {
        tracing::info!("Authentication enabled");
    } else {
        tracing::warn!("Authentication disabled - set API_KEY env var to enable");
    }

    // Create app state with engine and auth config
    let state = AppState::new(engine, auth_config.clone());

    // Build CORS layer
    let cors = build_cors_layer(&config);

    // Build router
    // Metrics route (separate state)
    let metrics_routes = Router::new()
        .route("/metrics", get(serve_metrics))
        .with_state(metrics_handle);

    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(routes::health::health_check))
        .route("/api-docs/openapi.json", get(serve_openapi))
        .nest("/api", routes::api_routes());

    // Protected routes (auth required)
    let protected_routes = Router::new()
        .nest("/threads", routes::thread_routes())
        .layer(axum::middleware::from_fn_with_state(
            auth_config,
            auth_middleware_fn,
        ));

    // Build rate limiter if enabled
    let rate_limiter = if config.rate_limit_rpm > 0 {
        tracing::info!(rpm = config.rate_limit_rpm, "Rate limiting enabled");
        Some(Arc::new(RateLimiter::new(
            config.rate_limit_rpm,
            Duration::from_secs(60),
        )))
    } else {
        tracing::info!("Rate limiting disabled");
        None
    };

    // Request ID header name
    let x_request_id = axum::http::HeaderName::from_static("x-request-id");

    // Build app with layered middleware
    // Layers are applied in reverse order (bottom layer runs first)
    let app = Router::new()
        .merge(metrics_routes)
        .merge(public_routes)
        .merge(protected_routes)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        // Security headers middleware (X-Content-Type-Options, X-Frame-Options, etc.)
        .layer(axum::middleware::from_fn(security_headers_middleware))
        // Metrics middleware to track request counts and durations
        .layer(axum::middleware::from_fn(metrics_middleware))
        .layer(axum::middleware::from_fn(move |request, next| {
            let limiter = rate_limiter.clone();
            rate_limit_middleware(limiter, request, next)
        }))
        // Request ID: propagate to response
        .layer(PropagateRequestIdLayer::new(x_request_id.clone()))
        // Request ID: set on incoming request (uses UUID v4)
        .layer(SetRequestIdLayer::new(x_request_id, MakeRequestUuid))
        .with_state(state);

    // Start server with graceful shutdown
    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!(
        addr = %addr,
        engine = %config.engine_type,
        "Starting Infraware backend"
    );

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    tracing::info!("Server shutdown complete");
    Ok(())
}

/// Wait for shutdown signal (Ctrl+C or SIGTERM)
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {
            tracing::info!("Received Ctrl+C, starting graceful shutdown");
        },
        _ = terminate => {
            tracing::info!("Received SIGTERM, starting graceful shutdown");
        },
    }
}

/// Auth middleware function
async fn auth_middleware_fn(
    axum::extract::State(config): axum::extract::State<auth_middleware::AuthConfig>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    use axum::http::{StatusCode, header};

    // If auth is disabled, pass through
    if !config.is_enabled() {
        return Ok(next.run(request).await);
    }

    // Extract API key from headers
    let api_key = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.strip_prefix("Bearer "))
        .or_else(|| {
            request
                .headers()
                .get("x-api-key")
                .and_then(|v| v.to_str().ok())
        });

    match api_key {
        Some(key) if config.validate(key) => {
            tracing::debug!("Authentication successful");
            Ok(next.run(request).await)
        }
        Some(_) => {
            tracing::warn!("Authentication failed: invalid API key");
            Err(StatusCode::UNAUTHORIZED)
        }
        None => {
            tracing::warn!("Authentication failed: no API key provided");
            Err(StatusCode::UNAUTHORIZED)
        }
    }
}

/// Simple sliding window rate limiter
///
/// Uses atomic operations to avoid lock contention and TOCTOU races.
/// The window is stored as a timestamp in milliseconds since process start.
struct RateLimiter {
    /// Maximum requests allowed in the window
    max_requests: u64,
    /// Time window duration in milliseconds
    window_ms: u64,
    /// Current request count
    count: AtomicU64,
    /// Window start time in milliseconds since process start
    window_start_ms: AtomicU64,
    /// Process start instant for calculating relative timestamps
    process_start: Instant,
}

impl RateLimiter {
    fn new(max_requests: u64, window: Duration) -> Self {
        Self {
            max_requests,
            window_ms: window.as_millis() as u64,
            count: AtomicU64::new(0),
            window_start_ms: AtomicU64::new(0),
            process_start: Instant::now(),
        }
    }

    /// Check if request is allowed under rate limit
    ///
    /// Returns true if request is allowed, false if rate limit exceeded.
    /// Uses compare-and-swap for lock-free window reset.
    fn check(&self) -> bool {
        let now_ms = self.process_start.elapsed().as_millis() as u64;

        // Try to reset window if expired (lock-free)
        loop {
            let window_start = self.window_start_ms.load(Ordering::Acquire);
            if now_ms.saturating_sub(window_start) >= self.window_ms {
                // Window expired, try to reset atomically
                if self
                    .window_start_ms
                    .compare_exchange(window_start, now_ms, Ordering::AcqRel, Ordering::Relaxed)
                    .is_ok()
                {
                    // We won the race to reset the window
                    self.count.store(1, Ordering::Release);
                    return true;
                }
                // Another thread reset the window, retry
                continue;
            }
            break;
        }

        // Window still valid, increment count
        let count = self.count.fetch_add(1, Ordering::AcqRel);
        count < self.max_requests
    }
}

/// Rate limiting middleware
async fn rate_limit_middleware(
    limiter: Option<Arc<RateLimiter>>,
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    if let Some(limiter) = limiter
        && !limiter.check()
    {
        tracing::warn!("Rate limit exceeded");
        return Err(axum::http::StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(request).await)
}
