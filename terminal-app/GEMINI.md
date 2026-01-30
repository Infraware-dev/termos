# Infraware Terminal

**A VTE-based terminal emulator built with Rust and egui.**

> **⚠️ IMPORTANT CONTEXT**: This project has recently migrated from a TUI-based architecture (using `ratatui`/`crossterm`) to a GUI-based architecture (using `egui`/`eframe`). The `README.md` in the root directory describes the legacy TUI version and is currently **OUTDATED**. Please refer to this file and `CLAUDE.md` for the current state of the codebase.

## 🚀 Project Overview

Infraware Terminal provides full terminal emulation with PTY support for running interactive shell sessions. It intercepts commands for potential AI assistance (future feature) but currently functions primarily as a robust terminal emulator.

**Tech Stack:**
*   **Language:** Rust (2021 edition)
*   **GUI Framework:** `egui` + `eframe` (0.28)
*   **Terminal Emulation:** `vte` (parser) + `portable-pty` (pseudoterminal)
*   **Async Runtime:** `tokio`

**Current Status:**
*   ✅ Terminal Emulator Complete (VTE parsing, Grid management, PTY I/O)
*   ✅ Zero clippy warnings
*   ✅ Microsoft Pragmatic Rust Guidelines compliant
*   🚧 LLM Integration (Placeholders exist, but core logic is not active)

## 🛠️ Building and Running

**Prerequisites (Linux):**
```bash
sudo apt install -y pkg-config libssl-dev
```

**Common Commands:**
```bash
# Development (debug build)
cargo run

# Production build
cargo build --release

# Run with debug logging
LOG_LEVEL=debug cargo run

# Run all tests
cargo test

# Fast type check
cargo check
```

## 🏗️ Architecture

The application follows a unidirectional data flow for terminal emulation:

```mermaid
graph TD
    User[User Input] -->|egui events| Keyboard[KeyboardHandler]
    Keyboard -->|Bytes| PtyWriter[PTY Writer]
    PtyWriter -->|Stdin| Shell[Shell (bash/zsh)]
    Shell -->|Stdout/Stderr| PtyReader[PTY Reader]
    PtyReader -->|Bytes| VTE[VTE Parser]
    VTE -->|Updates| Grid[Terminal Grid]
    Grid -->|Draw commands| Renderer[egui Renderer]
```

### Key Components

| Component | Source File | Description |
| :--- | :--- | :--- |
| **App Entry** | `src/main.rs` | Entry point, window setup, Ctrl+C handling. |
| **Main Loop** | `src/app.rs` | `InfrawareApp` struct, update loop, render delegation. |
| **State Machine** | `src/state.rs` | `AppMode` enum (Normal, WaitingLLM, etc.). |
| **PTY Layer** | `src/pty/` | Manages shell processes (`manager.rs`, `session.rs`, `io.rs`). |
| **Terminal Core** | `src/terminal/` | Holds the grid state (`grid.rs`), cell data (`cell.rs`), and handles escapes (`handler.rs`). |
| **Input** | `src/input/` | Maps egui keyboard events to ANSI bytes (`keyboard.rs`). |
| **UI** | `src/ui/` | Rendering logic (`renderer.rs`), themes (`theme.rs`). |

## 📏 Development Conventions

### Code Style & Guidelines
*   **Microsoft Pragmatic Rust:**
    *   All public types must implement `Debug`.
    *   Use `#[expect]` instead of `#[allow]` where possible.
    *   Fail-fast on lock poisoning (`.expect()` on mutex locks).
*   **Safety:**
    *   Use `anyhow::Result` for error handling.
    *   Safe indexing only (`.get()`, `.first()`) - never index arrays directly/unwrap if avoidable.
*   **Formatting:** Run `cargo fmt` before committing.

### Testing
*   **Unit Tests:** `cargo test` runs 21+ tests covering the classifier (legacy/refactored parts) and executor.
*   **Coverage:** `cargo llvm-cov` is configured for coverage reporting.

### Commit Messages
*   Format: `<type>: <description>` (e.g., `feat: Add scrollbar support`)
*   Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
*   No emojis or "Co-Authored-By" lines.

## 📂 Directory Structure

```text
/
├── .claude/            # Agent definitions for Claude (useful context)
├── docs/               # Documentation (INDEX.md is the current guide)
│   └── plans/          # Implementation plans
├── src/
│   ├── app.rs          # Main application logic
│   ├── input/          # Keyboard and selection handling
│   ├── pty/            # PTY management
│   ├── terminal/       # VTE parser handler and grid
│   ├── ui/             # Egui rendering helpers
│   └── ...
├── Cargo.toml          # Dependencies (trust this over README)
├── CLAUDE.md           # Authoritative developer guide
└── README.md           # ⚠️ OUTDATED (describes previous TUI version)
```
