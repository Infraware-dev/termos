# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

> See the root [CLAUDE.md](../CLAUDE.md) for workspace-wide commands, architecture, and code style guidelines.

## Terminal App Overview

**infraware-terminal** is the egui-based terminal emulator frontend. It handles VTE emulation, PTY communication, and LLM integration via orchestrators.

## Commands

```bash
cargo run -p infraware-terminal          # Run terminal
cargo test -p infraware-terminal         # Run tests (36 tests)
LOG_LEVEL=debug cargo run -p infraware-terminal  # Debug mode
```

## Module Structure

| Directory | Purpose |
|-----------|---------|
| `terminal/` | VTE: `grid.rs` (terminal grid + scrollback), `cell.rs` (cell attributes), `handler.rs` (escape sequences) |
| `pty/` | PTY: `session.rs` (lifecycle), `io.rs` (async reader/writer), `traits.rs` (DI traits) |
| `input/` | Input: `keyboard.rs` (key mapping), `selection.rs` (text selection), `classifier.rs` (command vs NLP) |
| `llm/` | LLM: `client.rs` (HTTP+SSE client), `renderer.rs` (markdown→ANSI with syntect) |
| `orchestrators/` | Workflows: `natural_language.rs` (LLM queries), `hitl.rs` (human-in-the-loop approval) |
| `auth/` | Auth: `authenticator.rs` (trait + HTTP/Mock impl) |
| `ui/` | UI: `renderer.rs` (egui helpers), `theme.rs` (colors), `scrollbar.rs` |
| `app.rs` | Main egui application (~1,300 LOC) |
| `state.rs` | AppMode state machine |
| `config.rs` | Constants (timing, rendering, sizes) |

## Quick Reference

| Task | Location |
|------|----------|
| Add keyboard shortcut | `src/input/keyboard.rs` → `process_ctrl_keys()` or `process_other_keys()` |
| Modify terminal rendering | `src/app.rs` → `render_terminal()` |
| Modify scrollbar | `src/ui/scrollbar.rs` → `ScrollbarLogic` |
| Change cursor blink rate | `src/config.rs` → `timing::CURSOR_BLINK_INTERVAL` |
| Modify PTY spawn | `src/pty/mod.rs` → `Pty::spawn()` |
| Add VTE escape handler | `src/terminal/handler.rs` → `csi_dispatch()` |
| Modify cell attributes | `src/terminal/cell.rs` → `CellAttrs` |
| Change theme colors | `src/ui/theme.rs` → `Theme` |
| Add app mode | `src/state.rs` → `AppMode` enum |
| Modify LLM query flow | `src/orchestrators/natural_language.rs` |
| Handle HITL interrupts | `src/orchestrators/hitl.rs` |
| Change LLM rendering | `src/llm/renderer.rs` → `ResponseRenderer` |

## State Machine (AppMode)

```rust
pub enum AppMode {
    Normal,                                    // Default state
    WaitingLLM,                                // Querying LLM
    AwaitingApproval { command, message },     // LLM wants to execute command (y/n)
    AwaitingAnswer { question, options },      // LLM asks question (free-text)
}
```

Valid transitions:
- `Normal → WaitingLLM` (user types `? query`)
- `WaitingLLM → Normal` (LLM completes)
- `WaitingLLM → AwaitingApproval` (LLM suggests command)
- `WaitingLLM → AwaitingAnswer` (LLM asks question)
- `AwaitingApproval → Normal` (user responds y/n)
- `AwaitingAnswer → Normal` (user provides answer)
- `Any → Normal` (cancel)

## Constants (src/config.rs)

```rust
// Timing
CURSOR_BLINK_INTERVAL: 530ms
SHELL_INIT_DELAY: 500ms
RESIZE_DEBOUNCE: 100ms

// Rendering
MAX_BYTES_PER_FRAME: 4096
FONT_SIZE: 14.0
CHAR_WIDTH: 8.4
CHAR_HEIGHT: 16.0

// Default terminal size
DEFAULT_ROWS: 24
DEFAULT_COLS: 80
```

## Design Patterns

- **State Machine**: `state.rs` (AppMode with validated transitions)
- **Dependency Injection**: `pty/traits.rs` (PtyWrite, PtyControl), `llm/client.rs` (LLMClientTrait)
- **Single-Pass Rendering**: `app.rs` (batched backgrounds, text, decorations)
- **Orchestrator Pattern**: `orchestrators/` (workflow coordination)
- **SSE Streaming**: `llm/client.rs` (real-time LLM responses)

## Keyboard Shortcuts

| Shortcut | Action | Bytes |
|----------|--------|-------|
| `Ctrl+C` | Interrupt | 0x03 |
| `Ctrl+D` | EOF | 0x04 |
| `Ctrl+L` | Clear screen | 0x0C |
| `Ctrl+A/E` | Start/End of line | 0x01/0x05 |
| `Ctrl+K/U` | Kill to end/start | 0x0B/0x15 |
| `Ctrl+W` | Delete word | 0x17 |
| `Ctrl+R` | Reverse search | 0x12 |
| `Ctrl+Z` | Suspend | 0x1A |
| Arrow keys | Navigation | VT100 |
| Page Up/Down | Scroll viewport | VT100 |
