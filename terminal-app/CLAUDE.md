# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> See the root [CLAUDE.md](../CLAUDE.md) for workspace-wide commands, architecture, and code style guidelines.

## Terminal App Overview

**infraware-terminal** is the egui-based terminal emulator frontend. It handles VTE emulation, PTY communication, and LLM integration. The main application logic (`app.rs`) has been decomposed into focused submodules following a handler pattern.

## Commands

```bash
cargo run -p infraware-terminal          # Run terminal
cargo test -p infraware-terminal         # Run all tests
cargo test -p infraware-terminal -- test_name  # Single test
LOG_LEVEL=debug cargo run -p infraware-terminal  # Debug mode
```

## Module Structure

### Core application (`app.rs` + `app/`)

The `InfrawareApp` struct in `app.rs` delegates to focused handler modules:

| Module | Purpose |
|--------|---------|
| `app/state.rs` | Core application state struct (sessions map, buffers, flags) |
| `app/input_handler.rs` | Keyboard input processing and command classification |
| `app/hitl_handler.rs` | Human-in-the-loop interaction (approval/answer flows) |
| `app/llm_controller.rs` | LLM query management and background event dispatch |
| `app/llm_event_handler.rs` | LLM SSE event processing |
| `app/session_manager.rs` | Session lifecycle (create, close, initialize) |
| `app/tiles_manager.rs` | Split view and tab management via egui_tiles |
| `app/clipboard.rs` | Copy/paste operations |
| `app/render.rs` | Terminal rendering state and helpers |
| `app/terminal_renderer.rs` | Pure rendering logic (cell painting, cursor, decorations) |
| `app/behavior.rs` | egui_tiles `Behavior` trait implementation |

### Other modules

| Directory/File | Purpose |
|----------------|---------|
| `terminal/` | VTE: `grid.rs` (terminal grid + scrollback), `cell.rs` (cell attributes), `handler.rs` (escape sequences) |
| `pty/` | PTY: `session.rs` (lifecycle), `io.rs` (async reader/writer), `traits.rs` (DI traits) |
| `input/` | Input: `keyboard.rs` (key mapping), `selection.rs` (text selection), `classifier.rs` (command vs NLP), `prompt_detector.rs`, `output_capture.rs` |
| `llm/` | LLM: `client.rs` (HTTP+SSE client), `renderer.rs` (markdown→ANSI with syntect) |
| `orchestrators/` | Only `hitl.rs` — single utility function `parse_approval()` |
| `auth/` | Auth: `authenticator.rs` (trait + HTTP/Mock impl) |
| `ui/` | UI: `renderer.rs` (egui helpers), `theme.rs` (colors), `scrollbar.rs` |
| `session.rs` | `TerminalSession` — each tab/pane (independent PTY, VTE parser, AppMode) |
| `state.rs` | `AppMode` state machine and `AgentState` (per-session mode tracking) |
| `config.rs` | Constants (timing, rendering, sizes, PTY channel config) |

## Quick Reference

| Task | Location |
|------|----------|
| Add keyboard shortcut | `src/input/keyboard.rs` → `process_tab_keys()`, `process_ctrl_keys()`, or `process_other_keys()` |
| Modify terminal rendering | `src/app/terminal_renderer.rs` and `src/app/render.rs` |
| Add/modify tab behavior | `src/app/tiles_manager.rs` |
| Modify session lifecycle | `src/app/session_manager.rs` |
| Handle keyboard input | `src/app/input_handler.rs` |
| Modify LLM query flow | `src/app/llm_controller.rs` and `src/app/llm_event_handler.rs` |
| Handle HITL interrupts | `src/app/hitl_handler.rs` |
| Modify clipboard behavior | `src/app/clipboard.rs` |
| Modify application state | `src/app/state.rs` (AppState) or `src/state.rs` (AppMode) |
| Modify scrollbar | `src/ui/scrollbar.rs` → `ScrollbarLogic` |
| Change cursor blink rate | `src/config.rs` → `timing::CURSOR_BLINK_INTERVAL` |
| Modify PTY spawn | `src/pty/mod.rs` → `Pty::spawn()` |
| Add VTE escape handler | `src/terminal/handler.rs` → `csi_dispatch()` |
| Modify cell attributes | `src/terminal/cell.rs` → `CellAttrs` |
| Change theme colors | `src/ui/theme.rs` → `Theme` |
| Add app mode | `src/state.rs` → `AppMode` enum |
| Change LLM rendering | `src/llm/renderer.rs` → `ResponseRenderer` |

## State Machine (AppMode)

```rust
pub enum AppMode {
    Normal,                                    // Default state
    WaitingLLM,                                // Querying LLM
    AwaitingApproval { command, message },     // LLM wants to execute command (y/n)
    AwaitingAnswer { question, options },      // LLM asks question (free-text)
    ExecutingCommand { command },              // Running approved command, capturing output
}
```

Valid transitions:
- `Normal → WaitingLLM` (user types `? query`)
- `WaitingLLM → Normal` (LLM completes)
- `WaitingLLM → AwaitingApproval` (LLM suggests command)
- `WaitingLLM → AwaitingAnswer` (LLM asks question)
- `AwaitingApproval → ExecutingCommand` (user approves)
- `AwaitingApproval → Normal` (user rejects)
- `ExecutingCommand → WaitingLLM` (needs_continuation=true)
- `ExecutingCommand → Normal` (needs_continuation=false)
- `AwaitingAnswer → WaitingLLM` (user provides answer)
- `Any → Normal` (cancel)

## Constants (src/config.rs)

```rust
// Timing
CURSOR_BLINK_INTERVAL: 530ms
SHELL_INIT_DELAY: 500ms
INIT_COMMANDS_DELAY: 100ms
BACKGROUND_REPAINT: 500ms

// Rendering (adaptive)
MAX_BYTES_PER_FRAME_ACTIVE: 64KB   // During keyboard activity
MAX_BYTES_PER_FRAME_IDLE: 1MB      // When idle
FONT_SIZE: 14.0
CHAR_WIDTH: 8.4
CHAR_HEIGHT: 16.0

// Default terminal size
DEFAULT_ROWS: 24
DEFAULT_COLS: 80

// PTY
CHANNEL_CAPACITY: 512              // Sync channel slots (~6 frames headroom)
READER_BUFFER_SIZE: 64KB           // PTY reader buffer
```

## Design Patterns

- **Handler Decomposition**: `app/` — monolithic app.rs split into focused handler modules with clear responsibilities
- **Session Per Pane**: `session.rs` — each tab/split pane is an independent `TerminalSession` with its own PTY, VTE parser, and state
- **Tile-Based Layout**: `egui_tiles` manages tab bar and split panes; `SessionId` doubles as pane ID
- **State Machine**: `state.rs` — `AppMode` with validated transitions, per-session
- **Two State Files**: `state.rs` (AppMode/AgentState) vs `app/state.rs` (AppState struct with sessions map)
- **Dependency Injection**: `pty/traits.rs` (PtyWrite, PtyControl), `llm/client.rs` (LLMClientTrait)
- **Single-Pass Rendering**: `app/terminal_renderer.rs` (batched backgrounds, text, decorations)
- **SSE Streaming**: `llm/client.rs` (real-time LLM responses)
- **Adaptive Throughput**: `config.rs` — different PTY byte limits for active vs idle frames
