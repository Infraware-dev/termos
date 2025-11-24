# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It intelligently routes user input to either shell command execution or an LLM backend for natural language queries.

**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Status**: M1 Complete, Production-Ready (224 tests, 0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant)
**Target Users**: DevOps engineers working with cloud environments (AWS/Azure)

**Prerequisites** (Linux): `sudo apt install -y pkg-config libssl-dev`

## Commands

```bash
# Build and Run
cargo build                          # Build
cargo build --release                # Release build
cargo run                            # Run application

# Testing
cargo test                           # All tests (224 tests)
cargo test --test classifier_tests   # SCAN algorithm tests
cargo test --test executor_tests     # Executor tests
cargo test -- --nocapture            # Tests with output

# Benchmarking
cargo bench                          # All benchmarks
cargo bench scan_                    # SCAN benchmarks only

# Development (run before commits)
cargo fmt                            # Format code
cargo clippy                         # Lint (warnings = errors in CI)
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info  # Coverage
```

## Architecture

### Core Flow
```
User Input → Alias Expansion → InputClassifier → [Command Path | Natural Language Path]
              (if matches)    (10-handler chain)     ↓                    ↓
                           incl. History Expansion  CommandExecutor      LLMClient
                                                         ↓                    ↓
                                                    Shell Output      ResponseRenderer
```

### SCAN Algorithm (Shell-Command And Natural-language)

10-handler Chain of Responsibility executing in strict order (<100μs average):

| # | Handler | Purpose | Performance |
|---|---------|---------|-------------|
| 1 | EmptyInputHandler | Fast path for empty/whitespace | <1μs |
| 2 | HistoryExpansionHandler | `!!`, `!$`, `!^`, `!*` expansion | ~1-5μs |
| 3 | ApplicationBuiltinHandler | App builtins (clear, reload-aliases, reload-commands) | <1μs |
| 4 | ShellBuiltinHandler | 45+ builtins (., :, [, [[, export) | <1μs |
| 5 | PathCommandHandler | ./script.sh, /usr/bin/cmd | ~10μs |
| 6 | KnownCommandHandler | 60+ DevOps commands + PATH cache | <1μs hit |
| 7 | CommandSyntaxHandler | Language-agnostic: flags, pipes, redirects | ~10μs |
| 8 | TypoDetectionHandler | Levenshtein ≤2 ("dokcer" → "docker") | ~100μs |
| 9 | NaturalLanguageHandler | Language-agnostic heuristics (universal patterns) | ~0.5μs |
| 10 | DefaultHandler | Fallback to LLM | <1μs |

**Key optimizations**: Precompiled RegexSet via `once_cell::Lazy`, thread-safe `RwLock<CommandCache>` with poisoning recovery, fast paths first.

### Module Structure

**`terminal/`** - TUI rendering and state
- `tui.rs`: ratatui rendering, suspend/resume for interactive commands
- `state.rs`: Terminal state composition
- `buffers.rs`: SRP-compliant buffers (OutputBuffer, InputBuffer, CommandHistory)
- `events.rs`: Keyboard event handling (Windows: filter KeyEventKind::Press only)
- `splash.rs`: Animated splash screen with particle assembly effect (5s duration, skippable)

**`input/`** - SCAN Algorithm
- `classifier.rs`: InputClassifier coordinating handler chain + alias expansion
- `handler.rs`: 10-handler Chain of Responsibility implementation
- `history_expansion.rs`: Bash-style `!!`, `!$`, `!^`, `!*` with Arc<RwLock>
- `application_builtins.rs`: App builtin commands (clear, reload-aliases, reload-commands)
- `shell_builtins.rs`: 45+ builtins with `ShellBuiltinInfo` metadata
- `known_commands.rs`: Single source of truth for 60+ DevOps commands
- `patterns.rs`: Precompiled RegexSet patterns
- `discovery.rs`: PATH-aware CommandCache + alias loading/expansion
- `typo_detection.rs`: Levenshtein distance with `strsim`
- `parser.rs`: Shell parsing with `shell-words`

**`executor/`** - Command execution
- `command.rs`: Async execution + interactive command support (28 supported, 31 blocked)
- `package_manager.rs`: Strategy pattern for 7 package managers
- `install.rs`: Auto-install workflow
- `completion.rs`: Tab completion

**`orchestrators/`** - Workflow coordination (SRP)
- `command.rs`: Command execution + auto-install prompts
- `natural_language.rs`: LLM queries + response rendering
- `tab_completion.rs`: Tab completion workflow

**`llm/`** - LLM integration
- `client.rs`: MockLLMClient (testing), HttpLLMClient (production)
- `renderer.rs`: Markdown with syntax highlighting

### Design Patterns
- **Chain of Responsibility**: Input classification (`handler.rs`)
- **Strategy Pattern**: Package managers (`package_manager.rs`)
- **Builder Pattern**: Terminal construction (`main.rs`)
- **SRP**: Orchestrators, buffer components

## Development Guidelines

### Adding New Commands
1. Add to `known_commands.rs` (single source of truth for KnownCommandHandler + TypoDetectionHandler)
2. Commands auto-verified against PATH and cached

### Adding New Handlers
1. Implement `InputHandler` trait in `handler.rs`
2. Add to chain in `InputClassifier::new()` - ORDER MATTERS (fast paths first)
3. Use precompiled patterns from `patterns.rs` - NEVER compile regex in handlers
4. Run `cargo bench` to verify no performance regression

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
- `clear` - Clear terminal output buffer
- `reload-aliases` - Reload aliases from system/user config files
- `reload-commands` - Clear command cache (use after installing new commands)

These commands are recognized early in the classification chain to prevent misclassification as natural language.

### Interactive Commands
- **28 supported** (TUI suspends): vim, nvim, nano, emacs, less, more, man, top, htop, sudo, watch, mc, ranger, etc.
- **31 blocked** (helpful error): ssh, tmux, screen, python, mysql, gdb, etc.
- Unix/Linux/macOS only (Windows returns error)
- Implementation: `TerminalUI::suspend()` → command runs → `TerminalUI::resume()`
- Panic-safe via RAII `TuiGuard`

### Shell Builtins
- 45+ recognized without PATH verification (., :, [, [[, export, eval, exec, etc.)
- Executed via `sh -c` (Unix) or `cmd /C` (Windows)
- `ShellBuiltinInfo` provides metadata: `requires_shell`, `unix_only`

### Error Handling
- Use `anyhow::Result` for all errors
- Display user-friendly messages in TUI, don't crash on failures

## Constraints

### CI/CD
- `cargo fmt --all --check` must pass
- `cargo clippy --all-targets --all-features -- -D warnings` must pass
- 75% test coverage minimum
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

**Microsoft Pragmatic Rust Guidelines Compliance**:

This project adheres to Microsoft's enterprise-scale Rust best practices from https://microsoft.github.io/rust-guidelines/. Key highlights:

1. **Debug Trait on Public Types**:
   - All public types implement `Debug` trait
   - Complex types have custom implementations: `InfrawareTerminal`, `CommandCache`, `CompiledPatterns`, `ClassifierChain`, `KnownCommandHandler`
   - 9 marker structs derive Debug
   - Sensitive data is protected from exposure in Debug output

2. **Lint Configuration**:
   - Enabled compiler lints: `missing_debug_implementations`, `redundant_imports`, `redundant_lifetimes`, `unsafe_op_in_unsafe_fn`, `unused_lifetimes`, `ambiguous_negative_literals`, `trivial_numeric_casts`
   - Enabled Clippy lints: `all` set to warn, selected restriction lints, `pedantic` (too strict for M1)
   - **Zero clippy warnings** - all 224 tests pass cleanly

3. **Lint Overrides**:
   - Replaced all `#[allow]` attributes with `#[expect]` (31 instances total)
   - `#[expect]` generates compiler error if the underlying issue is fixed, preventing lint accumulation
   - Exception: Generated code/macros use `#[allow]` where appropriate

4. **Static Verification**:
   - `cargo fmt --all --check` enforced in CI/CD
   - `cargo clippy --all-targets --all-features -- -D warnings` enforced in CI/CD
   - `rustfmt` for consistent code formatting
   - All 224 tests passing, 0 compiler warnings

**Rationale**: These guidelines ensure code is readable, maintainable, and safe for production use. They catch bugs early (missing Debug), prevent outdated lint suppression, and establish consistent patterns across the codebase.

See `.claude/skills/microsoft-rust-guidelines.md` for detailed guidelines and best practices.

### M1 Scope Limitations (Deferred to M2/M3)
- LLM backend: HttpLLMClient exists but needs real endpoint/auth
- Auto-install: Framework prompts but doesn't execute
- Tab completion: Basic only, no bash/zsh integration
- History: Session-only, not persisted to disk
- Config: Hardcoded defaults, no config file
- Markdown: Basic rendering only, no tables/images
- Cache TTL: No automatic invalidation (use `reload-commands` after installing new commands)
- **Hardcoded English Fallback Words**:
  - `typo_detection.rs:100-103` contains `NL_SINGLE_WORDS` list with 15 hardcoded English words
  - `patterns.rs:60-68` contains regex patterns for English question words/articles
  - These patterns should be replaced with language-agnostic heuristics like `NaturalLanguageHandler` (commit cc6b784)
  - Issue: Single-word multilingual inputs (e.g., "cosa", "como") may be incorrectly classified as command typos

## Common Patterns

### Adding a TerminalEvent
1. Add variant to `TerminalEvent` in `events.rs`
2. Handle in `EventHandler::poll_event()`
3. Implement in `InfrawareTerminal::handle_event()` in `main.rs`

### Adding a Package Manager
1. Implement `PackageManager` trait in `package_manager.rs`
2. Add to `PackageInstaller::detect_package_manager()` in `install.rs`

### InputType Enum
- `Command { command, args, original_input }` - shell operators in `original_input`
- `NaturalLanguage(String)` - sent to LLM
- `Empty` - ignored
- `CommandTypo { input, suggestion, distance }` - shows suggestion

## Performance Targets

| Operation | Target |
|-----------|--------|
| Average classification | <100μs |
| Known command (cache hit) | <1μs |
| Typo detection | <100μs |
| Natural language | <5μs |
| PATH lookup (cache miss) | 1-5ms |

Run `cargo bench scan_` to verify.

## Windows Notes

- Filter `KeyEventKind::Press` only in `events.rs` (crossterm generates Press/Repeat/Release)
- Shell execution: `cmd /C` instead of `sh -c`
- Interactive commands not supported (POSIX limitation)
