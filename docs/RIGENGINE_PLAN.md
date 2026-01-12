# RigEngine Implementation Plan

## Overview

Implement a native Rust agentic engine using **rig-rs** library as an alternative to the LangGraph-based HttpEngine. The RigEngine will implement the existing `AgenticEngine` trait and coexist with other engines via `ENGINE_TYPE=rig`.

### Scope Decisions
- **Provider**: Anthropic (Claude) only
- **Strategy**: Coexistence with HttpEngine
- **Tools**: shell_command + ask_question (HITL base)
- **Persistence**: In-memory (state lost on restart)

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        RigEngine                                 │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐  │
│  │  StateStore │  │ RigOrchest- │  │     SecurityConfig      │  │
│  │  (threads,  │  │   rator     │  │  (blocklist, patterns)  │  │
│  │  interrupts)│  │ (streaming) │  │                         │  │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘  │
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐│
│  │                        Tools                                 ││
│  │  ┌──────────────────┐   ┌──────────────────────┐            ││
│  │  │ ShellCommandTool │   │   AskQuestionTool    │            ││
│  │  │ (HITL: approve)  │   │   (HITL: answer)     │            ││
│  │  └──────────────────┘   └──────────────────────┘            ││
│  └─────────────────────────────────────────────────────────────┘│
├─────────────────────────────────────────────────────────────────┤
│  ┌─────────────────────────────────────────────────────────────┐│
│  │              rig-rs (Anthropic Provider)                    ││
│  │              Claude API via rig-core                         ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

---

## Module Structure

```
crates/backend-engine/src/adapters/rig/
├── mod.rs              # Module exports, RigEngine + RigEngineConfig re-exports
├── engine.rs           # RigEngine implementing AgenticEngine trait
├── config.rs           # RigEngineConfig (Anthropic settings, system prompt)
├── state.rs            # StateStore, ThreadState, PendingInterrupt
├── orchestrator.rs     # Stream conversion, HITL marker detection
├── security.rs         # Command validation, blocklist, dangerous patterns
└── tools/
    ├── mod.rs          # Tool exports, HitlMarker enum
    ├── shell_command.rs # ShellCommandTool (triggers CommandApproval)
    └── ask_question.rs  # AskQuestionTool (triggers Question)
```

---

## Implementation Phases

### Phase 1: Scaffolding & Config
**Goal**: Feature flag compiles, config works

**Files to create/modify**:
- `crates/backend-engine/Cargo.toml` - Add `rig` feature + `rig-core` dependency
- `crates/backend-engine/src/adapters/mod.rs` - Export RigEngine under feature
- `crates/backend-engine/src/adapters/rig/mod.rs` - Module entry
- `crates/backend-engine/src/adapters/rig/config.rs` - RigEngineConfig

**Config structure**:
```rust
pub struct RigEngineConfig {
    pub api_key: String,           // ANTHROPIC_API_KEY
    pub model: String,             // Default: claude-sonnet-4-20250514
    pub max_tokens: u32,           // Default: 4096
    pub system_prompt: String,     // DevOps assistant prompt
    pub timeout_secs: u64,         // Default: 300
    pub temperature: f32,          // Default: 0.7
}
```

**Environment variables**:
```bash
ENGINE_TYPE=rig
ANTHROPIC_API_KEY=sk-ant-...
ANTHROPIC_MODEL=claude-sonnet-4-20250514
RIG_SYSTEM_PROMPT="You are a DevOps assistant..."
RIG_TIMEOUT_SECS=300
```

---

### Phase 2: State Management
**Goal**: Thread lifecycle works

**Files to create**:
- `crates/backend-engine/src/adapters/rig/state.rs`

**Key types**:
```rust
pub struct StateStore {
    threads: RwLock<HashMap<String, ThreadState>>,
    thread_counter: AtomicU64,
}

pub struct ThreadState {
    pub id: ThreadId,
    pub messages: Vec<Message>,
    pub active_run: Option<RunState>,
}

pub struct RunState {
    pub run_id: String,
    pub pending_interrupt: Option<PendingInterrupt>,
}

pub struct PendingInterrupt {
    pub interrupt: Interrupt,
    pub resume_context: ResumeContext,
}
```

---

### Phase 3: Tools & Security
**Goal**: HITL tools with security validation

**Files to create**:
- `crates/backend-engine/src/adapters/rig/security.rs`
- `crates/backend-engine/src/adapters/rig/tools/mod.rs`
- `crates/backend-engine/src/adapters/rig/tools/shell_command.rs`
- `crates/backend-engine/src/adapters/rig/tools/ask_question.rs`

**Security blocklist** (examples):
- `rm -rf /`, `rm -rf /*`
- `mkfs`, `dd if=/dev/zero`
- Fork bomb: `:(){:|:&};:`
- Pipe to shell: `curl ... | bash`

**HitlMarker protocol**:
```rust
#[derive(Serialize, Deserialize)]
#[serde(tag = "hitl_type")]
pub enum HitlMarker {
    CommandApproval { command: String, message: String },
    Question { question: String, options: Option<Vec<String>> },
}
```

---

### Phase 4: Orchestrator & Streaming
**Goal**: Convert rig-rs to EventStream

**Files to create**:
- `crates/backend-engine/src/adapters/rig/orchestrator.rs`

**Flow**:
1. `stream_run()` → Create rig-rs agent with tools
2. Execute agent.chat() with conversation history
3. Detect HitlMarker in tool outputs
4. If HITL: emit `AgentEvent::Updates { interrupts }`, store PendingInterrupt
5. If complete: emit `AgentEvent::Values { messages }`, `AgentEvent::End`

**resume_run() flow**:
1. Retrieve PendingInterrupt from state
2. Construct continuation prompt based on ResumeResponse
3. Re-run agent with updated context

---

### Phase 5: Engine Implementation
**Goal**: Full AgenticEngine implementation

**Files to create/modify**:
- `crates/backend-engine/src/adapters/rig/engine.rs`
- `crates/backend-api/src/main.rs` - Add "rig" to create_engine()

**RigEngine methods**:
```rust
impl AgenticEngine for RigEngine {
    async fn create_thread(&self, metadata) -> Result<ThreadId, EngineError>;
    async fn stream_run(&self, thread_id, input) -> Result<EventStream, EngineError>;
    async fn resume_run(&self, thread_id, response) -> Result<EventStream, EngineError>;
    async fn health_check(&self) -> Result<HealthStatus, EngineError>;
}
```

---

### Phase 6: Integration & Testing
**Goal**: End-to-end working with terminal-app

**Tests to add**:
- Unit tests in each module
- Integration test: `tests/rig_integration.rs`
- Manual E2E test with terminal-app

---

## Critical Files Reference

| Purpose | File Path |
|---------|-----------|
| AgenticEngine trait | `crates/backend-engine/src/traits.rs` |
| Engine feature flags | `crates/backend-engine/Cargo.toml` |
| Adapter exports | `crates/backend-engine/src/adapters/mod.rs` |
| Engine factory | `crates/backend-api/src/main.rs:165-198` |
| Shared types | `crates/shared/src/events.rs`, `crates/shared/src/models.rs` |
| MockEngine (pattern) | `crates/backend-engine/src/adapters/mock.rs` |
| HttpEngine (pattern) | `crates/backend-engine/src/adapters/http.rs` |

---

## Dependencies

Add to `Cargo.toml` root workspace:
```toml
[workspace.dependencies]
rig-core = "0.6"  # Verify latest version
regex = "1"
```

Add to `crates/backend-engine/Cargo.toml`:
```toml
[dependencies]
rig-core = { workspace = true, optional = true }
regex = { workspace = true, optional = true }

[features]
rig = ["rig-core", "async-stream", "regex"]
```

---

## Verification Plan

### Unit Tests
```bash
cargo test -p infraware-engine --features rig
```

### Build Validation
```bash
cargo build -p infraware-engine --features rig
cargo build -p infraware-backend --features rig
cargo clippy --workspace --features rig
```

### Manual E2E Test
1. Set environment:
   ```bash
   export ENGINE_TYPE=rig
   export ANTHROPIC_API_KEY=sk-ant-...
   ```
2. Start backend:
   ```bash
   cargo run -p infraware-backend --features rig
   ```
3. Start terminal:
   ```bash
   cargo run -p infraware-terminal
   ```
4. Test queries:
   - `? list docker containers` → Should trigger CommandApproval
   - Approve with `y` → Should execute and show result
   - `? what's my current directory?` → Should trigger Question or Complete

### Health Check
```bash
curl http://localhost:8080/health
# Should return: {"status":"healthy","engine":"rig","provider":"anthropic/claude-sonnet-4-20250514"}
```

---

## Security Considerations

1. **Command Blocklist**: Prevent dangerous commands (rm -rf /, mkfs, etc.)
2. **Pattern Detection**: Warn on pipe-to-shell patterns
3. **Rate Limiting**: Already handled by backend-api middleware
4. **API Key Security**: Never log API keys, use env vars only
5. **Input Sanitization**: Validate tool arguments before processing

---

## Risks & Mitigations

| Risk | Mitigation |
|------|------------|
| rig-rs API changes | Pin specific version, monitor changelog |
| Anthropic rate limits | Implement exponential backoff |
| Tool output parsing | Robust HitlMarker detection with fallbacks |
| Memory growth (threads) | Add thread TTL/cleanup (future enhancement) |
