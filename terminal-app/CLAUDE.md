# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It routes user input to either shell execution or an LLM backend.

**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Status**: M1 Complete + Backend Integration in Progress (0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

**Environment Variables**: `INFRAWARE_BACKEND_URL` (backend endpoint), `BACKEND_API_KEY` (LLM auth)

## Commands

```bash
# Build and Run
cargo build                          # Debug build
cargo build --release                # Release build
cargo run                            # Run application
cargo check                          # Fast type check (no codegen)

# Testing
cargo test                           # All tests
cargo test --test classifier_tests   # SCAN algorithm tests
cargo test --test executor_tests     # Executor tests
cargo test --test integration_tests  # Integration tests
cargo test test_name                 # Single test by name
cargo test -- --nocapture            # Show output during tests
cargo test -- --show-output          # Show println! for passing tests

# Benchmarking (benches/scan_benchmark.rs)
cargo bench                          # All benchmarks
cargo bench scan_                    # SCAN benchmarks only
cargo bench scan_individual_handlers # Individual handler isolation benchmarks
cargo bench scan_full_classification # Full pipeline benchmarks

# Pre-commit (required)
cargo fmt && cargo clippy            # Format + lint (CI enforces both)

# Coverage (requires: cargo install cargo-llvm-cov)
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

## Architecture

```
User Input → Alias Expansion → InputClassifier → [Command Path | Natural Language Path]
              (if matches)    (11-handler chain)     ↓                    ↓
                           incl. History Expansion  CommandExecutor      LLMClient
                                                         ↓                    ↓
                                                    Shell Output      ResponseRenderer
```

### SCAN Algorithm (Shell-Command And Natural-language)

Chain of Responsibility with 11 handlers executing in strict order (<100μs average):

| # | Handler | Purpose |
|---|---------|---------|
| 1 | EmptyInputHandler | Fast path for empty/whitespace |
| 2 | HistoryExpansionHandler | `!!`, `!$`, `!^`, `!*` expansion |
| 3 | ApplicationBuiltinHandler | App builtins (clear, exit, jobs, reload-aliases, reload-commands, auth-status, history) |
| 4 | ShellBuiltinHandler | 45+ builtins (., :, [, [[, export, eval, exec) |
| 5 | PathCommandHandler | ./script.sh, /usr/bin/cmd, background suffix detection |
| 6 | KnownCommandHandler | 60+ DevOps commands + PATH cache |
| 7 | PathDiscoveryHandler | Auto-discover newly installed commands |
| 8 | CommandSyntaxHandler | Language-agnostic: flags, pipes, redirects, glob patterns |
| 9 | TypoDetectionHandler | Levenshtein ≤2 ("dokcer" → "docker"), disabled by default |
| 10 | NaturalLanguageHandler | Language-agnostic heuristics (universal patterns) |
| 11 | DefaultHandler | Fallback to LLM |

### Quick Reference: Where to Find X

| Task | Location |
|------|----------|
| Add known command | `src/input/known_commands.rs` |
| Add app builtin | `src/input/application_builtins.rs` |
| Add shell builtin | `src/input/shell_builtins.rs` |
| Add keyboard shortcut | `src/terminal/events.rs` → `EventHandler::map_key_event()` |
| Add terminal event | `src/terminal/events.rs` → `TerminalEvent` enum |
| Modify TUI rendering | `src/terminal/tui.rs` |
| Add package manager | `src/executor/package_manager.rs` |
| Language patterns | `config/language.toml` |
| Precompiled regex | `src/input/patterns.rs` |

### Key Modules

| Directory | Purpose |
|-----------|---------|
| `terminal/` | TUI: `tui.rs` (suspend/resume), `buffers.rs` (SRP), `events.rs` (keyboard) |
| `input/` | SCAN: `classifier.rs` (coordinator), `handler.rs` (chain), `patterns.rs` (regex) |
| `executor/` | Execution: `command.rs` (async), `job_manager.rs` (background `&`) |
| `orchestrators/` | Workflows: `command.rs`, `natural_language.rs`, `tab_completion.rs` |
| `llm/` | LLM: `client.rs` (Mock/HTTP with HITL), `renderer.rs` (syntax highlighting) |
| `auth/` | Auth: `authenticator.rs`, `config.rs`, `models.rs` |

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

### History Expansion
`!!` (previous cmd), `!$` (last arg), `!^` (first arg), `!*` (all args). Thread-safe via `Arc<RwLock<Vec<String>>>`. Uses get-second-to-last semantics (current input already in history when classified).

### Aliases
System files loaded first, then user files (`~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`). Single-level expansion, O(1) lookup. `is_safe_alias()` rejects dangerous patterns. Runtime reload: `reload-aliases`.

### Interactive Commands
28 commands suspend TUI (vim, nano, less, etc.), 30 blocked with suggestions (ssh, tmux, python REPL, yes). Cloud CLI auth commands (gcloud auth, az login, aws sso, gh auth, etc.) blocked as they open browsers. Implementation: `TerminalUI::suspend()` → run → `resume()` with RAII `TuiGuard` for panic safety. Unix only.

### Background Processes
`&` suffix → `JobManager` with `Arc<RwLock>`. 250ms polling interval. Lock poisoning triggers fail-fast per Microsoft guidelines.

### Glob Pattern Expansion
Commands with glob patterns (`*`, `?`, `[...]`, `{...}`) execute through shell for proper expansion. Detected via `has_glob_patterns()` helper in command arguments. Examples: `rm -rf file*`, `ls *.txt`, `echo file{1..3}`. Execution uses `sh -c` to ensure proper shell expansion of wildcards and brace patterns.

### LLM Integration (HITL)
`HttpLLMClient` with SSE streaming. `LLMQueryResult` enum: `Complete`, `CommandApproval`, `Question`. Resume via `resume_run()` or `resume_with_answer()`.

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
- **NO Co-Authored-By** in commit messages
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

### M1 Scope Limitations (Deferred to M2/M3)
- Auto-install: Framework prompts but doesn't execute
- Tab completion: Basic only, no bash/zsh integration
- History: Session-only, not persisted to disk (accessible via `history` command)
- Markdown: Basic rendering only, no tables/images
- Cache TTL: No automatic invalidation (use `reload-commands` after installing new commands)
- Interactive subcommands: Cloud CLI auth commands that open browsers are blocked (gcloud auth, az login, aws sso, gh auth, firebase login, heroku login, netlify login, vercel login)

## Common Patterns

### Adding a TerminalEvent
1. Add variant to `TerminalEvent` in `events.rs`
2. Handle in `EventHandler::poll_event()`
3. Implement in `InfrawareTerminal::handle_event()` in `main.rs`

### InputType Enum
`Command { command, args, original_input }`, `NaturalLanguage(String)`, `Empty`, `CommandTypo { input, suggestion, distance }`

### ClassifierContext (Dependency Injection)
Provides `Arc<RwLock<CommandCache>>`, `Arc<CompiledPatterns>`, and language patterns to handlers. Enables testability and avoids global state.

## Performance Targets

| Operation | Target |
|-----------|--------|
| Average classification | <100μs |
| Known command (cache hit) | <1μs |
| Typo detection | <100μs (disabled by default) |
| Natural language | <5μs |
| PATH lookup (cache miss) | 1-5ms |
| Background job check (read path) | <1μs (no jobs) |
| Job polling interval | 250ms (balances responsiveness vs lock contention) |

Run `cargo bench scan_` to verify. Use `cargo bench scan_individual_handlers` to measure each handler in isolation and identify bottlenecks.

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
