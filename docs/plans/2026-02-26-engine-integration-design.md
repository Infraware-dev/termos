# Engine Integration: Collapse to Single Crate

**Date**: 2026-02-26
**Status**: Approved

## Goal

Eliminate the backend binary and HTTP layer. Embed the engine directly into the terminal process, then collapse the
entire workspace into a single crate.

## Decisions

| Decision | Choice |
|----------|--------|
| Command execution | Via terminal PTY (not engine-internal) |
| Authentication | Removed entirely (no network boundary) |
| Abstraction level | Single crate, engine as internal module |
| MockEngine | Kept for testing; terminal's MockLLMClient removed |
| Migration strategy | Phased (3 phases) |
| Shared crate | Absorbed into `src/engine/shared/` |

## Target Structure

```
infraware-terminal/
├── Cargo.toml                  # Single crate, no workspace
├── src/
│   ├── main.rs
│   ├── app.rs
│   ├── app/                    # Handler modules (unchanged)
│   │   ├── input_handler.rs
│   │   ├── hitl_handler.rs
│   │   ├── llm_controller.rs   # Rewritten: drives engine directly
│   │   ├── llm_event_handler.rs
│   │   ├── session_manager.rs
│   │   ├── tiles_manager.rs
│   │   ├── clipboard.rs
│   │   ├── render.rs
│   │   ├── terminal_renderer.rs
│   │   ├── behavior.rs
│   │   └── state.rs
│   ├── engine/                  # From crates/infraware-engine
│   │   ├── mod.rs
│   │   ├── traits.rs            # AgenticEngine, EventStream, EngineError
│   │   ├── types.rs             # ResumeResponse, HealthStatus
│   │   ├── adapters/
│   │   │   ├── mod.rs
│   │   │   ├── mock.rs
│   │   │   └── rig/             # RigEngine (structure preserved)
│   │   │       ├── mod.rs
│   │   │       ├── engine.rs
│   │   │       ├── orchestrator.rs
│   │   │       ├── config.rs
│   │   │       ├── state.rs
│   │   │       ├── memory/
│   │   │       └── tools/
│   │   └── shared/              # From crates/shared
│   │       ├── mod.rs
│   │       ├── models.rs
│   │       ├── events.rs
│   │       └── status.rs
│   ├── terminal/
│   ├── pty/
│   ├── llm/                     # Renderers only (client.rs removed)
│   │   ├── renderer.rs
│   │   └── incremental_renderer.rs
│   ├── input/
│   ├── orchestrators/
│   ├── ui/
│   ├── config.rs
│   └── state.rs
```

## Integration Flow

### LlmController (rewritten)

```
LlmController
  ├── engine: Arc<dyn AgenticEngine>
  ├── thread_id: Option<ThreadId>
  ├── renderer: IncrementalRenderer
  └── event_tx: mpsc::Sender<AppBackgroundEvent>
```

**start_query(text)**:
1. Ensure thread exists (`engine.create_thread()` if needed)
2. Build `RunInput` from user text
3. Spawn tokio task: `engine.stream_run()` → consume `EventStream` → convert `AgentEvent` to
   `AppBackgroundEvent` → send via channel

**resume_with_command_output(cmd, output)**:
1. Spawn tokio task: `engine.resume_run(thread_id, ResumeResponse::CommandOutput { command, output })`
   → consume `EventStream` → same conversion pipeline

### Command Execution (PTY-based)

1. Engine emits `Interrupt::CommandApproval { command, message, needs_continuation }`
2. Terminal enters `AppMode::AwaitingApproval`
3. User approves → terminal writes command to PTY
4. Terminal captures PTY output
5. Terminal calls `engine.resume_run(thread_id, ResumeResponse::CommandOutput { command, output })`
6. If `needs_continuation=true`: engine continues reasoning with the output
7. If `needs_continuation=false`: engine ends

The engine never executes commands itself. `ResumeResponse::Approved` handling is removed from the orchestrator.

## Removals

### Crates removed
- `crates/infraware-backend/` (axum server, routes, auth, rate limiting, metrics, OpenAPI, CORS)
- `crates/shared/` (absorbed into `src/engine/shared/`)
- `crates/infraware-engine/` (moved to `src/engine/`)
- `terminal-app/` (moved to `src/`)

### Terminal files removed
- `src/llm/client.rs` (HttpLLMClient, MockLLMClient, LLMClientTrait)
- `src/auth/` (all authentication logic)

### Dependencies removed
- `reqwest` (no HTTP calls)

### Engine code removed
- `execute_command()`, `handle_approved_command()`, `execute_with_sudo()` in orchestrator.rs
- `ResumeResponse::Approved` handling path
- Safety validation logic (shell handles this naturally)
- `LLMQueryResult` (legacy, unused)

### Engine code kept
- `AgenticEngine` trait + `EventStream` + `EngineError`
- `RigEngine` (agent creation, tool registration, HITL hook, stream creation)
- `MockEngine` (testing adapter)
- All tools (ShellCommandTool, AskUserTool, SaveMemoryTool, etc.)
- Memory system (MemoryStore, SaveMemoryTool)
- StateStore (thread/run state)

## Migration Phases

### Phase 1: Integrate engine into terminal (workspace still exists)

1. Add `infraware-engine` dependency to `terminal-app/Cargo.toml` with `rig` feature
2. Rewrite `llm_controller.rs`: `Arc<dyn AgenticEngine>` replaces `Arc<dyn LLMClientTrait>`
3. Convert `AgentEvent` stream to `AppBackgroundEvent` (replaces SSE parsing)
4. Remove `src/llm/client.rs`, `src/auth/`
5. Remove `reqwest` dependency
6. Update `state.rs` imports
7. Add engine env vars to config
8. Verify: `cargo build -p infraware-terminal && cargo test -p infraware-terminal`

### Phase 2: Flatten to single crate

1. Move `crates/infraware-engine/src/` → `src/engine/`
2. Move `crates/shared/src/` → `src/engine/shared/`
3. Merge dependencies into single root `Cargo.toml`
4. Move `terminal-app/src/` → root `src/`
5. Remove `[workspace]` from `Cargo.toml`
6. Update imports: `infraware_engine::` → `crate::engine::`, `infraware_shared::` → `crate::engine::shared::`
7. Verify: `cargo build && cargo test`

### Phase 3: Clean up

1. Delete `crates/infraware-backend/`, `crates/shared/`, `crates/infraware-engine/`, `terminal-app/`
2. Remove engine-internal command execution from orchestrator
3. Remove `ResumeResponse::Approved` path
4. Clean up `LLMQueryResult`
5. Update `CLAUDE.md` and docs
6. Final: `cargo build && cargo test && cargo clippy`

## Environment Variables (final)

```bash
# Engine selection
ENGINE_TYPE="rig"                    # rig (default) or mock

# Anthropic (required for rig engine)
ANTHROPIC_API_KEY="sk-..."
ANTHROPIC_MODEL="claude-sonnet-4-20250514"  # optional, has default

# Engine tuning
RIG_MAX_TOKENS="4096"               # optional
RIG_TEMPERATURE="0.7"               # optional
RIG_TIMEOUT_SECS="300"              # optional

# Memory
MEMORY_PATH="./.infraware/memory.json"
MEMORY_LIMIT="200"

# MockEngine
MOCK_WORKFLOW_FILE="path/to/workflow.json"  # optional

# Logging
LOG_LEVEL="debug"
```
