# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware** is a Rust single-crate project containing an AI-powered terminal emulator with an integrated LLM agent
for DevOps assistance. The terminal combines VTE-based terminal emulation with an in-process agentic engine.

**Key feature**: Prefix any command with `?` for natural language queries (e.g., `? how do I revert a git commit`)

**Tech Stack**: Rust 2024 edition, single crate, egui/eframe, tokio

**Prerequisites** (Linux):

```bash
sudo apt install -y pkg-config libssl-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

## Commands

```bash
# Build & Run
cargo build                          # Build the crate
cargo run                            # Run terminal app
LOG_LEVEL=debug cargo run            # With debug logging

# Run with different engines
ENGINE_TYPE=mock cargo run           # MockEngine (for testing)
ENGINE_TYPE=rig ANTHROPIC_API_KEY=sk-... cargo run  # RigEngine (default)

# Testing
cargo test                           # All tests
cargo test -- test_name              # Single test by name
cargo test -- --nocapture            # With output

# Watch mode (auto-rebuild)
cargo watch -x run                   # Requires: cargo install cargo-watch

# Linting (CI enforces both)
cargo +nightly fmt --all && cargo clippy  # Always format with nightly to support rustfmt.toml rules
cargo clippy -- -D warnings          # CI-strict mode (warnings = errors)

# Coverage (CI enforces 50% minimum, excluding UI/PTY/VTE modules)
cargo llvm-cov --all-features --lcov --output-path lcov.info
cargo llvm-cov --all-features --summary-only  # Quick summary
```

## Project Structure

```
├── src/
│   ├── main.rs                # Entry point
│   ├── app.rs                 # InfrawareApp struct, eframe::App impl
│   ├── app/                   # App submodules (handlers, rendering)
│   ├── state.rs               # AppMode state machine, AgentState
│   ├── session.rs             # TerminalSession (PTY + VTE + state per tab)
│   ├── config.rs              # Constants (timing, rendering, sizes)
│   ├── engine.rs              # Engine module root (re-exports)
│   ├── engine/                # AgenticEngine trait + adapters
│   │   ├── traits.rs          # AgenticEngine trait, EventStream type
│   │   ├── error.rs           # EngineError type
│   │   ├── types.rs           # HealthStatus, ResumeResponse
│   │   ├── shared/            # Shared types (AgentEvent, Interrupt, etc.)
│   │   └── adapters/          # Engine implementations
│   │       ├── mock/          # MockEngine (testing)
│   │       └── rig/           # RigEngine (Anthropic Claude, default)
│   ├── terminal/              # VTE parser, grid, cell attributes
│   ├── pty/                   # PTY session, async I/O, DI traits
│   ├── llm/                   # Markdown→ANSI renderer (syntect highlighting)
│   ├── input/                 # Keyboard mapping, text selection, command classification
│   ├── orchestrators/         # hitl.rs utility (parse_approval)
│   └── ui/                    # egui helpers, theme, scrollbar
└── docs/                      # Design documents and technical debt analysis
```

## Architecture

```
┌─────────────────────────────────────────────────┐
│ infraware-terminal (single binary)              │
│                                                 │
│  ┌───────────┐      ┌────────────────────────┐  │
│  │ Terminal   │      │ AgenticEngine (trait)   │  │
│  │ UI (egui) │─────►│ ┌────────┐ ┌─────────┐ │  │
│  └─────┬─────┘      │ │ Mock   │ │ Rig     │ │  │
│        │            │ │ Engine │ │ Engine  │ │  │
│   ┌────▼────┐       │ └────────┘ └────┬────┘ │  │
│   │   PTY   │       └────────────────┼──────┘  │
│   │ Session │                        │          │
│   └────┬────┘                        │          │
│        │                             │          │
│   ┌────▼────┐                 ┌──────▼──────┐   │
│   │  VTE    │                 │ Anthropic   │   │
│   │ Parser  │                 │ API         │   │
│   └────┬────┘                 └─────────────┘   │
│        │                                        │
│   ┌────▼────┐                                   │
│   │Terminal │                                   │
│   │  Grid   │                                   │
│   └─────────┘                                   │
└─────────────────────────────────────────────────┘
```

## Key Modules

### engine (src/engine/)

Engine abstraction with pluggable backends, running in-process:

- `AgenticEngine` trait: `create_thread()`, `stream_run()`, `resume_run()`, `health_check()`
- `RigEngine` - Native Rust agent using rig-rs + Anthropic Claude API (default engine, HITL via tool execution)
- `MockEngine` - In-memory workflow-based matching (testing, no external dependencies)

**Shared types** (in `src/engine/shared/`): `AgentEvent` (engine events), `Interrupt` (HITL),
`Message`, `ThreadId`, `RunInput`, `EngineStatus`

### Terminal UI (src/)

Terminal emulator with LLM integration. The main `app.rs` has been decomposed into focused submodules following a
handler pattern:

**Core modules:**

- `app.rs` - Main `InfrawareApp` struct, eframe::App implementation, top-level update loop
- `app/state.rs` - Core application state struct (sessions map, buffers, flags)
- `state.rs` - `AppMode` state machine and `AgentState` (per-session mode tracking)
- `session.rs` - `TerminalSession` struct (each tab/pane has independent PTY, VTE parser, state)

**Handler modules (in `app/`):**

- `input_handler.rs` - Keyboard input processing and command classification
- `hitl_handler.rs` - Human-in-the-loop interaction handling (approval/answer flows)
- `llm_controller.rs` - LLM query management and background event dispatch
- `llm_event_handler.rs` - LLM event processing (engine event stream handling)
- `session_manager.rs` - Session lifecycle (create, close, initialize)
- `tiles_manager.rs` - Split view and tab management via egui_tiles
- `clipboard.rs` - Copy/paste operations
- `render.rs` - Terminal rendering state and helpers
- `terminal_renderer.rs` - Pure rendering logic (cell painting, cursor, decorations)
- `behavior.rs` - egui_tiles `Behavior` trait implementation

**Other directories:**

- `terminal/` - VTE parser (`handler.rs`), grid (`grid.rs`), cell attributes (`cell.rs`)
- `pty/` - PTY session, async I/O, DI traits
- `llm/` - Markdown to ANSI renderer (syntect highlighting)
- `input/` - Keyboard mapping, text selection, command classification, prompt detection
- `orchestrators/` - Only `hitl.rs` utility function (`parse_approval()`)
- `ui/` - egui helpers, theme, scrollbar
- `config.rs` - Constants (timing, rendering, sizes)

**Tab/Split View Architecture**: Uses `egui_tiles` for window management. Each `TerminalSession` represents an
independent terminal pane with its own PTY process, VTE parser, and app mode state. Tabs are created at root level;
splits can nest within tabs.

### State Machine Flow

```
Normal
  │
  ├──(? query)──► WaitingLLM ──┬──► AwaitingApproval (y/n for commands, has needs_continuation)
                                │       │ (approve) ──► ExecutingCommand (has needs_continuation)
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

**ExecutingCommand State**: When user approves a shell command, the terminal enters ExecutingCommand to
capture output. After command execution, the `needs_continuation` flag controls whether the agent continues reasoning
with the output or completes the interaction.

## Terminal Quick Reference

| Task                      | Location                                                      |
|---------------------------|---------------------------------------------------------------|
| Add keyboard shortcut     | `src/input/keyboard.rs`                                       |
| Modify terminal rendering | `src/app/terminal_renderer.rs` and `src/app/render.rs`        |
| Add VTE escape handler    | `src/terminal/handler.rs` -> `csi_dispatch()`                 |
| Modify LLM query flow     | `src/app/llm_controller.rs` and `src/app/llm_event_handler.rs`|
| Handle HITL interrupts    | `src/app/hitl_handler.rs`                                     |
| Add tab/split behavior    | `src/app/tiles_manager.rs`                                    |
| Modify session lifecycle  | `src/app/session_manager.rs`                                  |
| Modify application state  | `src/app/state.rs` (AppState) or `src/state.rs` (AppMode)     |
| Handle keyboard input     | `src/app/input_handler.rs`                                    |
| Modify clipboard behavior | `src/app/clipboard.rs`                                        |
| Change theme colors       | `src/ui/theme.rs`                                             |
| Change config constants   | `src/config.rs`                                               |
| Add engine adapter        | `src/engine/adapters/`                                        |
| Modify shared types       | `src/engine/shared/`                                          |

## Keyboard Shortcuts

| Shortcut                       | Action             | Platform      |
|--------------------------------|--------------------|---------------|
| `Cmd+T` / `Ctrl+Shift+T`       | New tab            | macOS / Linux |
| `Cmd+W` / `Ctrl+Shift+W`       | Close tab          | macOS / Linux |
| `Ctrl+Tab`                     | Next tab           | All           |
| `Ctrl+Shift+Tab`               | Previous tab       | All           |
| `Cmd+Shift+H` / `Ctrl+Shift+H` | Split horizontal   | macOS / Linux |
| `Cmd+Shift+J` / `Ctrl+Shift+J` | Split vertical     | macOS / Linux |
| `Cmd+C` / `Ctrl+Shift+C`       | Copy               | macOS / Linux |
| `Cmd+V` / `Ctrl+Shift+V`       | Paste              | macOS / Linux |
| `Ctrl+C`                       | SIGINT (interrupt) | All           |
| `Ctrl+D`                       | EOF                | All           |
| `Ctrl+L`                       | Clear screen       | All           |
| `Ctrl+Shift+/`                 | Enter LLM mode     | All           |

## Configuration

Environment variables (via `.env` or shell):

```bash
# Engine selection
ENGINE_TYPE="rig"                    # rig (default), mock
MOCK_WORKFLOW_FILE="path/to/wf.json" # MockEngine workflow file

# Anthropic / RigEngine
ANTHROPIC_API_KEY="sk-..."           # Required for rig engine
ANTHROPIC_MODEL="claude-sonnet-4-20250514"  # Model to use
RIG_MAX_TOKENS="8096"               # Max tokens per response
RIG_TEMPERATURE="1.0"               # Sampling temperature
RIG_TIMEOUT_SECS="120"              # Request timeout

# Memory (RigEngine)
MEMORY_PATH="./.infraware/memory.json"  # Session memory JSON path
MEMORY_LIMIT="200"                      # Max session memory entries

# Application
LOG_LEVEL="debug"                    # debug, info, warn, error
```

## Code Style

### Rust Guidelines (Microsoft Pragmatic)

- All public types implement `Debug` (custom impl for sensitive data)
- Use `#[expect]` instead of `#[allow]` when lint suppression should be revisited
- Panic for programming errors, `Result` for expected failures
- Avoid weasel words in names (Service, Manager, Factory)

### Git Commits

- Format: `<type>: <description>` (max 50 chars, imperative mood)
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
- **NO** Co-Authored-By, emojis, or AI attribution
- Run `cargo fmt` before committing

### Pull Requests

- Include how to test (commands and expected outcome)
- Include screenshots or recordings for UI changes
- Link related issues if applicable

### Error Handling

- Use `anyhow::Result` for application code
- Use `thiserror` for library error types
- Safe indexing: `.first()`, `.get()` instead of `[0]`

## Dependencies

Key dependencies in `Cargo.toml`:

- GUI: `egui`, `eframe`, `egui_tiles`
- Terminal: `portable-pty`, `vte`
- Async: `tokio`, `async-trait`, `futures`
- Serde: `serde`, `serde_json`
- Error: `anyhow`, `thiserror`
- Logging: `tracing`, `tracing-subscriber`
- AI (behind `rig` feature): `rig-core`, `chrono`, `schemars`

## Adding New Components

### New Engine Adapter

1. Create `src/engine/adapters/your_engine.rs` (or a directory `src/engine/adapters/your_engine/`)
2. Implement `AgenticEngine` trait:
   ```rust
   #[async_trait]
   pub trait AgenticEngine: Send + Sync + Debug {
       async fn create_thread(&self, metadata: Option<serde_json::Value>) -> Result<ThreadId, EngineError>;
       async fn stream_run(&self, thread_id: &ThreadId, input: RunInput) -> Result<EventStream, EngineError>;
       async fn resume_run(&self, thread_id: &ThreadId, response: ResumeResponse) -> Result<EventStream, EngineError>;
       async fn health_check(&self) -> Result<HealthStatus, EngineError>;
   }
   ```
3. Export from `src/engine/adapters.rs`
4. Add construction logic in app initialization

### New Shared Types

1. Add to `src/engine/shared/models.rs` or `src/engine/shared/events.rs`
2. Re-export from `src/engine/shared.rs` and `src/engine.rs` as needed

## Skills (`.claude/skills/`)

When writing Rust code, these skills are automatically applied:

| Skill                       | When to Apply                                                                              |
|-----------------------------|--------------------------------------------------------------------------------------------|
| `microsoft-rust-guidelines` | All Rust code (safety, naming, panics, Debug impl)                                         |
| `rig-rs`                    | Code using rig-rs (agents, tools, embeddings, completions, extractors, vector stores, MCP) |

**rig-rs key patterns:**

- Always set `max_tokens` for Anthropic
- Use `schemars::JsonSchema` for tool parameter schemas
- Use `Option<T>` for extractor fields
- Wrap vector stores in `Arc<RwLock<>>` for concurrent access
- Use `multi_turn()` for complex multi-step tool calling

## RigEngine: Native Rust Agent with Function Calling

The RigEngine uses **rig-rs** to build a native Rust agent with Anthropic Claude API and function calling support.

### How It Works

1. **Tool Registration**: ShellCommandTool, AskUserTool, DiagnosticCommandTool, and StartIncidentTool are registered with the agent via `.tool()`
2. **PromptHook Interception**: LLM tool calls are intercepted via `PromptHook::on_tool_call()` for HITL approval
3. **needs_continuation Flag**: Distinguishes command execution intent:

- `false` (default): Command output IS the answer (e.g., `ls` -> list files directly)
- `true`: Command output is INPUT for agent processing (e.g., `uname -s` -> then OS-specific instructions)

### Files Involved

- `src/engine/adapters/rig/orchestrator.rs` - Handles tool call interception and HITL flow
- `src/engine/adapters/rig/tools/shell.rs` - ShellCommandTool with needs_continuation parameter
- `src/engine/adapters/rig/tools/ask_user.rs` - AskUserTool for questions
- `src/engine/adapters/rig/tools/diagnostic_command.rs` - DiagnosticCommandTool
- `src/engine/adapters/rig/tools/start_incident.rs` - StartIncidentTool
- `src/engine/shared/events.rs` - Interrupt enum with needs_continuation field

## Memory System (RigEngine Feature)

### Session Memory (Complete, Active)

Persistent facts/preferences the LLM learns about the user across sessions.

- **Storage**: JSON at `MEMORY_PATH` (default: `./.infraware/memory.json`)
- **How it works**: `MemoryStore::load_or_create()` loads on startup; `build_preamble()` injects stored facts into system prompt; `SaveMemoryTool` lets the agent persist new facts mid-conversation
- **Categories**: Preference, PersonalFact, Workflow, Restriction, Convention
- **Eviction**: FIFO rotation when `MEMORY_LIMIT` entries exceeded (default: 200)
- **Files**:
  - `src/engine/adapters/rig/memory/session.rs` - MemoryStore, MemoryEntry, SaveMemoryTool
  - `src/engine/adapters/rig/orchestrator.rs` - preamble injection, tool registration
  - `src/engine/adapters/rig/config.rs` - MemoryConfig with env var loading

### Interaction Memory (Framework Complete, Not Yet Wired)

Learns from past executed commands and NL queries via text similarity search.

- **Storage**: JSONL append-only at `~/.local/share/infraware/memory/interactions.jsonl`
- **Architecture**: Strategy pattern - `JsonlStorage` (Phase 1 text search) + `NoopEmbedding` (placeholder) + `RegexIntentGenerator` (heuristic intent)
- **Status**: Module code complete and tested; pre-retrieval/capture hooks not yet called from engine streams
- **Files**: `src/engine/adapters/rig/memory/` - `mod.rs`, `models.rs`, `traits.rs`, `storage/`, `intent/`

## CI Pipeline

CI runs on PRs to `main` and pushes to `main`:

1. **Format Check**: `cargo fmt --all --check`
2. **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings`
3. **Test Coverage**: 50% minimum threshold (excludes `main.rs`, `app/behavior.rs`, `app/terminal_renderer.rs`,
   `app/render.rs`, `ui/`)
4. **Build**: Cross-platform (ubuntu-latest, macos-latest)
