# Backend Architecture

> ## ⚠️ NOTA: Diagrammi Mermaid
>
> Questo documento contiene diagrammi in formato **Mermaid**.
>
> - **GitHub/GitLab**: I diagrammi vengono renderizzati automaticamente
> - **VS Code**: Installa l'estensione "Markdown Preview Mermaid Support"
> - **IntelliJ/RustRover**: Installa il plugin "Mermaid" dal Marketplace
> - **Obsidian**: Supporto nativo
>
> Senza plugin, vedrai il codice sorgente dei diagrammi invece dei grafici.

---

Questo documento descrive l'architettura del backend Rust di Infraware Terminal, le opzioni di integrazione disponibili
e i relativi pro/contro.

## Panoramica

Il backend è progettato con un'architettura modulare che permette di collegare diversi "motori agentici" (agentic
engines) senza modificare il codice dell'API.

```mermaid
flowchart TB
    subgraph Client["Terminal Client"]
        TA[terminal-app]
    end

    subgraph API["Backend API (axum)<br/>crates/infraware-backend"]
        E1["POST /threads"]
        E2["POST /threads/{id}/runs/stream"]
        E3["GET /health"]
        E4["GET /metrics"]
        E5["GET /api-docs/openapi.json"]
    end

    subgraph Engine["Engine Layer<br/>crates/infraware-engine"]
        AE{{"trait AgenticEngine"}}
        ME[MockEngine<br/>test]
        RE[RigEngine<br/>nativo Rust]
    end

    TA -->|HTTP/SSE| API
    API --> AE
    AE --> ME
    AE --> RE

    style AE fill:#f9f,stroke:#333,stroke-width:2px
```

## Il Trait AgenticEngine

Tutte le implementazioni condividono questa interfaccia:

```rust
#[async_trait]
pub trait AgenticEngine: Send + Sync + Debug {
    /// Crea un nuovo thread di conversazione
    async fn create_thread(&self, metadata: Option<Value>) -> Result<ThreadId, EngineError>;

    /// Avvia un run con streaming di eventi
    async fn stream_run(&self, thread_id: &ThreadId, input: RunInput) -> Result<EventStream, EngineError>;

    /// Riprende dopo un interrupt HITL (Human-in-the-Loop)
    async fn resume_run(&self, thread_id: &ThreadId, response: ResumeResponse) -> Result<EventStream, EngineError>;

    /// Verifica lo stato di salute dell'engine
    async fn health_check(&self) -> Result<HealthStatus, EngineError>;
}
```

## Tipi di Eventi

Gli engine producono un stream di `AgentEvent`:

```rust
pub enum AgentEvent {
    Metadata { run_id: String },           // Inizio run
    Message(MessageEvent),                  // Chunk di messaggio (streaming)
    Values { messages: Vec<Message> },      // Stato completo messaggi
    Updates { interrupts: Option<Vec<Interrupt>> },  // Interrupt HITL
    Error { message: String },              // Errore
    End,                                    // Fine stream
}

pub enum Interrupt {
    CommandApproval { command: String, message: String },  // Richiede approvazione comando
    Question { question: String, options: Option<Vec<String>> },  // Domanda all'utente
}
```

---

## Engine Disponibili

### RigEngine (Default - Nativo Rust)

```mermaid
flowchart TB
    subgraph Backend["Backend (Rust) - Single Process"]
        subgraph API["Backend API (axum)"]
            AX[Axum Server]
        end

        subgraph Engine["RigEngine"]
            RIG[rig-rs]
        end

        subgraph LLM["LLM Provider"]
            ANTHROPIC[Anthropic API]
            OPENAI[OpenAI API]
            OTHER[Altri...]
        end

        AX --> RIG
        RIG --> ANTHROPIC
        RIG --> OPENAI
        RIG --> OTHER
    end

    style Backend fill:#e8f5e9
```

**Come funziona:**

- Tutto in un unico processo Rust
- rig-rs gestisce la logica agentica con function calling nativo
- Strumenti registrati: ShellCommandTool, AskUserTool
- PromptHook intercetta le tool call per HITL (Human-in-the-Loop)
- needs_continuation flag distingue tra comandi con risposta diretta vs quelli che richiedono continuazione
- Chiamate dirette all'API del provider LLM (Anthropic Claude)

**Configurazione:**

```bash
ENGINE_TYPE=rig
ANTHROPIC_API_KEY=sk-...
```

**Pro:**

- Performance massima: nessun overhead di rete interna, nessun subprocess
- Type-safe: tutto compilato insieme
- Semplicità deployment: un solo binario
- Memoria condivisa: nessuna serializzazione tra componenti
- HITL integrato: PromptHook intercetta le tool call per approvazione utente
- needs_continuation flag: intelligente routing tra risposte dirette e agentive

**Contro:**

- Meno flessibile: devi ricompilare per cambiare logica
- Lock-in Rust: la logica agentica deve essere in Rust
- Maturità: rig-rs 0.28 è giovane, API potrebbe cambiare

**Quando usarlo:**

- Uso primario (default): migliore bilanciamento tra performance, semplicità e features
- Deployment su macchine singole o cluster Kubernetes
- Quando HITL e intelligenza di continuazione sono importanti

---

## needs_continuation Flag

The `needs_continuation` flag is a critical feature that controls how the RigEngine handles command output after user
approval.

### What It Means

When the agent proposes a shell command, it sets `needs_continuation` to indicate whether the output should be:

- **false** (default): Output IS the final answer to the user's query
  - Example: `ls -la` → list files → done
  - Example: `whoami` → show current user → done
  - The agent doesn't need to see the output; terminal shows it directly

- **true**: Output is INPUT for the agent to continue reasoning
  - Example: `uname -s` → get OS → then provide OS-specific instructions
  - Example: `node --version` → get version → then suggest upgrade if outdated
  - The agent receives the output and continues the conversation

### Why It Matters

This flag allows the agent to distinguish between:

1. **Query commands** (output is the answer): "list files", "show my username"
2. **Decision commands** (output guides next steps): "detect OS", "check Python version", "see if service is running"

### Implementation Details

**In Interrupt (crates/shared/src/events.rs):**

```rust
pub enum Interrupt {
    CommandApproval {
        command: String,
        message: String,
        needs_continuation: bool,  // <-- This field
    },
    // ...
}
```

**In Tool Args (crates/infraware-engine/src/adapters/rig/tools/shell.rs):**

```rust
pub struct ShellCommandArgs {
    pub command: String,
    pub explanation: String,
    pub needs_continuation: bool,  // <-- LLM sets this
}
```

**In Resume Flow (crates/infraware-engine/src/adapters/rig/orchestrator.rs):**

```rust
match resume_response {
    ResumeResponse::Approved => {
        let output = execute_command(&command).await;

        if needs_continuation {
            // Send output back to LLM for further processing
            agent.continuation(output).await
        } else {
            // Output IS the answer - return directly to user
            emit_final_response(output).await
        }
    }
}
```

## RigEngine Execution Flow with needs_continuation

The `needs_continuation` flag enables intelligent command handling in RigEngine:

### Sequence Diagram: RigEngine HITL with needs_continuation

```mermaid
sequenceDiagram
    participant User as Terminal User
    participant Terminal as Terminal App
    participant Backend as RigEngine Backend
    participant LLM as Anthropic Claude
    participant PTY as PTY Session

    User->>Terminal: ? list files in /tmp
    Terminal->>Backend: stream_run(query)

    Backend->>LLM: "list files in /tmp"
    activate LLM
    LLM-->>Backend: ToolCall("execute_shell_command", {command: "ls /tmp", needs_continuation: false})
    deactivate LLM

    Backend->>Terminal: AgentEvent::Updates(CommandApproval)
    Terminal->>Terminal: State: AwaitingApproval
    Terminal-->>User: Show: "Execute: ls /tmp?" [y/n]

    User->>Terminal: [press y]
    Terminal->>Terminal: State: ExecutingCommand
    Terminal->>PTY: Execute: ls /tmp

    activate PTY
    PTY-->>Terminal: Output (file list)
    deactivate PTY

    Terminal->>Backend: resume_run(Approved, CommandOutput)

    alt needs_continuation = false
        Backend->>Backend: Output IS the answer
        Backend-->>Terminal: AgentEvent::Values{messages: [user_query, final_response]}
        Backend-->>Terminal: AgentEvent::End
        Terminal->>Terminal: State: Normal
        Terminal-->>User: Display file list
    else needs_continuation = true
        Backend->>LLM: "Command output: ..., continue processing"
        activate LLM
        LLM-->>Backend: Tool call or Message with instructions
        deactivate LLM
        Backend-->>Terminal: Continue agent loop
        Terminal->>Terminal: State: WaitingLLM
    end
```

### Example Scenarios

**Scenario 1: Direct Answer (needs_continuation=false)**

```
User: "List all Python files"
  ↓
Agent: execute_shell_command("find . -name '*.py'", needs_continuation=false)
  ↓
[User approves execution in PTY]
  ↓
Output: "file1.py file2.py file3.py"
  ↓
Agent: [Output is the complete answer - return directly to user]
  ↓
User sees: The list of Python files
```

**Scenario 2: Processing Needed (needs_continuation=true)**

```
User: "Help me setup Node.js with the right version"
  ↓
Agent: execute_shell_command("node --version", needs_continuation=true)
  ↓
[User approves execution in PTY]
  ↓
Output: "v14.21.0"
  ↓
Agent receives version and continues:
  "I see you have Node v14. Let me help you upgrade to v18..."
  ↓
Agent: execute_shell_command("nvm install 18", needs_continuation=true)
  ↓
[User approves]
  ↓
Agent continues with next steps based on output...
```

**Scenario 3: Question Handling (No Command)**

```
User: "Help me install Redis"
  ↓
Agent: AskUserTool("Which Linux distribution?", options=["Ubuntu", "Debian", "CentOS"])
  ↓
Terminal: State: AwaitingAnswer
  ↓
User: [Select "Ubuntu"]
  ↓
Agent: [Receives answer and continues with Ubuntu-specific instructions]
```

---

### MockEngine (Test/Demo)

```mermaid
flowchart TB
    subgraph Backend["Backend (Rust) - Single Process"]
        subgraph API["Backend API (axum)"]
            AX[Axum Server]
        end

        subgraph Mock["MockEngine"]
            MM[In-Memory<br/>Pattern Matching]
        end

        AX --> MM
    end

    style Backend fill:#fff3e0
```

**Come funziona:**

- Risposte simulate in-memory
- Pattern matching su input (es. "docker" → risposta docker)
- Simula anche interrupt HITL

**Configurazione:**

```bash
ENGINE_TYPE=mock
```

**Pro:**

- Zero dipendenze esterne
- Velocissimo
- Deterministico
- Ottimo per CI/CD

**Contro:**

- Non è un vero agente
- Risposte statiche

**Quando usarlo:**

- Test unitari e integrazione
- Sviluppo UI senza backend
- Demo e prototipi

---

## Tabella Comparativa

| Aspetto                | RigEngine              | MockEngine |
|------------------------|------------------------|------------|
| **Status**             | Default (primary)      | Testing    |
| **Latenza**            | Minima                 | Minima     |
| **Complessità**        | Bassa                  | Minima     |
| **Debug**              | Facile                 | Facile     |
| **HITL**               | Native PromptHook      | Simulato   |
| **needs_continuation** | Sì                     | Sì         |
| **Deployment**         | 1 processo             | 1 processo |
| **Dipendenze**         | rig-rs 0.28+           | Nessuna    |

---

## Sicurezza e Middleware

Il backend include diversi layer di sicurezza e middleware per la produzione.

### Stack Middleware

```mermaid
flowchart TB
    REQ[Request] --> RID[Request ID<br/>x-request-id UUID]
    RID --> RL[Rate Limiter<br/>token bucket]
    RL --> TRACE[Tracing<br/>tower-http]
    TRACE --> METRICS[Metrics<br/>Prometheus]
    METRICS --> CORS[CORS Layer]
    CORS --> AUTH{Auth Check}
    AUTH -->|Protected| AUTHM[API Key Validation]
    AUTH -->|Public| ROUTE[Route Handler]
    AUTHM --> ROUTE
    ROUTE --> ENGINE[Engine]
```

### Autenticazione

- **Endpoint protetti**: `/threads/*` richiedono API key
- **Endpoint pubblici**: `/health`, `/metrics`, `/api-docs/*`

Header supportati:

```http
Authorization: Bearer <api-key>
X-Api-Key: <api-key>
```

### Rate Limiting

Implementazione token bucket con sliding window:

- **Configurabile** via `RATE_LIMIT_RPM` (requests per minute)
- **Default**: 100 req/min
- **Risposta**: `429 Too Many Requests` quando superato

### CORS

Configurazione flessibile:

- **Sviluppo**: `ALLOWED_ORIGINS=*` (permissivo)
- **Produzione**: Lista esplicita `ALLOWED_ORIGINS=https://app.example.com,https://admin.example.com`

---

## Observability

### OpenAPI Documentation

Specifica OpenAPI 3.0 generata automaticamente con **utoipa**:

```bash
# Recupera spec JSON
curl http://localhost:8080/api-docs/openapi.json
```

Annotazioni nei route handler:

```rust
#[utoipa::path(
    post,
    path = "/threads",
    tag = "threads",
    request_body = CreateThreadRequest,
    responses(
        (status = 200, body = CreateThreadResponse),
        (status = 401, description = "Unauthorized"),
    ),
    security(("api_key" = []))
)]
pub async fn create_thread(...) { ... }
```

### Request Tracing

Ogni richiesta riceve un UUID unico:

```
Request:  X-Request-Id: (generato se assente)
Response: X-Request-Id: 550e8400-e29b-41d4-a716-446655440000
```

Utile per:

- Correlare log tra client e server
- Debug di problemi specifici
- Audit trail

### Prometheus Metrics

Endpoint: `GET /metrics`

Metriche esposte:

```
# Counter: richieste totali
http_requests_total{method="POST",path="/threads",status="200"} 42

# Histogram: latenza richieste (secondi)
http_request_duration_seconds_bucket{method="GET",path="/health",status="200",le="0.01"} 100
http_request_duration_seconds_sum{...} 0.523
http_request_duration_seconds_count{...} 150
```

Bucket configurati per latenze tipiche:
`[0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0]` secondi

---

## Perché Più Crates?

Il workspace contiene crates backend separati. Questa separazione è intenzionale:

```mermaid
flowchart TB
    subgraph Workspace["Workspace Rust"]
        SHARED["shared<br/>Tipi API condivisi"]
        ENGINE["infraware-engine<br/>Logica engine"]
        API["infraware-backend<br/>Server HTTP"]
    end

    TERMINAL["terminal-app<br/>(frontend)"]

    TERMINAL -->|"usa tipi"| SHARED
    API --> ENGINE
    ENGINE --> SHARED
```

| Crate                  | Scopo                                                       | Dipendenze Principali                               |
|------------------------|-------------------------------------------------------------|-----------------------------------------------------|
| **shared**             | Tipi condivisi (`AgentEvent`, `Message`, `Interrupt`, ecc.) | Solo `serde`, `thiserror`                           |
| **infraware-engine**   | Trait `AgenticEngine` + implementazioni (Rig, Mock)         | `shared`, `tokio`, `rig-core`                       |
| **infraware-backend**  | Server HTTP, routing, middleware, observability              | `engine`, `axum`, `tower-http`, `utoipa`, `metrics` |

### Razionale della Separazione

1. **shared** - Deve essere importabile sia da `terminal-app` che dal backend, **senza portarsi dietro** axum, tokio, o
   altre dipendenze pesanti. Se fosse dentro `infraware-engine`, il frontend dipenderebbe da tutto il backend.

2. **infraware-engine** - Contiene la logica pura degli engine. Può essere usato:
  - Senza server HTTP (in test, CLI, o embedded)
  - Da altri crates che non hanno bisogno del layer HTTP

3. **infraware-backend** - Solo il layer HTTP (axum). Se domani vuoi cambiare framework (da axum ad actix-web), tocchi
   **solo questo crate**.

---

## Struttura Directory

```
crates/
├── infraware-backend/        # Server HTTP (axum)
│   └── src/
│       ├── main.rs           # Entry point + middleware stack
│       ├── auth_middleware.rs # API key authentication
│       ├── openapi.rs        # OpenAPI spec (utoipa)
│       ├── error.rs          # ApiError types
│       ├── state.rs          # AppState con engine
│       └── routes/
│           ├── mod.rs        # Route aggregation
│           ├── health.rs     # GET /health
│           ├── auth.rs       # POST /api/auth
│           └── threads.rs    # POST /threads, /threads/{id}/runs/stream
│
├── infraware-engine/         # Engine implementations
│   └── src/
│       ├── lib.rs
│       ├── traits.rs         # trait AgenticEngine
│       ├── types.rs          # HealthStatus, ResumeResponse
│       ├── error.rs          # EngineError
│       └── adapters/
│           ├── mock.rs       # MockEngine (test)
│           └── rig.rs        # RigEngine (default)
│
└── shared/                   # Tipi condivisi
    └── src/
        ├── lib.rs
        ├── events.rs         # AgentEvent, Interrupt, MessageEvent
        └── models.rs         # Message, ThreadId, RunInput, LLMQueryResult
```

---

## Configurazione Ambiente

```bash
# === Engine Selection ===
ENGINE_TYPE=rig|mock             # Default: rig

# === Server ===
PORT=8080

# === Security ===
# API key per autenticazione (vuoto = auth disabilitata)
API_KEY=your-secret-api-key

# CORS origins (comma-separated, o "*" per qualsiasi)
ALLOWED_ORIGINS=http://localhost:3000,http://localhost:8080

# Rate limiting (requests per minute, 0 = disabilitato)
RATE_LIMIT_RPM=100

# === RigEngine (Default) ===
ANTHROPIC_API_KEY=sk-ant-...  # Richiesta per ENGINE_TYPE=rig

# === Debug ===
RUST_LOG=debug
```

### Variabili per Ambiente

| Variabile         | Dev     | Staging         | Prod            |
|-------------------|---------|-----------------|-----------------|
| `API_KEY`         | (vuoto) | `dev-key`       | `prod-key-xxx`  |
| `ALLOWED_ORIGINS` | `*`     | lista specifica | lista specifica |
| `RATE_LIMIT_RPM`  | `0`     | `200`           | `100`           |
| `RUST_LOG`        | `debug` | `info`          | `warn`          |

---

## Aggiungere un Nuovo Engine

1. Crea il file in `crates/infraware-engine/src/adapters/myengine.rs`

2. Implementa il trait:

```rust
pub struct MyEngine { ... }

#[async_trait]
impl AgenticEngine for MyEngine {
    async fn create_thread(&self, metadata: Option<Value>) -> Result<ThreadId, EngineError> {
        // ...
    }

    async fn stream_run(&self, thread_id: &ThreadId, input: RunInput) -> Result<EventStream, EngineError> {
        // ...
    }

    async fn resume_run(&self, thread_id: &ThreadId, response: ResumeResponse) -> Result<EventStream, EngineError> {
        // ...
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        // ...
    }
}
```

3. Esporta in `adapters/mod.rs`:

```rust
#[cfg(feature = "myengine")]
mod myengine;
#[cfg(feature = "myengine")]
pub use myengine::MyEngine;
```

4. Aggiungi feature in `Cargo.toml`:

```toml
[features]
myengine = ["dep-if-needed"]
```

5. Aggiungi case in `infraware-backend/src/main.rs`:

```rust
"myengine" => {
    let engine = MyEngine::new(...);
    Ok(Arc::new(engine))
}
```

---

## Roadmap

### Completate

- [x] **Fase 1**: Scaffolding + MockEngine
- [x] **Fase 2**: RigEngine (native Rust agent)
- [x] **Fase 3**: Security Hardening
  - CORS configurabile
  - Autenticazione API key
  - Rate limiting (token bucket)
  - Input validation (ThreadId, messages)
  - Graceful shutdown (SIGTERM, Ctrl+C)
- [x] **Fase 5**: Observability
  - OpenAPI documentation (utoipa)
  - Request tracing (x-request-id)
  - Prometheus metrics

### In Corso / Pianificate

- [ ] **Fase 6**: State Persistence
  - SQLite per sviluppo
  - Redis/PostgreSQL per produzione
  - Persistenza interrupts e chat history
- [ ] **Fase 7**: Advanced Resilience
  - Retry con exponential backoff
  - Circuit breaker per Anthropic API
  - Health check avanzato (deep health check)
  - Graceful degradation per API timeouts
- [ ] **Fase 8**: Performance Optimization
  - Token streaming optimization
  - Connection pooling per Anthropic API
  - Batch interrupt processing
- [ ] **Fase 9**: Advanced Features
  - Tool result caching
  - Multi-model support (Claude 3, 4 variants)
  - Custom tool registration API
