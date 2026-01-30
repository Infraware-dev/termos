# Piano Code Review - Backend Rust

> Documento generato dalla code review dei crates backend.
> Data: 2026-01-11

## Riepilogo Problemi

| Priorità | Categoria | Problemi |
|----------|-----------|----------|
| **Critica** | Sicurezza | 3 |
| **Alta** | Robustezza | 3 |
| **Media** | Miglioramenti | 6 |
| **Bassa** | Suggerimenti | 5 |

---

## Fase 1: Fix Critici di Sicurezza

### 1.1 CORS Restrittivo
**File**: `crates/backend-api/src/main.rs:118`

**Problema**:
```rust
.layer(CorsLayer::new().allow_origin(Any).allow_methods(Any))
```
Permette richieste da qualsiasi origine.

**Fix**:
```rust
use tower_http::cors::AllowOrigin;

let cors = CorsLayer::new()
    .allow_origin(AllowOrigin::list([
        "http://localhost:3000".parse().unwrap(),
        // Aggiungere origini permesse
    ]))
    .allow_methods([Method::GET, Method::POST])
    .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);
```

**Configurazione**: Leggere origini da env var `ALLOWED_ORIGINS`.

---

### 1.2 Autenticazione Reale
**File**: `crates/backend-api/src/routes/auth.rs:38`

**Problema**:
```rust
let success = !api_key.is_empty();  // Accetta QUALSIASI key!
```

**Fix**:
1. Validare API key contro lista/database
2. Aggiungere middleware auth su routes protette
3. Usare header `Authorization: Bearer <token>`

```rust
// Middleware da aggiungere
async fn require_auth(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_header = request.headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok());

    match auth_header {
        Some(token) if state.validate_token(token) => Ok(next.run(request).await),
        _ => Err(StatusCode::UNAUTHORIZED),
    }
}
```

---

### 1.3 Input Validation
**File**: `crates/backend-api/src/routes/threads.rs:73-124`

**Problema**: Nessuna validazione su input utente.

**Fix**:
```rust
const MAX_MESSAGE_LENGTH: usize = 100_000;  // 100KB
const MAX_MESSAGES: usize = 100;

fn validate_run_input(input: &RunRequestBody) -> Result<(), AppError> {
    if input.messages.len() > MAX_MESSAGES {
        return Err(AppError::BadRequest("Too many messages".into()));
    }

    for msg in &input.messages {
        if msg.content.len() > MAX_MESSAGE_LENGTH {
            return Err(AppError::BadRequest("Message too long".into()));
        }
    }

    Ok(())
}
```

---

## Fase 2: Fix Robustezza

### 2.1 Race Condition MockEngine
**File**: `crates/backend-engine/src/adapters/mock.rs:130-136`

**Problema**: Lock rilasciato e riacquisito, altro thread può interferire.

**Fix**:
```rust
// Mantenere il lock per tutta l'operazione
let mut threads = self.threads.write().await;
if let Some(msgs) = threads.get_mut(&thread_id.0) {
    msgs.push(assistant_msg.clone());
}
// Lock rilasciato qui automaticamente
```

---

### 2.2 Timeout ProcessEngine
**File**: `crates/backend-engine/src/adapters/process.rs`

**Problema**: Solo `health_check` ha timeout, altre operazioni possono bloccarsi.

**Fix**:
```rust
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

pub async fn stream_run(...) -> Result<EventStream, EngineError> {
    tokio::time::timeout(DEFAULT_TIMEOUT, async {
        // operazione
    })
    .await
    .map_err(|_| EngineError::Timeout("stream_run timed out".into()))?
}
```

Aggiungere `Timeout` variant a `EngineError`.

---

### 2.3 Health Check Status Code
**File**: `crates/backend-api/src/routes/health.rs`

**Problema**: Restituisce sempre 200, anche se unhealthy.

**Fix**:
```rust
pub async fn health_check(State(state): State<AppState>) -> impl IntoResponse {
    let status = state.engine.health_check().await;

    match status {
        Ok(health) if health.healthy => (StatusCode::OK, Json(health)),
        Ok(health) => (StatusCode::SERVICE_UNAVAILABLE, Json(health)),
        Err(e) => (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(HealthStatus::unhealthy(e.to_string())),
        ),
    }
}
```

---

## Fase 3: Miglioramenti Consigliati

### 3.1 Unificare MessageEvent.role con MessageRole
**File**: `crates/shared/src/events.rs:53-59`

```rust
// Prima
pub struct MessageEvent {
    pub role: String,  // "assistant", "user", etc.
    pub content: String,
}

// Dopo
pub struct MessageEvent {
    pub role: MessageRole,  // Usa l'enum esistente
    pub content: String,
}
```

---

### 3.2 Validazione ThreadId
**File**: `crates/shared/src/models.rs:6-35`

```rust
impl ThreadId {
    pub fn new(id: impl Into<String>) -> Result<Self, ValidationError> {
        let id = id.into();
        if id.is_empty() {
            return Err(ValidationError::EmptyThreadId);
        }
        if id.len() > 256 {
            return Err(ValidationError::ThreadIdTooLong);
        }
        Ok(Self(id))
    }

    /// Crea senza validazione (per deserializzazione)
    pub fn new_unchecked(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}
```

---

### 3.3 Rimuovere Dead Code
**File**: `crates/backend-engine/src/adapters/http.rs:56-57`

```rust
// Rimuovere campo non usato
pub struct HttpEngine {
    config: HttpEngineConfig,
    client: Client,
    // last_thread: Arc<RwLock<Option<String>>>,  // RIMUOVERE
}
```

---

### 3.4 Configurare assistant_id
**File**: `crates/backend-engine/src/adapters/http.rs:392`

```rust
pub struct HttpEngineConfig {
    pub base_url: String,
    pub timeout_secs: u64,
    pub assistant_id: String,  // NUOVO
}

impl Default for HttpEngineConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:2024".to_string(),
            timeout_secs: 300,
            assistant_id: "supervisor".to_string(),
        }
    }
}
```

---

### 3.5 Rate Limiting
**File**: `crates/backend-api/src/main.rs`

```rust
use tower::limit::RateLimitLayer;
use std::time::Duration;

let app = Router::new()
    // ... routes ...
    .layer(RateLimitLayer::new(100, Duration::from_secs(60)))  // 100 req/min
```

Oppure usare `tower_governor` per rate limiting più sofisticato.

---

### 3.6 Graceful Shutdown
**File**: `crates/backend-api/src/main.rs`

```rust
use tokio::signal;

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}

// In main:
axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;
```

---

## Fase 4: Suggerimenti Opzionali

| Suggerimento | Descrizione | Effort |
|--------------|-------------|--------|
| OpenAPI docs | Generare spec con `utoipa` | Medio |
| Request tracing | Correlation ID per debug | Basso |
| Metriche Prometheus | `/metrics` endpoint | Medio |
| Doc examples | Esempi per tipi pubblici | Basso |
| Crate state | Implementare o rimuovere | Variabile |

---

## Checklist Implementazione

### Fase 1 - Critici ✅
- [x] 1.1 CORS restrittivo
- [x] 1.2 Autenticazione reale
- [x] 1.3 Input validation

### Fase 2 - Robustezza ✅
- [x] 2.1 Fix race condition MockEngine
- [x] 2.2 Timeout ProcessEngine
- [x] 2.3 Health check status code

### Fase 3 - Miglioramenti ✅
- [x] 3.1 Unificare MessageEvent.role
- [x] 3.2 Validazione ThreadId
- [x] 3.3 Rimuovere dead code (last_thread)
- [x] 3.4 Configurare assistant_id
- [x] 3.5 Rate limiting
- [x] 3.6 Graceful shutdown

### Fase 4 - Opzionali ✅
- [x] OpenAPI documentation (utoipa)
- [x] Request tracing (x-request-id)
- [x] Metriche Prometheus (/metrics)
- [x] Doc examples (doctests)

---

## Stima Effort

| Fase | Tempo Stimato | Priorità |
|------|---------------|----------|
| Fase 1 | 2-4 ore | **Obbligatorio per produzione** |
| Fase 2 | 1-2 ore | Alta |
| Fase 3 | 2-3 ore | Media |
| Fase 4 | 4-8 ore | Bassa |

**Totale**: 9-17 ore per implementazione completa
