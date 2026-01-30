# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware** is a Rust monorepo containing an AI-powered terminal emulator and its backend services. The terminal combines VTE-based terminal emulation with an integrated LLM agent for DevOps assistance.

**Key feature**: Prefix any command with `?` for natural language queries (e.g., `? how do I revert a git commit`)

**Tech Stack**: Rust 2024 edition, workspace with 4 members, egui/eframe, axum, tokio

**Prerequisites** (Linux):
```bash
sudo apt install -y pkg-config libssl-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Commands

```bash
# Build & Run
cargo build --workspace              # Build all crates
cargo run -p infraware-terminal      # Run terminal app
cargo run -p infraware-backend       # Run backend server (port 8080)
LOG_LEVEL=debug cargo run -p infraware-terminal  # With debug logging

# Backend with different engines
ENGINE_TYPE=mock cargo run -p infraware-backend    # MockEngine (default, for testing)
ENGINE_TYPE=http LANGGRAPH_URL=http://localhost:2024 cargo run -p infraware-backend  # HttpEngine
ENGINE_TYPE=process BRIDGE_SCRIPT=bin/engine-bridge/main.py cargo run -p infraware-backend  # ProcessEngine
ENGINE_TYPE=rig ANTHROPIC_API_KEY=sk-... cargo run -p infraware-backend --features rig  # RigEngine

# Testing
cargo test --workspace               # All tests
cargo test -p infraware-terminal -- test_name    # Single test by name
cargo test -p infraware-engine       # Test engine crate only
cargo test -- --nocapture            # With output

# Watch mode (auto-rebuild)
cargo watch -x 'run -p infraware-backend'    # Requires: cargo install cargo-watch

# Linting (CI enforces both)
cargo fmt --all && cargo clippy --workspace
cargo clippy --workspace -- -D warnings    # CI-strict mode (warnings = errors)

# Coverage (CI enforces 60% minimum, excluding UI/PTY/VTE modules)
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
cargo llvm-cov --all-features --workspace --summary-only  # Quick summary

# Quick API verification
curl http://localhost:8080/health
curl -X POST http://localhost:8080/threads -H "Content-Type: application/json" -d '{}'
```

## Workspace Structure

```
├── terminal-app/              # egui terminal client (infraware-terminal)
├── crates/
│   ├── shared/                # API contract types (infraware-shared)
│   ├── backend-api/           # axum REST/SSE server (infraware-backend)
│   └── backend-engine/        # AgenticEngine trait + adapters
├── bin/engine-bridge/         # Python bridge for ProcessEngine
├── backend/                   # Python FastAPI (legacy, being replaced)
└── docs/plans/                # Design documents and technical debt analysis
```

## Architecture

```
┌─────────────────┐     HTTP/SSE      ┌──────────────────────────────────────────┐
│ infraware-      │◄────────────────► │ infraware-backend (axum)                 │
│ terminal (egui) │                   │ Port 8080                                │
└────────┬────────┘                   │                                          │
         │                            │  ┌──────────────────────────────────────┐│
    ┌────▼────┐                       │  │ AgenticEngine trait                  ││
    │   PTY   │                       │  │ ┌────────┐ ┌────────┐ ┌────────┐    ││
    │ Session │                       │  │ │ Mock   │ │ HTTP   │ │Process │    ││
    └────┬────┘                       │  │ │ Engine │ │ Engine │ │Engine  │    ││
         │                            │  │ └────────┘ └────┬───┘ └────┬───┘    ││
    ┌────▼────┐                       │  │ ┌────────────────────────┐           ││
    │  VTE    │                       │  │ │ RigEngine (Primary)    │           ││
    │ Parser  │                       │  │ │ - Anthropic Claude API │           ││
    └────┬────┘                       │  │ │ - HITL tool execution  │           ││
         │                            │  │ │ - needs_continuation   │           ││
    ┌────▼────┐                       │  │ └────┬─────────────┬──────┘           ││
    │Terminal │                       │  └──────┼─────────────┼──────────────────┘│
    │  Grid   │                       └─────────┼─────────────┼──────────────────┘
    └─────────┘                                 │             │
                                    ┌───────────▼──┐   ┌──────▼──────┐
                                    │ LangGraph    │   │ Anthropic   │
                                    │ Server       │   │ API         │
                                    │ (HttpEngine) │   │ (RigEngine) │
                                    └──────────────┘   └─────────────┘
```

## Key Crates

### infraware-shared
Shared API contract types: `LLMQueryResult` (Complete/CommandApproval/Question), `AgentEvent` (SSE events), `Interrupt` (HITL), `Message`, `ThreadId`, `RunInput`

### infraware-engine
Engine abstraction with pluggable backends:
- `AgenticEngine` trait: `create_thread()`, `stream_run()`, `resume_run()`, `health_check()`
- `RigEngine` - Native Rust agent using rig-rs + Anthropic Claude API (primary engine, HITL via tool execution)
- `MockEngine` - In-memory pattern matching (testing, no external dependencies)
- `HttpEngine` - Direct proxy to LangGraph HTTP endpoint (alternative for LangGraph deployments)
- `ProcessEngine` - Subprocess bridge with JSON-RPC over stdio (alternative for custom bridges)

### infraware-backend
Axum REST/SSE API:
- `GET /health` - Health check
- `GET /metrics` - Prometheus metrics
- `GET /api-docs/openapi.json` - OpenAPI spec
- `POST /api/auth` - Authentication validation
- `POST /threads` - Create new thread
- `POST /threads/{id}/runs/stream` - Stream run with SSE

### infraware-terminal
Terminal emulator with LLM integration. The main `app.rs` has been decomposed into focused submodules following a handler pattern:

**Core modules:**
- `app.rs` - Main `InfrawareApp` struct, eframe::App implementation, top-level update loop
- `app/state.rs` - Core application state struct (sessions map, buffers, flags)
- `state.rs` - `AppMode` state machine and `AgentState` (per-session mode tracking)
- `session.rs` - `TerminalSession` struct (each tab/pane has independent PTY, VTE parser, state)

**Handler modules (in `app/`):**
- `input_handler.rs` - Keyboard input processing and command classification
- `hitl_handler.rs` - Human-in-the-loop interaction handling (approval/answer flows)
- `llm_controller.rs` - LLM query management and background event dispatch
- `llm_event_handler.rs` - LLM event processing (SSE stream handling)
- `session_manager.rs` - Session lifecycle (create, close, initialize)
- `tiles_manager.rs` - Split view and tab management via egui_tiles
- `clipboard.rs` - Copy/paste operations
- `render.rs` - Terminal rendering state and helpers
- `terminal_renderer.rs` - Pure rendering logic (cell painting, cursor, decorations)
- `behavior.rs` - egui_tiles `Behavior` trait implementation

**Other directories:**
- `terminal/` - VTE parser (`handler.rs`), grid (`grid.rs`), cell attributes (`cell.rs`)
- `pty/` - PTY session, async I/O, DI traits
- `llm/` - HTTP/SSE client, markdown→ANSI renderer (syntect highlighting)
- `input/` - Keyboard mapping, text selection, command classification, prompt detection
- `orchestrators/` - Only `hitl.rs` utility function (`parse_approval()`)
- `auth/` - Authentication (trait + HTTP/Mock implementations)
- `ui/` - egui helpers, theme, scrollbar
- `config.rs` - Constants (timing, rendering, sizes)

**Tab/Split View Architecture**: Uses `egui_tiles` for window management. Each `TerminalSession` represents an independent terminal pane with its own PTY process, VTE parser, and app mode state. Tabs are created at root level; splits can nest within tabs.

### State Machine Flow
```
Normal
  │
  ├──(? query)──► WaitingLLM ──┬──► AwaitingApproval (y/n for commands)
                                │       │ (approve) ──► ExecutingCommand
                                │       │                    │
                                │       │                    ├──► WaitingLLM (needs_continuation=true)
                                │       │                    │
                                │       │                    └──► Normal (needs_continuation=false)
                                │       │
                                │       └──► Normal (reject)
                                │
                                ├──► AwaitingAnswer (free-text for questions)
                                │       └──► WaitingLLM (resume with answer)
                                │
                                └──► Normal (complete, no further action)
```

**ExecutingCommand State**: When user approves a shell command in RigEngine, the terminal enters ExecutingCommand to capture output. After command execution, the `needs_continuation` flag controls whether the agent continues reasoning with the output or completes the interaction.

## Terminal Quick Reference

| Task | Location |
|------|----------|
| Add keyboard shortcut | `terminal-app/src/input/keyboard.rs` |
| Modify terminal rendering | `terminal-app/src/app/terminal_renderer.rs` and `app/render.rs` |
| Add VTE escape handler | `terminal-app/src/terminal/handler.rs` → `csi_dispatch()` |
| Modify LLM query flow | `terminal-app/src/app/llm_controller.rs` and `app/llm_event_handler.rs` |
| Handle HITL interrupts | `terminal-app/src/app/hitl_handler.rs` |
| Add tab/split behavior | `terminal-app/src/app/tiles_manager.rs` |
| Modify session lifecycle | `terminal-app/src/app/session_manager.rs` |
| Modify application state | `terminal-app/src/app/state.rs` (AppState) or `state.rs` (AppMode) |
| Handle keyboard input | `terminal-app/src/app/input_handler.rs` |
| Modify clipboard behavior | `terminal-app/src/app/clipboard.rs` |
| Change theme colors | `terminal-app/src/ui/theme.rs` |
| Change config constants | `terminal-app/src/config.rs` |

## Keyboard Shortcuts

| Shortcut | Action | Platform |
|----------|--------|----------|
| `Cmd+T` / `Ctrl+Shift+T` | New tab | macOS / Linux |
| `Cmd+W` / `Ctrl+Shift+W` | Close tab | macOS / Linux |
| `Ctrl+Tab` | Next tab | All |
| `Ctrl+Shift+Tab` | Previous tab | All |
| `Cmd+Shift+H` / `Ctrl+Shift+H` | Split horizontal | macOS / Linux |
| `Cmd+Shift+J` / `Ctrl+Shift+J` | Split vertical | macOS / Linux |
| `Cmd+C` / `Ctrl+Shift+C` | Copy | macOS / Linux |
| `Cmd+V` / `Ctrl+Shift+V` | Paste | macOS / Linux |
| `Ctrl+C` | SIGINT (interrupt) | All |
| `Ctrl+D` | EOF | All |
| `Ctrl+L` | Clear screen | All |
| `Ctrl+Shift+/` | Enter LLM mode | All |

## Configuration

Environment variables (via `.env` or shell):
```bash
# Terminal client
INFRAWARE_BACKEND_URL="http://localhost:8080"
ANTHROPIC_API_KEY="your-api-key"
LOG_LEVEL="debug"  # debug, info, warn, error

# Backend server
ENGINE_TYPE="mock"              # mock, http, process, rig
LANGGRAPH_URL="http://localhost:2024"  # for http/process engines
BRIDGE_SCRIPT="bin/engine-bridge/main.py"  # for process engine
ANTHROPIC_API_KEY="sk-..."      # for rig engine (native Rust agent)
PORT="8080"
API_KEY=""                      # empty = auth disabled
ALLOWED_ORIGINS="*"             # CORS origins
RATE_LIMIT_RPM="100"            # requests per minute (0 = disabled)
RUST_LOG="infraware_backend=debug"  # tracing level
```

## Code Style

### Rust Guidelines (Microsoft Pragmatic)
- All public types implement `Debug` (custom impl for sensitive data)
- Use `#[expect]` instead of `#[allow]` when lint suppression should be revisited
- Panic for programming errors, `Result` for expected failures
- Avoid weasel words in names (Service, Manager, Factory)
- Prefer splitting crates over monoliths

### Git Commits
- Format: `<type>: <description>` (max 50 chars, imperative mood)
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
- **NO** Co-Authored-By, emojis, or AI attribution
- Run `cargo fmt` before committing

### Error Handling
- Use `anyhow::Result` for application code
- Use `thiserror` for library error types
- Safe indexing: `.first()`, `.get()` instead of `[0]`

## Workspace Dependencies

Shared in root `Cargo.toml` under `[workspace.dependencies]`. Use `{ workspace = true }` to inherit:
- Async: `tokio`, `async-trait`, `futures`
- HTTP: `axum`, `tower`, `reqwest`
- Serde: `serde`, `serde_json`
- Error: `anyhow`, `thiserror`
- Logging: `tracing`, `log`

## Adding New Components

### New Engine Adapter
1. Create `crates/backend-engine/src/adapters/your_engine.rs`
2. Implement `AgenticEngine` trait:
   ```rust
   #[async_trait]
   pub trait AgenticEngine: Send + Sync + Debug {
       async fn create_thread(&self) -> Result<ThreadId>;
       async fn stream_run(&self, thread_id: &ThreadId, input: RunInput) -> Result<EventStream>;
       async fn resume_run(&self, thread_id: &ThreadId, response: ResumeResponse) -> Result<EventStream>;
       async fn health_check(&self) -> Result<HealthStatus>;
   }
   ```
3. Export from `adapters/mod.rs`
4. Add match arm in `backend-api/src/main.rs` for `ENGINE_TYPE`

### New API Types
1. Add to `crates/shared/src/models.rs` or `events.rs`
2. Export from `crates/shared/src/lib.rs`

## Skills (`.claude/skills/`)

When writing Rust code, these skills are automatically applied:

| Skill | When to Apply |
|-------|---------------|
| `microsoft-rust-guidelines` | All Rust code (safety, naming, panics, Debug impl) |
| `rig-rs` | Code using rig-rs (agents, tools, embeddings, completions, extractors, vector stores, MCP) |

**rig-rs key patterns:**
- Always set `max_tokens` for Anthropic
- Use `schemars::JsonSchema` for tool parameter schemas
- Use `Option<T>` for extractor fields
- Wrap vector stores in `Arc<RwLock<>>` for concurrent access
- Use `multi_turn()` for complex multi-step tool calling

## RigEngine: Native Rust Agent with Function Calling

The RigEngine uses **rig-rs** to build a native Rust agent with Anthropic Claude API and function calling support.

### How It Works

1. **Tool Registration**: ShellCommandTool and AskUserTool are registered with the agent via `.tool()`
2. **PromptHook Interception**: LLM tool calls are intercepted via `PromptHook::on_tool_call()` for HITL approval
3. **needs_continuation Flag**: Distinguishes command execution intent:
   - `false` (default): Command output IS the answer (e.g., `ls` → list files directly)
   - `true`: Command output is INPUT for agent processing (e.g., `uname -s` → then OS-specific instructions)

### Files Involved

- `crates/backend-engine/src/adapters/rig/orchestrator.rs` - Handles tool call interception and HITL flow
- `crates/backend-engine/src/adapters/rig/tools/shell.rs` - ShellCommandTool with needs_continuation parameter
- `crates/backend-engine/src/adapters/rig/tools/ask_user.rs` - AskUserTool for questions
- `crates/shared/src/events.rs` - Interrupt enum with needs_continuation field

## CI Pipeline

CI runs on PRs to `main` and pushes to `main` (only when `terminal-app/**` changes):
1. **Format Check**: `cargo fmt --all --check`
2. **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Test Coverage**: 75% minimum threshold (excludes `main.rs`, `app/behavior.rs`, `app/terminal_renderer.rs`, `app/render.rs`, `ui/`)
4. **Build**: Cross-platform (ubuntu-latest, macos-latest)

## Known Issues

### Command Injection Risk
Location: `terminal-app/src/app.rs` (`submit_hitl_input`)
LLM-suggested commands sent to PTY without validation. Mitigation TODO: command blocklist, dangerous command warnings.
