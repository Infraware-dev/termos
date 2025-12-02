# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It intelligently routes user input to either shell command execution or an LLM backend for natural language queries.

**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Status**: M1 Complete + Backend Integration in Progress (0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)
**Recent**: Chain of Responsibility refactoring complete (ClassifierContext DI, HandlerPosition enum, external language config)
**Target Users**: DevOps engineers working with cloud environments (AWS/Azure)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

**Environment Variables**:
- `INFRAWARE_BACKEND_URL` - Backend API endpoint (e.g., `http://localhost:8000`)
- `BACKEND_API_KEY` - API key for LLM backend authentication

## Commands

```bash
# Build and Run
cargo build                          # Build
cargo build --release                # Release build
cargo run                            # Run application

# Testing
cargo test                           # All tests (~440 tests across unit/integration/doc)
cargo test --test classifier_tests   # SCAN algorithm tests (tests/classifier_tests.rs)
cargo test --test executor_tests     # Executor tests (tests/executor_tests.rs)
cargo test --test integration_tests  # Integration tests (tests/integration_tests.rs)
cargo test --test terminal_state_tests # Terminal state tests
cargo test --test interactive_command_test # Interactive command tests
cargo test test_name                 # Run single test by name
cargo test -- --nocapture            # Tests with output
cargo test -- --show-output          # Show println! even for passing tests

# Benchmarking (benches/scan_benchmark.rs)
cargo bench                          # All benchmarks
cargo bench scan_                    # SCAN benchmarks only
cargo bench scan_individual_handlers # Individual handler benchmarks (measures each handler in isolation)
cargo bench scan_full_classification # Full classification pipeline benchmarks

# Development (run before commits)
cargo fmt                            # Format code
cargo clippy                         # Lint (warnings = errors in CI)
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info  # Coverage
```

## Architecture

### Core Flow
```
User Input → Alias Expansion → InputClassifier → [Command Path | Natural Language Path]
              (if matches)    (11-handler chain)     ↓                    ↓
                           incl. History Expansion  CommandExecutor      LLMClient
                                                         ↓                    ↓
                                                    Shell Output      ResponseRenderer
```

### SCAN Algorithm (Shell-Command And Natural-language)

11-handler Chain of Responsibility executing in strict order (<100μs average):

| # | Handler | Purpose | Performance |
|---|---------|---------|-------------|
| 1 | EmptyInputHandler | Fast path for empty/whitespace | <1μs |
| 2 | HistoryExpansionHandler | `!!`, `!$`, `!^`, `!*` expansion | ~1-5μs |
| 3 | ApplicationBuiltinHandler | App builtins (clear, reload-aliases, reload-commands) | <1μs |
| 4 | ShellBuiltinHandler | 45+ builtins (., :, [, [[, export) | <1μs |
| 5 | PathCommandHandler | ./script.sh, /usr/bin/cmd | ~10μs |
| 6 | KnownCommandHandler | 60+ DevOps commands + PATH cache | <1μs hit |
| 7 | PathDiscoveryHandler | Auto-discover newly installed commands | ~1-5ms |
| 8 | CommandSyntaxHandler | Language-agnostic: flags, pipes, redirects | ~10μs |
| 9 | TypoDetectionHandler | Levenshtein ≤2 ("dokcer" → "docker"), disabled by default (max_distance=0) | ~100μs |
| 10 | NaturalLanguageHandler | Language-agnostic heuristics (universal patterns) | ~0.5μs |
| 11 | DefaultHandler | Fallback to LLM | <1μs |

**Key optimizations**: Precompiled RegexSet via `once_cell::Lazy`, thread-safe `RwLock<CommandCache>` with poisoning recovery, fast paths first.

### Key Modules

| Directory | Purpose | Key Files |
|-----------|---------|-----------|
| `terminal/` | TUI rendering and state | `tui.rs` (suspend/resume), `buffers.rs` (SRP buffers), `events.rs` (keyboard) |
| `input/` | SCAN Algorithm | `classifier.rs` (coordinator), `handler.rs` (11-handler chain), `known_commands.rs` (command registry) |
| `executor/` | Command execution | `command.rs` (async exec), `package_manager.rs` (Strategy pattern) |
| `orchestrators/` | Workflow coordination | `command.rs`, `natural_language.rs`, `tab_completion.rs` |
| `llm/` | LLM integration | `client.rs` (Mock/HTTP clients with HITL support), `renderer.rs` (syntax highlighting) |
| `auth/` | Backend authentication | `authenticator.rs` (HTTP/Mock auth), `config.rs` (env config), `models.rs` (API types) |
| `config/` | Configuration management | `language.rs` (multilingual patterns), `language.toml` (language config) |

### Design Patterns
- **Chain of Responsibility**: Input classification (`input/handler.rs`)
- **Strategy Pattern**: Package managers (`executor/package_manager.rs`)
- **Builder Pattern**: Terminal construction (`main.rs`)
- **SRP**: Orchestrators, buffer components

## Development Guidelines

### Adding New Commands
1. Add to `known_commands.rs` (single source of truth for KnownCommandHandler + TypoDetectionHandler)
2. Commands auto-verified against PATH and cached

### Adding New Handlers
1. Implement `InputHandler` trait in `handler.rs`
2. Add new position to `HandlerPosition` enum - ORDER MATTERS (fast paths first, expensive checks last)
3. Add to chain in `InputClassifier::new()` using the new `HandlerPosition` variant
4. Use precompiled patterns from `patterns.rs` - NEVER compile regex in handlers
5. If handler needs language-specific patterns, use `ClassifierContext::language_patterns`
6. If handler needs shared state (cache, patterns), access via `ClassifierContext` parameter (no global state)
7. Run `cargo bench scan_individual_handlers` to measure handler performance in isolation
8. Run `cargo bench scan_full_classification` to verify no regression in overall pipeline
9. Use `#[serial_test::serial]` for tests that modify shared global state (CommandCache, aliases)

### History Expansion
- Patterns: `!!` (previous cmd), `!$` (last arg), `!^` (first arg), `!*` (all args)
- Thread-safe via `Arc<RwLock<Vec<String>>>`
- Set via `InputClassifier::with_history()`
- Get-second-to-last semantics (current input already in history when classified)

### Aliases
- System: `/etc/bash.bashrc`, `/etc/bashrc`, `/etc/profile`, `/etc/profile.d/*.sh`
- User: `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc` (override system)
- Single-level expansion, O(1) HashMap lookup
- Security: `is_safe_alias()` rejects dangerous patterns
- Runtime reload: `reload-aliases` built-in command

### Built-in Commands

Application-specific commands recognized by `ApplicationBuiltinHandler` (position 3 in SCAN chain):
- `cd` - Change working directory (handled by parent process to affect shell state)
- `clear` - Clear terminal output buffer
- `exit` - Exit the terminal application
- `reload-aliases` - Reload aliases from system/user config files
- `reload-commands` - Clear command cache (use after installing new commands)
- `auth-status` - Check backend authentication status

These commands are recognized early in the classification chain to prevent misclassification as natural language.

### Output Scrolling

Infraware Terminal supports scrolling through command output when content exceeds the visible area:

- **Primary controls**: `PageUp` / `PageDown` keys scroll output one page at a time
- **Laptop-friendly alternative**: `Ctrl+↑` / `Ctrl+↓` scroll output (useful when Fn+PageUp/PageDown is inconvenient)
- **Visual feedback**: Vertical scrollbar (▲│█▼) appears on the right side of the output area when content exceeds visible lines
- **No conflict with history**: Arrow keys (↑/↓ without Ctrl) navigate command history; arrow keys with Ctrl navigate output
- **Auto-scroll**: Output automatically scrolls to the bottom when new command results arrive
- **Implementation**: Scroll position managed in `OutputBuffer`, rendered via ratatui's `Scrollbar` widget

### Interactive Commands
- **28 supported** (TUI suspends): vim, nvim, nano, emacs, less, more, man, top, htop, sudo, watch, mc, ranger, etc.
- **31 blocked** (helpful error): ssh, tmux, screen, python, mysql, gdb, etc.
- Unix/Linux/macOS only (Windows returns error)
- Implementation: `TerminalUI::suspend()` → command runs → `TerminalUI::resume()`
- Panic-safe via RAII `TuiGuard`

### Infinite Output Commands (Blocked)
Commands that would freeze the terminal by producing infinite output are blocked with helpful suggestions:
- `yes` - produces infinite "y" output
- `cat /dev/zero`, `cat /dev/urandom`, `cat /dev/random` - infinite device output
- `dd if=/dev/zero`, `dd if=/dev/urandom` - infinite data copy
- `ping` without `-c N` flag - infinite ping

**Not blocked** (useful for DevOps, Ctrl+C works): `tail -f`, `docker logs -f`, `watch`

### Shell Builtins
- 45+ recognized without PATH verification (., :, [, [[, export, eval, exec, etc.)
- Executed via `sh -c` (Unix) or `cmd /C` (Windows)
- `ShellBuiltinInfo` provides metadata: `requires_shell`, `unix_only`

### LLM Integration (Human-in-the-Loop)

The `HttpLLMClient` supports conversational AI with HITL (Human-in-the-Loop) interactions:

- **Thread-based conversations**: Maintains context via `/threads` API
- **SSE streaming**: Real-time responses via Server-Sent Events
- **LLMQueryResult enum**:
  - `Complete(String)` - Final response from LLM
  - `CommandApproval { command, message }` - LLM wants to execute a command (y/n)
  - `Question { question, options }` - LLM is asking a question (free-form text)
- **Resume methods**: `resume_run()` for approval, `resume_with_answer()` for questions
- **Authentication**: API key via `BACKEND_API_KEY` environment variable

### Error Handling
- Use `anyhow::Result` for all errors
- Display user-friendly messages in TUI, don't crash on failures

### Logging

The application uses `log4rs` for structured logging with size-based rotation:

- **Configuration**: Environment variables (see below)
- **Log File**: `infraware.log` with automatic rotation and gzip compression
- **Timestamp Format**: ISO 8601 with milliseconds (e.g., `[2025-12-02T10:30:45.123]`)
- **Usage**: Use `log::debug!()`, `log::info!()`, `log::warn!()`, `log::error!()`
- **Initialization**: `logging::init()` called in `main.rs` before starting TUI
- **Module**: `src/logging.rs`
- **HTTP Logging**: All LLM HTTP operations log with structured prefixes:
  - `[HTTP-OUT]` - Request initiated (includes URL)
  - `[HTTP-IN]` - Response received (includes status code and elapsed time in ms)
  - Example: `[HTTP-IN] POST /threads | status=200 OK | elapsed=333ms`
- **SSE Logging**: Per-chunk logs use `debug` level to reduce I/O overhead

**Running with debug logging**:
```bash
LOG_LEVEL=debug cargo run       # Debug level
LOG_LEVEL=trace cargo run       # Trace level (very verbose)
```

**Environment variables**:
| Variable | Description | Default |
|----------|-------------|---------|
| `LOG_LEVEL` | trace, debug, info, warn, error | `info` |
| `LOG_MAX_SIZE_MB` | Max file size before rotation | `10` |
| `LOG_MAX_FILES` | Rotated files to keep | `5` |
| `LOG_PATH` | Custom log directory | Platform-specific |

**Log file locations**:
- Linux: `~/.local/share/infraware-terminal/logs/`
- macOS: `~/Library/Logs/infraware-terminal/`
- Windows: `%APPDATA%\infraware-terminal\logs\`

## Constraints

### CI/CD
- `cargo fmt --all --check` must pass
- `cargo clippy --all-targets --all-features -- -D warnings` must pass
- 75% test coverage minimum (~440 tests across unit/integration/doc tests)
- Multi-platform: Ubuntu, Windows, macOS

### Git Commits
- **NO Co-Authored-By** in commit messages
- **Run `cargo fmt` before committing**
- Keep descriptions brief and concise

### Code Style
- SOLID principles, design patterns
- Prefer zero-copy and CoW over clone
- No dead code
- Safe indexing (`.first()`, `.get()`) - no `parts[0]` or `.unwrap()` on arrays

### Code Quality Standards

**Microsoft Pragmatic Rust Guidelines Compliance** (https://microsoft.github.io/rust-guidelines/):

- All public types implement `Debug` (custom impl for complex types to protect sensitive data)
- Use `#[expect]` instead of `#[allow]` for lint overrides
- Zero clippy warnings, all tests passing
- See `.claude/skills/microsoft-rust-guidelines.md` for detailed guidelines

### Multilingual Support

Language-specific patterns are now externalized to `config/language.toml`:

- **Configuration File Priority** (checked in order):
  1. `./config/language.toml` (project directory)
  2. `~/.config/infraware-terminal/language.toml` (user config)
  3. Built-in English defaults (fallback)
- **Supported Languages**: English (en), Italian (it), Spanish (es) - easily extensible
- **Pattern Types**: Single words, question patterns, article patterns, polite patterns
- **Usage**: Patterns loaded via `ClassifierContext::language_patterns` at initialization
- **Adding Languages**: Add new `[languages.xx]` section in `language.toml` with appropriate patterns

Example from `config/language.toml`:
```toml
[languages.it]
single_words = ["cosa", "come", "perché", "quando", "dove", "chi", "quale"]
question_patterns = ["(?i)^(come|cosa|perché|quando|dove|chi|quale)\\s"]
```

### M1 Scope Limitations (Deferred to M2/M3)
- Auto-install: Framework prompts but doesn't execute
- Tab completion: Basic only, no bash/zsh integration
- History: Session-only, not persisted to disk
- Markdown: Basic rendering only, no tables/images
- Cache TTL: No automatic invalidation (use `reload-commands` after installing new commands)

## Keyboard Shortcuts

The terminal implements the following keyboard shortcuts via the `EventHandler` in `src/terminal/events.rs`:

| Key(s) | Event | Purpose |
|--------|-------|---------|
| **Input & Submission** | | |
| Character (a-z, A-Z, 0-9, symbols) | `InputChar(c)` | Type character into input |
| Backspace | `DeleteChar` | Delete character before cursor |
| Enter | `Submit` | Execute command or query |
| ← | `MoveCursorLeft` | Move cursor left in input |
| → | `MoveCursorRight` | Move cursor right in input |
| **History Navigation** | | |
| ↑ (no modifiers) | `HistoryPrevious` | Navigate to previous command |
| ↓ (no modifiers) | `HistoryNext` | Navigate to next command |
| **Output Scrolling** | | |
| PageUp | `ScrollUp` | Scroll output up |
| PageDown | `ScrollDown` | Scroll output down |
| Ctrl+↑ | `ScrollUp` | Alternative scroll up (laptop-friendly) |
| Ctrl+↓ | `ScrollDown` | Alternative scroll down (laptop-friendly) |
| **Control** | | |
| Ctrl+C | `CtrlC` | Context-aware: cancel ops or clear input |
| Ctrl+L | `ClearScreen` | Clear screen |
| Tab | `TabComplete` | Tab completion |

**Key Implementation Details**:
- Windows support: Filters `KeyEventKind::Release` and `KeyEventKind::Repeat` to avoid duplicate input
- Arrow key conflict prevention: Ctrl+↑/↓ for scrolling, plain ↑/↓ for history (no collision)
- Event mapping in `EventHandler::map_key_event()` - single source of truth for keyboard handling

## Common Patterns

### Adding a TerminalEvent
1. Add variant to `TerminalEvent` in `events.rs`
2. Handle in `EventHandler::poll_event()`
3. Implement in `InfrawareTerminal::handle_event()` in `main.rs`

### Adding a Package Manager
1. Implement `PackageManager` trait in `package_manager.rs`
2. Add to `PackageInstaller::detect_package_manager()` in `install.rs`

### InputType Enum (src/input/classifier.rs)
- `Command { command, args, original_input }` - shell operators in `original_input`
- `NaturalLanguage(String)` - sent to LLM
- `Empty` - ignored
- `CommandTypo { input, suggestion, distance }` - shows suggestion

### ClassifierContext (Dependency Injection)

The `ClassifierContext` struct provides shared dependencies to all handlers via dependency injection:
- **Command Cache**: Thread-safe `Arc<RwLock<CommandCache>>` for PATH lookups and alias storage
- **Compiled Patterns**: Precompiled `Arc<CompiledPatterns>` from `patterns.rs` for performance
- **Language Patterns**: External language-specific patterns from `config/language.toml`

Context is passed to handlers that need shared state (e.g., `KnownCommandHandler`, `TypoDetectionHandler`, `NaturalLanguageHandler`). This design enables testability and avoids global state.

**Recent Refactoring**: Chain of Responsibility refactored to use explicit `HandlerPosition` enum (preventing accidental reordering), `ClassifierContext` for dependency injection (eliminating global state), and external language configuration (supporting multilingual patterns without code changes).

## Performance Targets

| Operation | Target |
|-----------|--------|
| Average classification | <100μs |
| Known command (cache hit) | <1μs |
| Typo detection | <100μs (disabled by default) |
| Natural language | <5μs |
| PATH lookup (cache miss) | 1-5ms |

Run `cargo bench scan_` to verify. Use `cargo bench scan_individual_handlers` to measure each handler in isolation and identify bottlenecks.

## Windows Notes

- Filter `KeyEventKind::Press` only in `events.rs` (crossterm generates Press/Repeat/Release)
- Shell execution: `cmd /C` instead of `sh -c`
- Interactive commands not supported (POSIX limitation)
