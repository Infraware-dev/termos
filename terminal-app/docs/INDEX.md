# Infraware Terminal Documentation Index

## Overview

Infraware Terminal is a PTY-based terminal emulator built with Rust, egui, and VTE.

## Documentation Structure

```
docs/
├── INDEX.md                 # This file - navigation guide
├── SUPPORTED_COMMANDS.md    # Command support (all via PTY)
├── CODE_METRICS_REPORT.md   # Code quality metrics
├── plans/                   # Architecture decision records
│   └── PTY_IMPLEMENTATION_PLAN.md
└── UI/                      # UI mockups and screenshots
```

## Core Documentation

| Document | Description |
|----------|-------------|
| [SUPPORTED_COMMANDS.md](SUPPORTED_COMMANDS.md) | PTY-based command support explanation |
| [CODE_METRICS_REPORT.md](CODE_METRICS_REPORT.md) | Lines of code, complexity metrics |
| [CLAUDE.md](../CLAUDE.md) | Primary development guide (authoritative) |

## Source Code Structure

```
src/
├── app.rs              # Main application (InfrawareApp)
├── main.rs             # Entry point
├── config.rs           # Configuration
├── state.rs            # AppMode state machine
├── input/
│   └── keyboard.rs     # Keyboard event handling
├── pty/
│   ├── io.rs           # PtyReader, PtyWriter
│   ├── session.rs      # PtySession management
│   ├── manager.rs      # PtyManager
│   └── traits.rs       # PtyWrite, PtyControl traits
├── terminal/
│   ├── cell.rs         # Terminal cell representation
│   ├── grid.rs         # Terminal grid buffer
│   └── handler.rs      # VTE escape sequence handler
└── ui/
    ├── renderer.rs     # egui rendering
    ├── prompt.rs       # Prompt display
    └── theme.rs        # Color theme
```

## Key Concepts

### Architecture
- **egui/eframe**: Immediate-mode GUI framework
- **portable-pty**: Cross-platform PTY library
- **vte**: Terminal escape sequence parser

### Data Flow
```
Keyboard → PTY Writer → Shell (bash/zsh) → PTY Reader → VTE Parser → Terminal Grid → egui Render
```

### State Machine
`AppMode` enum in `src/state.rs`:
- `Normal` - Default state
- `WaitingLLM` - Awaiting LLM response (placeholder)
- `AwaitingApproval` - Command approval (placeholder)
- `AwaitingAnswer` - Question answer (placeholder)

## Development

See [CLAUDE.md](../CLAUDE.md) for:
- Build commands
- Code style guidelines
- Architecture details
- Feature status

## Historical Note

Previous documentation for SCAN-based architecture has been removed.
The codebase was refactored from ratatui/crossterm TUI to egui terminal emulator.
