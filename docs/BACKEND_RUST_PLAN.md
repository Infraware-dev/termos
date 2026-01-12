# Piano: Backend Rust con Motore Agentico Disaccoppiato

## Obiettivo
Creare un backend Rust che sostituisca l'attuale FastAPI Python, mantenendo compatibilità con il terminale e permettendo di sostituire il motore agentico (LangGraph oggi, rig-rs domani).

## Architettura Attuale
```
Terminal (Rust) → Python FastAPI (proxy) → LangGraph Server (port 2024)
```

## Architettura Target
```
                              ┌─────────────────────────────────────────────┐
                              │            RUST BACKEND (axum)              │
┌──────────────┐              │  ┌────────────────────────────────────────┐ │
│   Terminal   │◄────────────►│  │  REST API Layer                        │ │
│   (egui)     │   HTTP/SSE   │  │  /api/auth, /threads, /runs/stream     │ │
└──────────────┘              │  └───────────────┬────────────────────────┘ │
                              │                  │                          │
                              │  ┌───────────────▼────────────────────────┐ │
                              │  │       AgenticEngine Trait              │ │
                              │  │  create_thread() / stream_run()        │ │
                              │  └───┬─────────────────┬─────────────┬────┘ │
                              │      │                 │             │      │
                              │  ┌───▼───┐      ┌──────▼─────┐  ┌────▼───┐ │
                              │  │ HTTP  │      │  Process   │  │  Mock  │ │
                              │  │Engine │      │  Engine    │  │ Engine │ │
                              │  │(proxy)│      │  (stdio)   │  │ (test) │ │
                              │  └───┬───┘      └──────┬─────┘  └────────┘ │
                              └──────┼─────────────────┼───────────────────┘
                                     │                 │
                     ┌───────────────▼───┐    ┌────────▼────────┐
                     │ LangGraph Server  │    │ rig-rs / Python │
                     │ (port 2024)       │    │ subprocess      │
                     └───────────────────┘    └─────────────────┘
```

## Requisiti
- **Linguaggio**: Rust
- **Engine Deployment**: Sidecar/subprocess
- **Protocollo**: Compatibile con terminale esistente (no modifiche al client)
- **Framework**: axum (SSE nativo, ecosystem Tower)

## Location
`/home/crist/infraware-terminal/backend-rs`

## Struttura Crates

```
backend-rs/
├── Cargo.toml                 # Workspace
├── crates/
│   ├── api/                   # REST API (axum)
│   │   └── src/
│   │       ├── main.rs
│   │       ├── config.rs
│   │       ├── error.rs
│   │       ├── routes/
│   │       │   ├── auth.rs    # POST /api/auth
│   │       │   ├── threads.rs # POST /threads
│   │       │   └── runs.rs    # POST /threads/{id}/runs/stream (SSE)
│   │       └── middleware/
│   │           └── auth.rs    # X-Api-Key validation
│   │
│   ├── engine/                # Engine abstraction
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs      # AgenticEngine trait
│   │       ├── types.rs       # AgentEvent, Interrupt, etc.
│   │       └── adapters/
│   │           ├── http.rs    # HttpEngine (proxy a LangGraph)
│   │           ├── process.rs # ProcessEngine (subprocess stdio)
│   │           └── mock.rs    # MockEngine (test)
│   │
│   └── state/                 # State management
│       └── src/
│           ├── lib.rs
│           └── thread.rs      # Thread state + persistence
```

## Trait Principale

```rust
#[async_trait]
pub trait AgenticEngine: Send + Sync + Debug {
    /// Crea nuovo thread di conversazione
    async fn create_thread(&self, metadata: Option<Value>) -> Result<ThreadId>;

    /// Avvia run con streaming
    async fn stream_run(&self, thread_id: &ThreadId, input: RunInput) -> Result<EventStream>;

    /// Riprende run dopo interrupt HITL
    async fn resume_run(&self, thread_id: &ThreadId, response: ResumeResponse) -> Result<EventStream>;

    /// Health check
    async fn health_check(&self) -> Result<HealthStatus>;
}

pub type EventStream = Pin<Box<dyn Stream<Item = Result<AgentEvent>> + Send>>;
```

## AgentEvent (compatibile con protocollo esistente)

```rust
pub enum AgentEvent {
    Metadata { run_id: String },
    Message(MessageEvent),
    Values { messages: Vec<Message> },
    Updates { interrupts: Option<Vec<Interrupt>> },  // __interrupt__
    Error { message: String },
    End,
}

pub enum Interrupt {
    CommandApproval { command: String, message: String },
    Question { question: String, options: Option<Vec<String>> },
}
```

## Fasi di Implementazione

### Fase 1: Scaffolding + MockEngine
**File da creare:**
- `backend-rs/Cargo.toml` (workspace)
- `crates/api/Cargo.toml` + `src/main.rs`
- `crates/engine/Cargo.toml` + `src/lib.rs`, `traits.rs`, `types.rs`
- `crates/engine/src/adapters/mock.rs`
- `crates/api/src/routes/auth.rs`, `threads.rs`, `runs.rs`

**Output**: Server avviabile con MockEngine, test di compatibilità con terminale

### Fase 2: HttpEngine (proxy a LangGraph)
**File da creare:**
- `crates/engine/src/adapters/http.rs`

**Output**: Funzionalità identica all'attuale FastAPI (reverse proxy)

### Fase 3: ProcessEngine (subprocess)
**File da creare:**
- `crates/engine/src/adapters/process.rs`
- `crates/engine/src/ipc/protocol.rs` (JSON-RPC)
- `crates/engine/src/ipc/stdio.rs`
- `bin/engine-bridge/main.py` (adapter Python per LangGraph)

**Output**: Engine via subprocess, pronto per rig-rs

### Fase 4: State Persistence
**File da creare:**
- `crates/state/src/thread.rs`
- `crates/state/src/persistence.rs`

**Output**: Thread state sopravvive a restart engine

### Fase 5: Hardening
- Error handling robusto
- Health checks + monitoring
- Logging (tracing crate)
- Docker setup

## Protocollo IPC (ProcessEngine)

JSON-RPC over stdio (line-delimited):

```json
// Request (Rust → Engine)
{"jsonrpc":"2.0","id":"uuid","method":"stream_run","params":{...}}

// Response event (Engine → Rust)
{"jsonrpc":"2.0","id":"uuid","event":{"type":"message",...}}

// End response
{"jsonrpc":"2.0","id":"uuid","result":{"status":"completed"}}
```

## File di Riferimento (esistenti, non modificare)

| File | Scopo |
|------|-------|
| `terminal-app/src/llm/client.rs` | Contratto REST/SSE da rispettare |
| `backend/src/api/routes/langgraph_routes.py` | Logica proxy attuale |
| `backend/src/agents/supervisor/agent.py` | Supervisor LangGraph |

## Dipendenze Rust Raccomandate

```toml
[dependencies]
axum = { version = "0.7", features = ["macros"] }
tokio = { version = "1", features = ["full"] }
tower = "0.4"
tower-http = { version = "0.5", features = ["cors", "trace"] }
reqwest = { version = "0.12", features = ["stream", "json"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
async-trait = "0.1"
futures = "0.3"
tracing = "0.1"
tracing-subscriber = "0.3"
anyhow = "1"
thiserror = "1"
uuid = { version = "1", features = ["v4"] }
```

## Test di Compatibilità

Il terminale esistente (`src/llm/client.rs`) non deve essere modificato. Verificare:
1. `POST /api/auth` → `{ "success": bool, "message": string }`
2. `POST /threads` → `{ "thread_id": string }`
3. `POST /threads/{id}/runs/stream` → SSE con eventi corretti
4. Interrupt HITL con `__interrupt__` nel formato atteso

## Note

- Il backend Python esistente rimane operativo durante lo sviluppo
- Possiamo testare in parallelo sulla stessa macchina (porta diversa)
- La migrazione è graduale: prima HttpEngine, poi ProcessEngine
