# Session Context Store Design

## Problem

Each new `stream_run()` call in the RigEngine intentionally skips conversation history to avoid LLM confusion.
This means the agent has no memory of facts discovered earlier in the same session (e.g., host OS, installed tools).
The agent repeatedly re-runs commands like `uname -a` on every query.

The existing `MemoryStore` is persistent and cross-session — wrong scope for ephemeral host/environment facts.

## Solution

Add a per-thread, in-memory `SessionContextStore` with a companion `SaveSessionContextTool`.
The LLM saves important session-scoped facts via the tool, and those facts are injected into the system prompt
on every subsequent query within the same thread.

## Data Model

### SessionContextCategory

```rust
enum SessionContextCategory {
    HostInfo,          // OS, kernel, hostname, architecture
    Environment,       // shell, installed tools, cloud CLI availability
    WorkingDirectory,  // project root, git branch, repo info
    ServiceState,      // running services, ports, process info
    Discovery,         // catch-all for anything else learned mid-session
}
```

### SessionContextEntry

```rust
struct SessionContextEntry {
    fact: String,
    category: SessionContextCategory,
    created_at: DateTime<Utc>,
}
```

### SessionContextStore

```rust
struct SessionContextStore {
    entries: VecDeque<SessionContextEntry>,
    limit: usize, // default: 50
}
```

Methods:

- `new(limit: usize) -> Self` — creates an empty store
- `add(fact: String, category: SessionContextCategory)` — adds entry with FIFO eviction at limit, no disk I/O. Empty facts after sanitization are silently ignored.
- `build_preamble() -> String` — formats all entries as a `## Current Session Context` section. Returns an empty string when the store has no entries (avoids polluting the prompt on the first query).

No `path`, no `flush()`, no `load_or_create()` — purely in-memory.

The entry limit uses a named constant: `const DEFAULT_SESSION_CONTEXT_LIMIT: usize = 50;`

## Tool Definition

### SaveSessionContextTool

```rust
struct SaveSessionContextTool {
    store: Option<Arc<RwLock<SessionContextStore>>>,  // tokio::sync::RwLock
}
```

Arguments:

- `fact: String` — clear statement of the discovered fact
- `category: SessionContextCategory`

Returns `SaveSessionContextResult { saved: bool, message: String }`.

Tool description instructs the LLM:

- **Use for**: host/environment info discovered via commands, service state, working directory details,
  any fact that avoids re-running a command later in this session
- **Do not use for**: user preferences/conventions (use `save_memory`), transient output not needed again,
  information already in the system prompt

The tool is NOT intercepted by `HitlHook` — it executes silently like `save_memory`.

## Preamble Injection Order

Session context has higher priority than persistent memory, so it is injected closer to the system prompt:

```
1. config.system_prompt           (base DevOps assistant prompt)
2. session_context.build_preamble() (current session facts — higher priority)
3. memory.build_preamble()          (persistent cross-session facts)
```

The `build_preamble()` output format:

```
## Current Session Context

The following facts were discovered during this session. Use them to avoid
re-running commands unnecessarily.

- [host_info] OS is Ubuntu 22.04 x86_64
- [environment] Shell is zsh
- [service_state] nginx running on port 80
```

## StateStore Integration

The `SessionContextStore` is owned per-thread inside `StateStore`:

```rust
// Inside the per-thread state (ThreadState or equivalent)
struct ThreadState {
    messages: Vec<Message>,
    active_run: Option<RunState>,
    session_context: Arc<RwLock<SessionContextStore>>, // NEW
}
```

- Created with `SessionContextStore::new(50)` when `create_thread()` is called
- Accessed via `StateStore::get_session_context(&self, thread_id) -> Option<Arc<RwLock<SessionContextStore>>>`

## Orchestrator Changes

### create_agent()

Gains a `session_context` parameter (read guard of `SessionContextStore`) and a `session_context_store`
parameter (`Arc<RwLock<SessionContextStore>>` for the tool). Injects session context preamble before memory preamble:

```rust
pub fn create_agent(
    client: &anthropic::Client,
    config: &RigAgentConfig,
    memory_store: &Arc<RwLock<MemoryStore>>,
    memory: &MemoryStore,
    session_context: &SessionContextStore,
    session_context_store: &Arc<RwLock<SessionContextStore>>,
) -> RigAgent {
    let session_preamble = session_context.build_preamble();
    let memory_preamble = memory.build_preamble();

    client
        .agent(&config.model)
        .preamble(&config.system_prompt)
        .append_preamble(&session_preamble)    // session-specific first
        .append_preamble(&memory_preamble)      // persistent second
        .max_tokens(config.max_tokens as u64)
        .temperature(f64::from(config.temperature))
        .tool(ShellCommandTool::new())
        .tool(AskUserTool::new())
        .tool(SaveMemoryTool::new(Arc::clone(memory_store)))
        .tool(SaveSessionContextTool::new(Arc::clone(session_context_store)))
        .tool(StartIncidentInvestigationTool)
        .build()
}
```

### create_run_stream() and create_resume_stream()

Look up the session context store from `StateStore` by `thread_id` via `get_session_context()`,
acquire a read lock when building the agent, and pass both the read guard and the `Arc` to `create_agent()`.

### run_agent_turn()

Gains a `session_context_store: &Arc<RwLock<SessionContextStore>>` parameter.
Acquires a read lock on the session context store when building the agent, same as it does for `memory_store`.

### handle_agent_continuation()

Gains a `session_context_store: Arc<RwLock<SessionContextStore>>` parameter, passed through from
`create_resume_stream()`. Forwards to `run_agent_turn()`.

## File Organization

### New file

`src/agent/adapters/rig/memory/session_context.rs`:

- `SessionContextCategory` enum + `Display` impl
- `SessionContextEntry` struct
- `SessionContextStore` struct + methods (`new`, `add`, `build_preamble`)
- `SaveSessionContextTool` + `SaveSessionContextArgs` + `SaveSessionContextResult` + error type
- `SESSION_CONTEXT_SYSTEM_PROMPT` constant
- `sanitize_fact()` (duplicated from `session.rs` — 5 lines, not worth shared utility)
- Unit tests

### Modified files

- `src/agent/adapters/rig/memory.rs` — add `pub mod session_context;` declaration
- `src/agent/adapters/rig/state.rs` — add `session_context: Arc<RwLock<SessionContextStore>>` field to `ThreadState`, initialize in `StateStore::create_thread()`, add `get_session_context()` method. Uses `tokio::sync::RwLock` (consistent with existing `StateStore` fields).
- `src/agent/adapters/rig/orchestrator.rs` — update `create_agent()` signature, update `run_agent_turn()` and `handle_agent_continuation()` signatures, update `create_run_stream()` and `create_resume_stream()` to look up and pass through session context
- `src/agent/adapters/rig/agent.rs` — no changes needed; `RigAgent::create_thread()` delegates to `StateStore::create_thread()` which handles initialization

## Design Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Scope | Per-thread | Different tabs may connect to different hosts |
| Storage | In-memory only | Session facts are ephemeral, no disk I/O needed |
| Population | Tool-only (LLM decides) | Simpler than hybrid auto-extraction, consistent with `SaveMemoryTool` pattern |
| Eviction | FIFO, 50 entries | Simple, sufficient for session scope |
| Limit configurability | Hardcoded | Internal optimization, not user-facing |
| Preamble order | Session context before memory | Session-specific facts have higher priority |
| HITL interception | Not intercepted | Silent background operation, same as `save_memory` |
