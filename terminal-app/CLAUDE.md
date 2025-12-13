# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It routes user input to either shell execution or an LLM backend.

**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Status**: M1 Complete (0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

**Environment Variables**: `INFRAWARE_BACKEND_URL` (backend endpoint), `BACKEND_API_KEY` (LLM auth)

## Commands

```bash
# Build and Run
cargo run                            # Development (debug build)
cargo build --release                # Production build
cargo check                          # Fast type check
LOG_LEVEL=debug cargo run            # With debug logging

# Testing
cargo test                           # All tests
cargo test --test classifier_tests   # SCAN algorithm tests
cargo test test_name                 # Single test
cargo test -- --nocapture            # With output
cargo test -- --test-threads=1       # For tests with shared state

# Benchmarking
cargo bench scan_individual_handlers # Handler isolation
cargo bench scan_full_classification # Full pipeline

# Pre-commit (required)
cargo fmt && cargo clippy            # CI enforces both

# Coverage
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

## Architecture

```
User Input → Alias Expansion → InputClassifier → [Command | NaturalLanguage]
                              (11-handler chain)      ↓            ↓
                                               CommandExecutor  LLMClient
```

### SCAN Algorithm (Shell-Command And Natural-language)

Chain of Responsibility with 11 handlers. **Order enforced by `HandlerPosition` enum** - do not reorder without understanding performance implications (fast paths first).

| Position | Handler | Target Time |
|----------|---------|-------------|
| 1 | EmptyInputHandler | <1μs |
| 2 | HistoryExpansionHandler (`!!`, `!$`, `!^`, `!*`) | ~5μs |
| 3 | ApplicationBuiltinHandler (cd, clear, exit, jobs, history) | <1μs |
| 4 | ShellBuiltinHandler (45+ builtins) | <1μs |
| 5 | PathCommandHandler (./script, /usr/bin/cmd, `&` suffix) | ~10μs |
| 6 | KnownCommandHandler (60+ DevOps commands + cache) | <1μs hit |
| 7 | PathDiscoveryHandler (newly installed commands) | 1-5ms |
| 8 | CommandSyntaxHandler (flags, pipes, redirects) | ~10μs |
| 9 | TypoDetectionHandler (Levenshtein ≤2, disabled) | ~100μs |
| 10 | NaturalLanguageHandler (universal patterns) | <5μs |
| 11 | DefaultHandler (LLM fallback) | <1μs |

### Quick Reference: Where to Find X

| Task | Location |
|------|----------|
| Add known command | `src/input/known_commands.rs` |
| Add app builtin | `src/input/application_builtins.rs` |
| Add shell builtin | `src/input/shell_builtins.rs` |
| Add keyboard shortcut | `src/terminal/events.rs` → `EventHandler::map_key_event()` |
| Add terminal event | `src/terminal/events.rs` → `TerminalEvent` enum |
| Modify TUI rendering | `src/terminal/tui.rs` |
| Modify throbber animation | `src/terminal/throbber.rs` (change `ANIMATION_INTERVAL_MS` constant) |
| Modify LLM render loop | `src/orchestrators/natural_language.rs` (change `RENDER_INTERVAL_MS` constant) |
| Add package manager | `src/executor/package_manager.rs` |
| Add shell confirmation | `src/orchestrators/command.rs` → `ConfirmationType` enum |
| Language patterns | `config/language.toml` |
| Precompiled regex | `src/input/patterns.rs` |

### Key Modules

| Directory | Purpose |
|-----------|---------|
| `terminal/` | TUI: `tui.rs` (rendering/suspend/resume), `buffers.rs` (SRP), `events.rs` (keyboard), `state.rs` (modes/root), `throbber.rs` (animation thread) |
| `input/` | SCAN: `classifier.rs` (coordinator), `handler.rs` (chain), `patterns.rs` (regex) |
| `executor/` | Execution: `command.rs` (async batch), `job_manager.rs` (background `&`) |
| `orchestrators/` | Workflows: `command.rs`, `natural_language.rs`, `tab_completion.rs` |
| `llm/` | LLM: `client.rs` (Mock/HTTP with HITL), `renderer.rs` (syntax highlighting) |
| `auth/` | Auth: `authenticator.rs`, `config.rs`, `models.rs` |
| `config/` | Config: `language.rs` (multilingual patterns from TOML) |
| `logging.rs` | Log4rs setup with size rotation |

### Design Patterns
- **Chain of Responsibility**: `input/handler.rs` (position-enforced)
- **Strategy Pattern**: `executor/package_manager.rs`
- **Dependency Injection**: `ClassifierContext` (cache, patterns, language config)

## Development Guidelines

### Adding New Handlers
1. Implement `InputHandler` trait in `handler.rs`
2. Add position to `HandlerPosition` enum (ORDER MATTERS - fast paths first)
3. Add to chain in `InputClassifier::new()`
4. Use precompiled patterns from `patterns.rs` - NEVER compile regex in handlers
5. Access shared state via `ClassifierContext` (no global state)
6. Run `cargo bench scan_individual_handlers` to verify performance

### Testing with Shared State
Use `#[serial_test::serial]` for tests modifying `CommandCache` or aliases to prevent flaky tests.

### Test Organization
Tests are in `tests/` directory:
- `classifier_tests.rs` - SCAN algorithm and handler tests
- `executor_tests.rs` - Command execution tests
- `integration_tests.rs` - End-to-end workflows
- `interactive_command_test.rs` - TUI suspend/resume tests
- `terminal_state_tests.rs` - State management tests

### History Expansion
`!!` (previous cmd), `!$` (last arg), `!^` (first arg), `!*` (all args). Thread-safe via `Arc<RwLock<Vec<String>>>`. Uses get-second-to-last semantics (current input already in history when classified).

### Aliases
System files loaded first, then user files (`~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`). Single-level expansion, O(1) lookup. `is_safe_alias()` rejects dangerous patterns. Runtime reload: `reload-aliases`.

### Interactive Commands
28 commands suspend TUI (vim, nano, less, etc.), 31 blocked with suggestions (ssh, tmux, python REPL). Implementation: `TerminalUI::suspend()` → run → `resume()` with RAII `TuiGuard` for panic safety. Unix only.

**Event polling**: Paused during interactive command execution to prevent input lag in editors like vim/nano.

### Command Execution & Cancellation
- **SIGINT handling**: Ctrl+C propagates to child processes via cancellation token
- **Output timeout**: 500ms timeout after SIGINT prevents blocking on process output
- **Graceful shutdown**: Commands receive SIGINT before forced termination

### Shell Command Confirmations
Matches native shell behavior for interactive flags (`-i`, `-I`):
- `rm -i`: Per-file confirmation ("rm: remove 'file'?")
- `rm -I`: Bulk confirmation (>3 files or recursive)
- `rm` on write-protected files: Automatic prompt (matches native rm)
- `cp -i`, `mv -i`, `ln -i`: Overwrite/replace confirmation

Implementation in `orchestrators/command.rs`. Uses `ConfirmationType` enum and `AwaitingCommandApproval` terminal mode. `y`/`n` response handling with proper file iteration for multi-file operations.

### Root Mode
Terminal detects `sudo su`, `su`, `su -` commands and enters root mode:
- Prompt symbol changes from `$` to `#`
- Tracks `is_root_mode` state in `TerminalState`
- `enter_root_mode()` / `exit_root_mode()` methods
- Also checks actual root user via UID=0

### Background Processes
`&` suffix → `JobManager` with `Arc<RwLock>`. 250ms polling interval. Lock poisoning triggers fail-fast per Microsoft guidelines.

### LLM Integration (HITL)
`HttpLLMClient` with SSE streaming. `LLMQueryResult` enum: `Complete`, `CommandApproval`, `Question`. Resume via `resume_run()` or `resume_with_answer()`. Animated throbber during LLM wait state:
- **Throbber Animation**: 10 FPS (100ms `ANIMATION_INTERVAL_MS` in `throbber.rs`)
- **Render Loop**: `NaturalLanguageOrchestrator::handle_query()` renders at 10 FPS via `RENDER_INTERVAL_MS` (100ms)
- **Visual States**: Animated braille symbols (⠘⠙⠚⠒) in `WaitingLLM` mode; static `|~|` in other modes
- **Controls**: `start_throbber()` / `stop_throbber()` in `TerminalState`
- **Prompt Prefix**: `get_prompt_prefix()` returns animated symbol only when `WaitingLLM` mode active

### Error Handling
Use `anyhow::Result`. Display user-friendly messages, never crash.

### Logging
`log4rs` with size rotation. `LOG_LEVEL=debug cargo run`. HTTP prefixes: `[HTTP-OUT]`, `[HTTP-IN]`.

## Constraints

### CI/CD
- `cargo fmt --all --check` and `cargo clippy -- -D warnings` must pass
- 75% test coverage minimum
- Multi-platform: Ubuntu, Windows, macOS

### Git Commits
- Use conventional commit format: `<type>: <description>`
- Types: `feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`, `style`
- Maximum 50 characters for subject line, imperative mood ("Add" not "Added")
- **NO** Co-Authored-By, emojis, or AI attribution
- Run `cargo fmt` before committing

### Code Style
- Safe indexing (`.first()`, `.get()`) - no `parts[0]` or `.unwrap()` on arrays
- Prefer zero-copy and CoW over clone
- No dead code

### Microsoft Pragmatic Rust Guidelines

See `.claude/skills/microsoft-rust-guidelines.md` for full details.

**Key requirements:**
- All public types implement `Debug` (custom impl for sensitive data)
- Use `#[expect]` instead of `#[allow]` for lint overrides
- Lock poisoning triggers fail-fast (M-PANIC-IS-STOP)

### Multilingual Support

`config/language.toml` contains language-specific patterns. Priority: `./config/language.toml` → `~/.config/infraware-terminal/language.toml` → English defaults. Add languages via `[languages.xx]` sections.

### M1 Limitations (Deferred)
- Auto-install prompts only, no execution
- Session-only history (not persisted)
- No cache TTL (use `reload-commands` after installing)

## Common Patterns

### Adding a TerminalEvent
1. Add variant to `TerminalEvent` in `events.rs`
2. Handle in `EventHandler::poll_event()`
3. Implement in `InfrawareTerminal::handle_event()` in `main.rs`

### InputType Enum
`Command { command, args, original_input }`, `NaturalLanguage(String)`, `Empty`, `CommandTypo { input, suggestion, distance }`

### TerminalMode Enum
`Normal` (default), `AwaitingCommandApproval` (shell confirmations like `rm -i`), `AwaitingLLMApproval` (LLM command execution), `AwaitingLLMQuestion` (LLM clarification), `AwaitingInput` (multiline heredoc).

### ClassifierContext (Dependency Injection)
Provides `Arc<RwLock<CommandCache>>`, `Arc<CompiledPatterns>`, and language patterns to handlers. Enables testability and avoids global state.

## Performance Targets

| Operation | Target |
|-----------|--------|
| Average classification | <100μs |
| Known command (cache hit) | <1μs |
| PATH lookup (cache miss) | 1-5ms |
| Job polling interval | 250ms |

Run `cargo bench scan_` to verify.

## Claude Code Agents

Agents in `.claude/agents/` are invoked automatically when appropriate:

| Agent | Purpose |
|-------|---------|
| `rust-clippy-enforcer` | Run clippy and fix warnings (before commits) |
| `rust-code-reviewer` | Code review for best practices |
| `code-metrics-analyzer` | LOC, complexity metrics |
| `docs-updater` | Update CLAUDE.md/README.md |
| `git-committer` | Create commits (no emojis, no Co-Author) |

## Platform Notes

**Windows**: Filter `KeyEventKind::Press` only in `events.rs`. Use `cmd /C` for shell execution. Interactive commands not supported.
