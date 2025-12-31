# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a VTE-based terminal emulator built with egui. It provides full terminal emulation with PTY support for running interactive shell sessions.

**Tech Stack**: Rust + egui/eframe + VTE + portable-pty
**Status**: Terminal Emulator Complete (0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

## Commands

```bash
# Build and Run
cargo run                            # Development (debug build)
cargo build --release                # Production build
cargo check                          # Fast type check
LOG_LEVEL=debug cargo run            # With debug logging

# Testing
cargo test                           # All tests (21 tests)
cargo test test_name                 # Single test
cargo test -- --nocapture            # With output

# Pre-commit (required)
cargo fmt && cargo clippy            # CI enforces both

# Coverage
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

## Architecture

```
User Input (egui) → KeyboardHandler → PTY writer → Shell (bash/zsh)
                                           ↓
                          PTY reader → VTE parser → Terminal grid
                                           ↓
                          egui renderer (backgrounds, text, scrollbar)
```

### Core Components

| Component | Purpose |
|-----------|---------|
| **VTE Parser** | Parses ANSI escape sequences (CSI, SGR, cursor control) |
| **Terminal Grid** | 2D cell array with scrollback buffer |
| **PTY Session** | Bidirectional I/O with shell process |
| **egui Renderer** | Single-pass rendering with text batching |

### Quick Reference: Where to Find X

| Task | Location |
|------|----------|
| Add keyboard shortcut | `src/input/keyboard.rs` → `process_ctrl_keys()` or `process_other_keys()` |
| Modify terminal rendering | `src/app.rs` → `render_terminal()` |
| Modify scrollbar | `src/ui/renderer.rs` → `render_scrollbar()` |
| Change cursor blink rate | `src/config.rs` → `timing::CURSOR_BLINK_INTERVAL` |
| Modify PTY spawn | `src/pty/mod.rs` → `Pty::spawn()` |
| Add VTE escape handler | `src/terminal/handler.rs` → `csi_dispatch()` |
| Modify cell attributes | `src/terminal/cell.rs` → `CellAttrs` |
| Change theme colors | `src/ui/theme.rs` → `Theme` struct |
| Add app mode | `src/state.rs` → `AppMode` enum |

### Key Modules

| Directory | Purpose |
|-----------|---------|
| `terminal/` | VTE: `grid.rs` (terminal grid + scrollback), `cell.rs` (cell attributes), `handler.rs` (escape sequence handler) |
| `pty/` | PTY: `manager.rs` (coordinator), `session.rs` (session lifecycle), `io.rs` (async reader/writer), `traits.rs` (DI traits) |
| `input/` | Input: `keyboard.rs` (keyboard event mapping) |
| `ui/` | UI: `renderer.rs` (egui helpers), `theme.rs` (colors), `prompt.rs` (placeholder) |
| `llm/` | LLM: `client.rs` (placeholder for future LLM integration) |
| `app.rs` | Main egui application with rendering loop |
| `state.rs` | Application state machine (AppMode, transitions) |
| `config.rs` | Configuration constants (timing, rendering, sizes) |

### Design Patterns
- **State Machine**: `state.rs` (AppMode with validated transitions)
- **Dependency Injection**: `pty/traits.rs` (PtyWrite, PtyControl traits for testing)
- **Single-Pass Rendering**: `app.rs` (batched backgrounds, text, decorations)

## Keyboard Shortcuts (Active)

| Shortcut | Action | Bytes Sent |
|----------|--------|------------|
| `Ctrl+C` | Interrupt (SIGINT) | 0x03 |
| `Ctrl+D` | EOF | 0x04 |
| `Ctrl+L` | Clear screen | 0x0C |
| `Ctrl+A` | Start of line | 0x01 |
| `Ctrl+E` | End of line | 0x05 |
| `Ctrl+K` | Kill to end | 0x0B |
| `Ctrl+U` | Kill to start | 0x15 |
| `Ctrl+W` | Delete word | 0x17 |
| `Ctrl+R` | Reverse search | 0x12 |
| `Ctrl+Z` | Suspend | 0x1A |
| `Alt+B` | Back word | ESC-b |
| `Alt+F` | Forward word | ESC-f |
| `Alt+D` | Delete word | ESC-d |
| Arrow keys | Navigation | VT100 codes |
| Page Up/Down | Scroll viewport | VT100 codes |
| Home/End | Line edges | VT100 codes |
| F1-F12 | Function keys | VT100 codes |

## Feature Status

### Active Features
- Full VTE terminal emulation (ANSI escape sequences)
- Terminal grid with scrollback buffer
- Mouse wheel scrolling with visual scrollbar
- Cursor blinking (530ms interval)
- 256-color and RGB color support
- Cell attributes (bold, dim, underline, reverse, strikethrough)
- Alternate screen buffer (for vim, less, etc.)
- PTY with async I/O and backpressure
- SIGINT propagation to foreground process
- Reactive repaint (CPU <5% when idle)

### Placeholder (Code Ready, Not Active)
- **LLM Client** (`llm/client.rs`): Complete HTTP client, marked `#[allow(dead_code)]`
- **AppMode variants**: WaitingLLM, AwaitingApproval, AwaitingAnswer (state machine ready)
- **Prompt rendering** (`ui/prompt.rs`): Placeholder module

### Not Implemented
- Command classification (SCAN algorithm)
- Shell builtins recognition
- History expansion (!!, !$)
- Alias expansion
- Background job management
- Command confirmations (rm -i)
- Multilingual patterns

## Configuration Constants

All in `src/config.rs`:

```rust
// Timing
CURSOR_BLINK_INTERVAL: 530ms
SHELL_INIT_DELAY: 500ms
RESIZE_DEBOUNCE: 100ms
BACKGROUND_REPAINT: 500ms

// Rendering
MAX_BYTES_PER_FRAME: 4096
FONT_SIZE: 14.0
CHAR_WIDTH: 8.4
CHAR_HEIGHT: 16.0

// Size
DEFAULT_ROWS: 24
DEFAULT_COLS: 80

// PTY
CHANNEL_CAPACITY: 4 (for backpressure)
```

## Development Guidelines

### Adding Keyboard Shortcuts
1. Edit `src/input/keyboard.rs`
2. Add to `process_ctrl_keys()` for Ctrl combinations
3. Add to `process_other_keys()` for special keys
4. Return `KeyboardAction::SendBytes(vec![...])` with appropriate bytes

### Adding VTE Escape Sequences
1. Edit `src/terminal/handler.rs`
2. Add match arm in `csi_dispatch()` for CSI sequences
3. Add match arm in `esc_dispatch()` for ESC sequences
4. Update grid state via `self.grid` methods

### Testing PTY Operations
Use the DI traits in `src/pty/traits.rs`:
```rust
// Create mock for testing
struct MockPtyWriter { ... }
impl PtyWrite for MockPtyWriter {
    fn write_bytes(&self, data: &[u8]) -> Result<usize> { ... }
}
```

### Performance Optimization
- `render_terminal()` uses single-pass rendering - don't add extra iterations
- Pre-allocate buffers outside loops (see `bg_rects`, `text_runs`, `decorations`)
- Use `column_x_coords` cache for X position lookups
- Limit PTY output to `MAX_BYTES_PER_FRAME` per frame

## Constraints

### CI/CD
- `cargo fmt --all --check` and `cargo clippy -- -D warnings` must pass
- Multi-platform: Linux (primary), Windows, macOS

### Git Commits
- Use conventional commit format: `<type>: <description>`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
- Maximum 50 characters for subject line, imperative mood ("Add" not "Added")
- **NO** Co-Authored-By, emojis, or AI attribution
- Run `cargo fmt` before committing

### Code Style
- Safe indexing (`.first()`, `.get()`) - no `parts[0]` or `.unwrap()` on arrays
- Prefer zero-copy and CoW over clone
- No dead code (use `#[allow(dead_code)]` with comment for intentional placeholders)

### Microsoft Pragmatic Rust Guidelines

**Key requirements:**
- All public types implement `Debug` (custom impl for sensitive data)
- Use `#[expect]` instead of `#[allow]` when lint suppression should be revisited
- Lock poisoning triggers fail-fast with `.expect()` (M-PANIC-IS-STOP)

### Error Handling
Use `anyhow::Result`. Display user-friendly messages, never crash.

### Logging
Standard `log` crate. `LOG_LEVEL=debug cargo run` for debug output.

## State Machine (AppMode)

```rust
pub enum AppMode {
    Normal,                           // Default state
    WaitingLLM,                       // Querying LLM (placeholder)
    AwaitingApproval { command, message },  // LLM command approval (placeholder)
    AwaitingAnswer { question, options },   // LLM question (placeholder)
}
```

Valid transitions:
- `Normal → WaitingLLM` (QueryLLM event)
- `WaitingLLM → Normal` (LLMCompleted event)
- `WaitingLLM → AwaitingApproval` (LLMRequestsApproval event)
- `WaitingLLM → AwaitingAnswer` (LLMAsksQuestion event)
- `AwaitingApproval → Normal` (UserResponded event)
- `AwaitingAnswer → Normal` (UserAnswered event)
- `Any → Normal` (Cancel event)

## Claude Code Agents

Agents in `.claude/agents/` are invoked automatically when appropriate:

| Agent | Purpose |
|-------|---------|
| `rust-clippy-enforcer` | Run clippy and fix warnings (before commits) |
| `rust-code-reviewer` | Code review for best practices |
| `code-metrics-analyzer` | LOC, complexity metrics |
| `docs-updater` | Update CLAUDE.md/README.md |
| `git-committer` | Create commits (no emojis, no Co-Author) |
| `uml-diagram-generator` | Generate PlantUML diagrams for code structure |

## Code Metrics Summary

| Metric | Value |
|--------|-------|
| Total LOC | ~4,350 |
| Source files | 21 |
| Functions | 202 |
| Tests | 21 |
| Largest file | `terminal/grid.rs` (744 LOC) |
| Highest complexity | `render_terminal()` (CC ~28) |

## Platform Notes

**Linux**: Primary platform. Full PTY support.

**Windows**: PTY via ConPTY. Some escape sequences may differ.

**macOS**: Full PTY support. Similar to Linux.
