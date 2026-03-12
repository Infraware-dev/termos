# Memory System

The Infraware terminal agent uses two complementary memory systems to maintain context across interactions. **Persistent memory** stores long-lived user facts across sessions (preferences, workflows, restrictions). **Session context** caches ephemeral facts discovered during the current terminal session (host info, service state, environment details). Both systems inject their contents into the agent's system prompt and expose LLM-callable tools for saving new facts.

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                         RigAgent                                    │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐   │
│  │                    System Prompt                              │   │
│  │  ┌──────────────────┐  ┌──────────────────────────────────┐  │   │
│  │  │ Session Context  │  │ Persistent Memory Preamble       │  │   │
│  │  │ Preamble         │  │ (all stored facts injected here) │  │   │
│  │  └──────────────────┘  └──────────────────────────────────┘  │   │
│  └──────────────────────────────────────────────────────────────┘   │
│                                                                     │
│  ┌──────────────────────┐  ┌──────────────────────────────────┐    │
│  │  save_session_context│  │  save_memory                     │    │
│  │  (LLM tool)          │  │  (LLM tool)                      │    │
│  └──────────┬───────────┘  └──────────────┬───────────────────┘    │
│             │                              │                        │
│  ┌──────────▼───────────┐  ┌──────────────▼───────────────────┐    │
│  │ SessionContextStore  │  │ MemoryStore                      │    │
│  │ Arc<RwLock<>>        │  │ Arc<RwLock<>>                    │    │
│  │                      │  │                                  │    │
│  │ Per-thread           │  │ Global (shared across threads)   │    │
│  │ In-memory only       │  │ Persisted to JSON file           │    │
│  │ Limit: 50 entries    │  │ Limit: 200 entries (default)     │    │
│  └──────────────────────┘  └──────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────┘
```

## Persistent Memory

Persistent memory stores facts about the user that survive across terminal sessions. These are loaded from disk on startup and written back after every addition.

### Storage

- **Format:** JSON array of `MemoryEntry` objects
- **Default path:** `./.infraware/memory.json` (configurable via `MEMORY_PATH`)
- **Default limit:** 200 entries (configurable via `MEMORY_LIMIT`)
- **Eviction:** FIFO — when the limit is reached, the oldest entry is removed before adding a new one
- **Persistence:** Entries are flushed to disk immediately after each `add()` call

### Categories

| Category | Description | Example |
|----------|-------------|---------|
| `preference` | User preference | "Prefers tabs over spaces" |
| `personal_fact` | Personal information | "My name is Alice" |
| `workflow` | Workflow convention | "Always run clippy before commit" |
| `restriction` | Explicit restriction | "Never push to main" |
| `convention` | Team/project convention | "We use conventional commits" |

### Entry Format

Each entry contains:

```json
{
  "fact": "User prefers dark themes",
  "category": "preference",
  "created_at": "2026-03-12T10:30:00Z"
}
```

### When the Agent Saves Memory

The agent uses `save_memory` when:

- The user explicitly says "remember X" or "always do X"
- The user states a personal fact or team convention
- The user establishes a workflow preference
- The user sets a restriction

The agent does **not** save:

- Workspace-specific file paths or project structure
- Transient conversation context ("I'll be back in 5 min")
- Summaries of code changes, bug fixes, or task progress
- Information only relevant to the current session

### Fact Sanitization

Before storage, facts are sanitized: consecutive whitespace/newlines are collapsed to single spaces, leading/trailing whitespace is trimmed, and leading dashes are stripped to prevent markdown injection.

### Files

| File | Contents |
|------|----------|
| `src/engine/adapters/rig/memory/persistent.rs` | `MemoryStore`, `MemoryEntry`, `MemoryCategory`, `SaveMemoryTool` |

## Session Context

Session context caches facts the agent discovers during the current terminal session — things like the OS version, running services, or environment variables. This avoids re-running diagnostic commands when the agent already knows the answer.

### Storage

- **Scope:** Per-thread, in-memory only (lost when session ends)
- **Default limit:** 50 entries
- **Eviction:** FIFO, same as persistent memory
- **Persistence:** None — intentionally ephemeral

### Categories

| Category | Description | Example |
|----------|-------------|---------|
| `host_info` | OS, kernel, architecture | "Running Ubuntu 22.04, kernel 5.15, x86_64" |
| `environment` | Shell, locale, PATH entries | "Shell is zsh, locale is en_US.UTF-8" |
| `working_directory` | Current or observed directory | "Working directory is /home/user/project" |
| `service_state` | Running services | "nginx is active (running)" |
| `discovery` | Exploratory findings | "Project uses a Makefile with 'build' and 'test' targets" |

### When the Agent Saves Session Context

The agent uses `save_session_context` when:

- A command reveals host information (OS, kernel, architecture)
- Environment details are discovered (shell, locale, PATH)
- A service state is checked
- Filesystem or project structure is explored

The agent does **not** save:

- Long-lived user preferences (those go to persistent memory via `save_memory`)
- Transient command output that changes frequently
- Information already present in session context

### Files

| File | Contents |
|------|----------|
| `src/engine/adapters/rig/memory/session_context.rs` | `SessionContextStore`, `SessionContextEntry`, `SessionContextCategory`, `SaveSessionContextTool` |

## How Memory Is Wired Into the Agent

Both memory systems are injected into the agent at creation time via `create_agent()` in the orchestrator:

1. **Preamble injection:** `session_context.build_preamble()` and `memory.build_preamble()` are appended to the system prompt. The agent sees all stored facts before responding.

2. **Tool registration:** `SaveMemoryTool` and `SaveSessionContextTool` are registered alongside the other agent tools (shell, ask_user, etc.). The LLM can call them at any point during a conversation turn.

3. **Thread isolation:** Each thread gets its own `SessionContextStore` (created in `StateStore::create_thread()`), while the `MemoryStore` is shared globally across all threads.

### Agent Creation Flow

```
create_agent()
  ├── session_context.build_preamble()  →  appended to system prompt
  ├── memory.build_preamble()           →  appended to system prompt
  ├── .tool(SaveMemoryTool)             →  global MemoryStore (Arc<RwLock>)
  └── .tool(SaveSessionContextTool)     →  per-thread SessionContextStore (Arc<RwLock>)
```

### Relevant Files

| File | Role |
|------|------|
| `src/engine/adapters/rig/orchestrator.rs` | `create_agent()` — preamble injection, tool registration |
| `src/engine/adapters/rig/agent.rs` | `RigAgent` — MemoryStore initialization on startup |
| `src/engine/adapters/rig/state.rs` | `StateStore`, `ThreadState` — per-thread SessionContextStore |
| `src/engine/adapters/rig/config.rs` | `RigAgentConfig`, `MemoryConfig` — env var loading |

## Configuration

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `MEMORY_PATH` | `./.infraware/memory.json` | File path for persistent memory storage |
| `MEMORY_LIMIT` | `200` | Maximum persistent memory entries (FIFO eviction) |

Session context limit is a compile-time constant (`DEFAULT_SESSION_CONTEXT_LIMIT = 50`) and is not configurable via environment variable.

### Example

```bash
# Custom memory location with higher limit
MEMORY_PATH="$HOME/.infraware/memory.json" MEMORY_LIMIT=500 cargo run
```
