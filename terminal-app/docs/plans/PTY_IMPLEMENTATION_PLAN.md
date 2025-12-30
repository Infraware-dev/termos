# PTY Implementation Plan

> **Decision**: IMPLEMENT PTY
> **Target**: Terminal Replacement (Linux/macOS only)
> **Effort**: 9-11 days
> **Library**: `portable-pty`

---

## Current Architecture (without PTY)

```
User Input → TUI (ratatui/crossterm)
                    ↓
            ┌───────┴───────┐
            │               │
    Interactive         Non-Interactive
    (vim, htop)         (ls, grep)
            ↓               ↓
    suspend() TUI       Capture output
    → raw terminal      → display in TUI
    → resume()
```

### Current Limitations
- **28 interactive commands**: TUI suspend/resume (vim, nano, less, htop, etc.)
- **25 blocked commands**: ssh, tmux, python REPL, etc.
- **No PTY**: No ssh, tmux, screen, REPLs support

---

## What PTY Adds

| Feature | Without PTY | With PTY |
|---------|-------------|----------|
| ssh/tmux/screen | Blocked | Working |
| REPLs (python, node) | Blocked | Working |
| Password prompts | Limited | Native |
| ANSI colors | Parsed | Native |
| Terminal resize | Not propagated | SIGWINCH |
| Job control (Ctrl+Z) | Not working | Working |

---

## Implementation Plan

### Phase 1: Base Setup (2 days)

**New module**: `src/pty/`
```
src/pty/
├── mod.rs              # PTY wrapper
├── session.rs          # PTY session management
└── io.rs               # Async read/write
```

**Files to modify**:
- `Cargo.toml` → add `portable-pty = "0.8"`
- `src/lib.rs` → add `pub mod pty;`

### Phase 2: Executor Integration (2-3 days)

**Files to modify**:
- `src/executor/command.rs`:
  - New method `execute_in_pty(cmd, args)`
  - Replace `execute_interactive()` for PTY-required commands
- `src/orchestrators/command.rs`:
  - Route to PTY for ssh/tmux/REPLs

### Phase 3: TUI ↔ PTY Bridge (2-3 days)

**Files to modify**:
- `src/terminal/events.rs` → Forward keypresses to PTY
- `src/terminal/tui.rs` → Render PTY output in buffer
- `src/terminal/buffers.rs` → New `PtyOutputBuffer` (optional)

### Phase 4: Unblock Commands (1 day)

**Modify** `src/executor/command.rs`:
- Remove from `INTERACTIVE_BLOCKED`:
  - ssh, tmux, screen, ftp, sftp
  - python, python3, node, irb, ipython
  - mysql, psql, sqlite3, mongo, redis-cli
  - gdb, lldb, pdb
- Add to new `REQUIRES_PTY` list

### Phase 5: Testing (2 days)
- SIGWINCH (resize)
- Ctrl+C, Ctrl+Z forwarding
- Unicode/ANSI passthrough
- Exit detection

---

## Technical Approach: `portable-pty`

```rust
use portable_pty::{native_pty_system, PtySize, CommandBuilder};

// Open PTY
let pty_system = native_pty_system();
let pair = pty_system.openpty(PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
})?;

// Spawn command
let mut cmd = CommandBuilder::new("ssh");
cmd.arg("user@host");
let child = pair.slave.spawn_command(cmd)?;

// Async I/O
let reader = pair.master.try_clone_reader()?;
let writer = pair.master.take_writer()?;
```

**Benefits**:
- Clean, type-safe API
- Handles automatically: setsid, controlling terminal, signal groups
- Tested on Linux/macOS
- Supports resize via `pair.master.resize()`

---

## Dependencies

```toml
# Cargo.toml
[dependencies]
portable-pty = "0.8"    # PTY abstraction for Linux/macOS
```

---

## Risks and Mitigations

| Risk | Mitigation |
|------|------------|
| I/O race conditions | tokio::select! for multiplexing |
| Zombie processes | Proper SIGCHLD handling |
| PTY memory leaks | RAII pattern (Drop trait) |
| Blocking reads | Async wrapper with tokio::spawn_blocking |

---

## Final Result

After implementation:
- ssh, tmux, screen working
- REPLs (python, node, irb, etc.)
- Native password prompts
- Job control (Ctrl+Z, fg, bg)
- Dynamic resize (SIGWINCH)

---

## Files Summary

| File | Changes |
|------|---------|
| `Cargo.toml` | Add `portable-pty = "0.8"` |
| `src/lib.rs` | Add `pub mod pty;` |
| `src/pty/mod.rs` | NEW - PTY wrapper |
| `src/pty/session.rs` | NEW - Session management |
| `src/pty/io.rs` | NEW - Async I/O |
| `src/executor/command.rs` | Add `execute_in_pty()`, update `INTERACTIVE_BLOCKED` |
| `src/orchestrators/command.rs` | Route PTY commands |
| `src/terminal/events.rs` | Forward keys to PTY |
| `src/terminal/tui.rs` | Render PTY output |
