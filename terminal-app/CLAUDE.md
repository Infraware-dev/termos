# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It's NOT a traditional terminal emulator - it intelligently routes user input to either shell command execution or an LLM backend for natural language queries.

**Current Status**: M1 (Month 1) - Terminal Core MVP
**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Target Users**: DevOps engineers working with cloud environments (AWS/Azure)

**Prerequisites**:
- Linux systems require OpenSSL development libraries: `sudo apt update && sudo apt install -y pkg-config libssl-dev`
- Coverage reporting requires `cargo-llvm-cov`: Install with `cargo install cargo-llvm-cov`

## Commands

### Build and Run
```bash
# Build the project
cargo build

# Build release version
cargo build --release

# Run the application
cargo run

# Run with cargo watch for development
cargo watch -x run
```

### Benchmarking
```bash
# Run performance benchmarks
cargo bench

# Run specific benchmark
cargo bench scan_
```

### Testing
```bash
# Run all tests
cargo test

# Run specific test file
cargo test --test classifier_tests
cargo test --test executor_tests
cargo test --test integration_tests

# Run tests with output
cargo test -- --nocapture

# Run tests for a specific module
cargo test classifier
cargo test executor
```

### Development
```bash
# Check code without building
cargo check

# Format code
cargo fmt

# Run linter (run before commits)
cargo clippy

# Fix clippy warnings automatically where possible
cargo clippy --fix

# Run code coverage
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info

# Clean build artifacts
cargo clean
```

## Architecture

### Core Flow
```
User Input → Alias Expansion → InputClassifier → [Command Path | Natural Language Path]
              (if matches)    (9-handler chain)      ↓                           ↓
                           incl. History Expansion  CommandExecutor             LLMClient
                                   ↓                           ↓
                              Shell Output              ResponseRenderer
```

### Module Structure

**`terminal/`** - TUI rendering and state management
- `tui.rs`: ratatui rendering logic
- `state.rs`: Terminal state composition with buffer components
- `buffers.rs`: **SRP-compliant buffer components** (OutputBuffer, InputBuffer, CommandHistory)
- `events.rs`: Keyboard event handling

**`input/`** - Input classification and parsing (**SCAN Algorithm** - Shell-Command And Natural-language)
- `classifier.rs`: Main InputClassifier coordinating the 9-handler chain with **alias expansion** and **history expansion** support
- `handler.rs`: **Chain of Responsibility** implementation with 9 handlers:
  1. EmptyInputHandler - Fast path for empty/whitespace input
  2. HistoryExpansionHandler - Bash-style history expansions (!!,  !$, !^, !*)
  3. ShellBuiltinHandler - Shell builtins (45+) without PATH verification (., :, [, [[, source, export, etc.)
  4. PathCommandHandler - Executable paths (./script.sh, /usr/bin/cmd) with platform-specific checks
  5. KnownCommandHandler - DevOps commands whitelist (60+) with PATH existence verification
  6. CommandSyntaxHandler - Detects command syntax (flags, pipes, redirects, env vars, subshells)
  7. TypoDetectionHandler - Levenshtein distance ≤2 for typo detection (prevents LLM false positives)
  8. NaturalLanguageHandler - English patterns with precompiled regex (multilingual delegated to LLM)
  9. DefaultHandler - Fallback to natural language (guarantees a result)
- `history_expansion.rs`: **Bash-style history expansion** (!!,  !$, !^, !*) with Arc<RwLock> history sharing
  - `!!` - Expand to entire previous command
  - `!$` - Expand to last argument (or command itself if no args, Bash-compatible)
  - `!^` - Expand to first argument (fails if no args)
  - `!*` - Expand to all arguments (fails if no args)
  - Supports multiple expansions in one input (e.g., `printf '%s' !^ !$`)
  - Preserves shell operators (pipes, redirects) in expanded output
  - Thread-safe history access via Arc<RwLock<Vec<String>>>
- `shell_builtins.rs`: **Shell builtin recognition** without PATH verification (., :, [, [[, source, export, etc.)
  - Handles 45+ builtins: punctuation (`.`, `:`, `[`, `[[`), evaluation (eval, exec), variables (export, unset, set)
  - Execution via `sh -c` for proper builtin interpretation
  - Performance: <1μs handler overhead
- `known_commands.rs`: **Single source of truth** for 60+ DevOps commands (used by both KnownCommandHandler and TypoDetectionHandler)
- `patterns.rs`: **Precompiled RegexSet patterns** using `once_cell::Lazy` (10-100x faster)
- `discovery.rs`: **PATH-aware command discovery** with thread-safe `RwLock<CommandCache>` + **poisoning recovery** + **alias loading and expansion**
  - Loads system aliases from `/etc/bash.bashrc`, `/etc/bashrc`, `/etc/profile`, `/etc/profile.d/*.sh`
  - Loads user aliases from `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`
  - User aliases override system aliases (priority ordering)
  - Alias expansion: O(1) HashMap lookup, single-level expansion like Bash
  - Security: Validates aliases against dangerous patterns (rm -rf /, mkfs, dd, fork bombs, etc.)
- `typo_detection.rs`: **Levenshtein distance** typo detection with `strsim` crate
- `parser.rs`: Shell command parsing with `shell-words` crate (handles quotes, escapes)

**`executor/`** - Command execution (uses Strategy pattern)
- `command.rs`: Async command execution with stdout/stderr capture + **43 interactive commands blocklist**
  - Blocks: vim, top, python REPL, ssh, tmux, less, man, and 36+ other TTY-required commands
  - User-friendly error messages with command-specific alternatives
- `install.rs`: Auto-install workflow
- `package_manager.rs`: **Strategy pattern** for package managers (apt, yum, dnf, pacman, brew, choco, winget)
- `completion.rs`: Tab completion for commands and file paths

**`orchestrators/`** - Workflow coordination (uses Single Responsibility Principle)
- `command.rs`: CommandOrchestrator handles command execution workflow
- `natural_language.rs`: NaturalLanguageOrchestrator handles LLM query workflow
- `tab_completion.rs`: TabCompletionHandler handles tab completion workflow

**`llm/`** - LLM integration
- `client.rs`: LLM API client (MockLLMClient for testing, HttpLLMClient for production)
- `renderer.rs`: Markdown response formatting with syntax highlighting

**`utils/`** - Shared utilities
- `ansi.rs`: ANSI color utilities
- `message.rs`: Message formatting helpers

### Key Design Decisions

1. **Design Patterns Used**:
   - **Chain of Responsibility**: Input classification (`input/handler.rs`)
   - **Strategy Pattern**: Package managers (`executor/package_manager.rs`)
   - **Builder Pattern**: Terminal construction (`main.rs` InfrawareTerminalBuilder)
   - **Single Responsibility Principle**: Orchestrators, buffer components

2. **SCAN Algorithm** (Shell-Command And Natural-language): Production-ready input classification with alias expansion + Chain of Responsibility with 9 optimized handlers executing in strict order (<100μs average):

   **Pre-classification**: Alias expansion
   - Extract first word from input
   - Check if it's a user-defined alias (from `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`, system files)
   - If alias found: expand and re-classify expanded input
   - If not alias: proceed to handler chain

   **Handler chain**:
   1. **EmptyInputHandler**: Fast path for empty/whitespace input (<1μs)
   2. **HistoryExpansionHandler**: Bash-style history expansion (!!,  !$, !^, !*) with Arc<RwLock> history sharing (~1-5μs)
   3. **ShellBuiltinHandler**: Shell builtins (45+) without PATH verification - punctuation (`.`, `:`, `[`, `[[`), evaluation (eval, exec), variables (export, unset, set), I/O (echo, printf), job control (jobs, fg, bg) (<1μs)
   4. **PathCommandHandler**: Executable paths with platform-specific checks - Unix: executable bit check, Windows: .exe/.bat/.cmd extensions (~10μs)
   5. **KnownCommandHandler**: Whitelist of 60+ DevOps commands + cached PATH verification (<1μs cache hit, 1-5ms cache miss)
   6. **CommandSyntaxHandler**: Shell syntax detection - flags (--/-), pipes (|), redirects (>/</>>), logical operators (&&/||), env vars ($VAR), subshells ($()/ backticks) (~10μs)
   7. **TypoDetectionHandler**: Levenshtein distance ≤2 typo detection with `strsim` crate - prevents expensive LLM calls for "dokcer" → "docker" (~100μs)
   8. **NaturalLanguageHandler**: English-only patterns (question words, articles, polite phrases) using precompiled regex - delegates multilingual to LLM (~5μs)
   9. **DefaultHandler**: Fallback to natural language - guarantees result, never panics (<1μs)

   **Performance Optimizations** (see `benches/scan_benchmark.rs`):
   - **Precompiled RegexSet**: `once_cell::Lazy<CompiledPatterns>` compiles patterns once at startup (10-100x speedup)
   - **Thread-safe cache**: `RwLock<CommandCache>` for PATH lookups (99% read-heavy workload, <1μs cache hit)
   - **Handler ordering**: Fast paths first (70% of inputs hit KnownCommandHandler cache)
   - **Zero-cost abstractions**: Static dispatch for patterns, minimal allocations

   **Design Rationale**: English-first fast path (70-80% of queries) with LLM fallback for universal language support (100+ languages). Non-English queries pass through to DefaultHandler and reach LLM with negligible overhead (~1μs vs 100-500ms LLM latency).

3. **Async Execution**: Uses tokio for non-blocking command execution to keep TUI responsive

4. **Cross-Platform Package Management**: Strategy pattern supports 7 package managers:
   - Linux: apt-get, yum, dnf, pacman
   - macOS: brew (highest priority)
   - Windows: choco, winget (winget preferred over choco)

5. **M1 Rendering Limits**: Basic markdown only - code blocks with syntax highlighting (rust, python, bash, json), simple inline formatting. Tables, images, and complex markdown deferred to M2/M3.

## Important Constraints

### CI/CD
- GitHub Actions workflow runs on all PRs and pushes to main
- **Format check**: `cargo fmt --all --check` must pass
- **Clippy**: `cargo clippy --all-targets --all-features -- -D warnings` must pass (warnings treated as errors)
- **Test coverage**: Minimum 75% coverage threshold enforced
- **Multi-platform builds**: Tests run on Ubuntu, Windows, and macOS

### Git Commits
- **NEVER include Co-Authored-By in commit messages** (user preference)
- **ALWAYS run `cargo fmt` before committing** (user preference)
- Keep commit descriptions brief and concise
- Follow repository's existing commit message style (check git log)

### Scope Limitations (M1 Only)
DO NOT implement these yet (deferred to M2/M3):
- Advanced markdown rendering (tables, images)
- Full bash/zsh completion integration
- Multi-shell support (Zsh, Fish)
- Telemetry and analytics
- Performance optimization
- Complex credential management

### Testing Requirements
- All new utilities must have unit tests
- Input classifier changes require comprehensive test coverage
- Integration tests for command execution flow
- Use `tokio-test` for async test utilities

## Development Guidelines

### Working with History Expansion

**History Expansion Support**:
- Bash-style history expansion patterns: `!!`, `!$`, `!^`, `!*`
- Implemented in `src/input/history_expansion.rs` (405 lines, 16 unit tests)
- Requires Arc<RwLock<Vec<String>>> reference to command history
- Get-second-to-last semantics: Current input already in history when classified

**Supported Patterns**:
- `!!` - Expand to entire previous command
- `!$` - Expand to last argument (Bash-compatible: expands to command itself if no args)
- `!^` - Expand to first argument (fails if command has no args)
- `!*` - Expand to all arguments (fails if command has no args)
- Multiple expansions in one input work correctly (e.g., `printf '%s %s' !^ !$`)

**Integration with Classifier**:
1. HistoryExpansionHandler positioned at #2 in chain (after EmptyInputHandler, before ShellBuiltinHandler)
2. Set history via `InputClassifier::with_history(Arc<RwLock<Vec<String>>>)`
3. History synced after `submit_input()` in main.rs
4. Thread-safe via Arc<RwLock> with poisoning recovery

**Bug Fixes Completed**:
- **Bug 1 (Commit 787a96f)**: `!!` was being blocked by PATH check in orchestrator - Fixed: Added `is_history_expansion` check to skip PATH verification
- **Bug 2 (Commit 6d81b05)**: `!!` returned current input instead of previous command - Fixed: Modified get_last_command() to return second-to-last entry (history.len() - 2)
- **Bug 3 (Commit c6932da)**: `!$` failed when command had no arguments - Fixed: Made expand_bang_dollar() return command itself when args is empty (Bash-compatible)

**Performance**:
- History lookup: ~1-5μs (Arc<RwLock> read lock overhead)
- Pattern detection: <1μs (simple string contains checks)
- Command parsing: 1-10μs (via CommandParser)
- Total: <20μs for average history expansion

---

### Working with Aliases

**Alias Loading**:
1. Aliases are loaded at startup via `CommandCache::load_system_aliases()` in `main.rs` using `tokio::spawn_blocking`
2. System aliases loaded from: `/etc/bash.bashrc`, `/etc/bashrc`, `/etc/profile`, `/etc/profile.d/*.sh`
3. User aliases loaded from: `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`
4. User aliases override system aliases (priority ordering in `load_system_aliases()`)
5. Runtime reload via `reload-aliases` built-in command

**Alias Expansion**:
- Single-level expansion only (like Bash) - recursive/chained aliases require re-classification
- O(1) HashMap lookup in `CommandCache::expand_alias()`
- Expansion happens in `InputClassifier::classify()` before handler chain
- Preserves arguments after expansion (e.g., `ll` → `ls -la`, with original args appended)

**Built-in Commands**:
- `reload-aliases`: Reloads all aliases from system and user config files using `spawn_blocking`
- Accessible via `CommandOrchestrator::handle_reload_aliases()` in `src/orchestrators/command.rs`

**Security**:
- `is_safe_alias()` validation rejects dangerous patterns: `rm -rf /`, `mkfs`, `dd if=/dev/zero`, fork bombs, etc.
- Safe parsing with proper quote handling (single quotes, double quotes, escaped spaces)
- Validation occurs during `parse_aliases()` - dangerous aliases are silently rejected with warning

### Working with SCAN Algorithm

The **SCAN Algorithm** (Shell-Command And Natural-language) is the core input classification system. When modifying:

**Adding Commands**:
1. Add to `KnownCommandHandler::default_known_commands()` in `input/handler.rs`
2. Commands are automatically verified against PATH (cached for performance)
3. Add test cases to verify classification behavior
4. Consider auto-install support via package managers

**Adding Handlers**:
1. Implement the `InputHandler` trait in `input/handler.rs`
2. Add to the chain in `InputClassifier::new()` in the correct order
3. Order matters: fast paths first, expensive operations later
4. Add comprehensive test coverage

**Typo Detection**:
- Levenshtein distance threshold: max_distance = 2
- Only checks first word of input against known commands
- Filters out natural language via `looks_like_command()` heuristic
- Returns `InputType::CommandTypo` with suggestion and distance

**Performance Considerations**:
- **Always use precompiled patterns** in `input/patterns.rs` - NEVER compile regex in handlers
- **Leverage CommandCache** via `discovery.rs` for PATH lookups (thread-safe RwLock, <1μs reads)
- **Handler chain order is critical** - fast paths first, expensive operations last
- **Profile changes** with `cargo bench` - SCAN benchmarks in `benches/scan_benchmark.rs`
- **Target**: Average classification <100μs, known commands <1μs (cache hit)

### Working with LLM Integration
- Two client implementations: `MockLLMClient` (testing) and `HttpLLMClient` (production)
- LLM client is injected via Builder pattern for testability
- Real LLM backend integration pending (endpoint/auth TBD)
- When implementing real client, ensure proper error handling for network timeouts
- LLM workflow handled by `NaturalLanguageOrchestrator` in `orchestrators/natural_language.rs`

### TUI State Management
- Terminal state lives in `TerminalState` struct (in `terminal/state.rs`)
- State is composed of three SRP-compliant buffer components (`terminal/buffers.rs`):
  - `OutputBuffer`: scrollable output with auto-trim (max 10,000 lines)
  - `InputBuffer`: text input with cursor positioning (handles Unicode correctly via character-count, not byte-count)
  - `CommandHistory`: history navigation
- Use `TerminalMode` enum to track current state (Normal, ExecutingCommand, WaitingLLM, PromptingInstall)
- Always render after state changes
- Handle terminal resize events properly

**Unicode Support** (Fixed in recent updates):
- `InputBuffer::set_text()` uses `.chars().count()` for cursor positioning instead of `.len()`
- This ensures correct cursor placement for international users (CJK, emoji, etc.)
- All Unicode characters are handled correctly regardless of byte width

### Working with Orchestrators
- Orchestrators separate workflow logic from the main event loop
- `CommandOrchestrator`: handles command execution + auto-install prompts
- `NaturalLanguageOrchestrator`: handles LLM queries + response rendering
- `TabCompletionHandler`: handles tab completion
- When adding new workflows, create a new orchestrator instead of adding to main loop

### Error Handling
- Use `anyhow::Result` for all application errors (consolidated from previous custom error types)
- Provide user-friendly error messages in TUI output
- Don't crash on command failures - display error and continue
- All error handling uses standard Result type with context via anyhow

## Common Patterns

### Adding a New TerminalEvent
1. Add variant to `TerminalEvent` enum in `terminal/events.rs`
2. Handle event in `EventHandler::poll_event()`
3. Implement handler in `InfrawareTerminal::handle_event()` in `main.rs`
4. Update TUI rendering if needed

### Modifying Input Classification
1. **Add/modify handlers** in `input/handler.rs` - implement `InputHandler` trait
2. **Update chain order** in `InputClassifier::new()` - ORDER MATTERS! Fast paths first
3. **Use precompiled patterns** from `patterns.rs` - NEVER compile regex in handlers
4. **Add comprehensive test cases** in `tests/classifier_tests.rs` and handler tests
5. **Test edge cases**: typos, multilingual input, command-like natural language ("run the tests")
6. **Run benchmarks** with `cargo bench` to verify performance hasn't regressed
7. **Run integration tests** to ensure no classification regression

**Critical Design Constraint**: The classifier uses **English-only patterns** for fast path optimization (70-80% of queries). Multilingual queries (Italian, Spanish, French, German, etc.) are handled by the LLM backend via DefaultHandler fallback. This is by design - LLM provides better accuracy and flexibility than hardcoded regex for 100+ languages.

**InputType Enum** (`input/classifier.rs`):
- `Command { command, args, original_input }` - Shell operators preserved in `original_input`
- `NaturalLanguage(String)` - Sent to LLM (handles all languages)
- `Empty` - Ignored
- `CommandTypo { input, suggestion, distance }` - Shows suggestion to user

### Adding a New Package Manager
1. Create a new struct implementing the `PackageManager` trait in `executor/package_manager.rs`
2. Implement required methods: `name()`, `is_available()`, `install()`, `priority()`
3. Add the manager to `PackageInstaller::detect_package_manager()` in `executor/install.rs`
4. Add test cases for availability check and priority
5. Consider platform-specific behavior (Windows vs Linux vs macOS)

### Adding Syntax Highlighting
1. Update `ResponseRenderer::highlight_code()` in `llm/renderer.rs`
2. Use `syntect` crate with appropriate syntax set
3. Test with code samples in different languages
4. Ensure ANSI escape codes render correctly in TUI

## Code Quality & Production Readiness

### ✅ Code Review Results (Commit 99d87d1)
**Overall Score**: 95/100 - Production Ready
**Status**: M1 Milestone Complete

### ✅ Panic Safety Improvements (Commits ff5f881, 77b393f)
**Score**: 95/100 - All panic points eliminated
**Status**: Zero unsafe indexing, comprehensive edge case testing

1. **Unsafe Indexing Elimination** (High Priority - FIXED)
   - Location: `src/input/handler.rs` (3 occurrences), `src/executor/completion.rs` (1 occurrence)
   - Issues Eliminated:
     - `parts[0]` direct indexing → replaced with `.first().cloned().unwrap_or_default()`
     - `.rfind('/').unwrap()` → replaced with safe `if let Some(idx)` pattern
   - Handlers Fixed:
     - `KnownCommandHandler`: Safe extraction of command from parsed input
     - `CommandSyntaxHandler`: Safe first word detection for syntax checking
     - `PathCommandHandler`: Safe path parsing
   - Completion Fixed: Safe path separator handling in `completion.rs`
   - Impact: Terminal no longer panics on edge cases like empty quotes, malformed input, unclosed quotes

2. **Edge Case Test Coverage** (Medium Priority - ADDED)
   - Location: `tests/classifier_tests.rs` (2 new tests)
   - Tests Added:
     - `test_empty_quotes_no_panic()`: Verifies `""` input is classified safely without panic
     - `test_malformed_input_no_panic()`: Verifies unclosed quotes don't cause panic
   - Benefit: Comprehensive edge case coverage prevents regression

### Critical Issues Resolved
1. **RwLock Poisoning Fix** (High Priority - FIXED)
   - Location: `src/input/discovery.rs`
   - Issue: `.unwrap()` calls on RwLock could crash terminal if thread panics while holding lock
   - Fix: Implemented proper poisoned lock recovery with `poisoned.into_inner()` on all 6 lock acquisitions
   - Impact: Terminal now resilient to thread panics
   - Pattern: `match lock { Ok(l) => l, Err(poisoned) => poisoned.into_inner() }`

2. **Command List Deduplication** (High Priority - FIXED)
   - Location: Created new `src/input/known_commands.rs` module
   - Issue: 60+ DevOps commands hardcoded in two places (KnownCommandHandler + TypoDetectionHandler)
   - Fix: Single source of truth for command list, eliminating 120+ lines of duplicated code
   - Benefit: Consistency across handlers, easier to maintain/add commands

3. **Interactive Commands Blocking** (High Priority - FIXED)
   - Location: `src/executor/command.rs:44-101`
   - Issue: No blocking of interactive commands that require TTY (vim, top, python REPL, etc.)
   - Fix: Added `INTERACTIVE_COMMANDS` blocklist with 43 commands + user-friendly error messages
   - Examples:
     - `top` → "Try 'ps aux' or 'top -b -n 1' for non-interactive output"
     - `vim` → "Try 'cat' to view or edit externally"
     - `python` → "Pass code with -c flag: 'python -c \"code\"'"

4. **Orchestrator Shell Builtin Bug Fix** (High Priority - FIXED)
   - Location: `src/orchestrators/command.rs:59-67`
   - Issue: Shell builtins (`:`, `.`, `export`, `[[`, etc.) were being classified correctly but failing during execution with "Command ':' not found"
   - Root Cause: `CommandOrchestrator` was checking if command exists in PATH BEFORE executor, but shell builtins don't exist in PATH (they're built into shell)
   - Fix: Changed PATH existence check to skip shell builtins:
     - Before: `if original_input.is_none() && !CommandExecutor::command_exists(cmd)`
     - After: `if original_input.is_none() && !ShellBuiltinHandler::requires_shell_execution(cmd) && !CommandExecutor::command_exists(cmd)`
   - Impact: All 45 shell builtins now work end-to-end (`:`, `.`, `source`, `export`, `[[`, `eval`, `exec`, etc.)
   - Pattern: Orchestrator now properly delegates shell builtins to executor which executes them via `sh -c`

### ✅ Dead Code Cleanup and Unicode Fix
**Status**: Improved maintainability - 233 tests passing, 0 clippy warnings, 537 lines removed

**Changes**:
1. **Removed Facade Pattern** (High Priority - REMOVED)
   - Deleted: `src/executor/facade.rs` (294 lines)
   - Rationale: Unnecessary abstraction layer - direct executor access is simpler and clearer
   - Impact: Cleaner codebase, reduced maintenance burden

2. **Consolidated Error Handling** (High Priority - REMOVED)
   - Deleted: `src/utils/errors.rs` (228 lines)
   - Changed: All error handling uses `anyhow::Result` instead of custom `InfraError` type
   - Impact: Simpler error handling, consistency with Rust ecosystem best practices

3. **Unicode Fix for International Users** (High Priority - FIXED)
   - Location: `src/terminal/buffers.rs:228` in `InputBuffer::set_text()`
   - Issue: Cursor positioning used byte count (`.len()`) instead of character count
   - Fix: Changed to `.chars().count()` for proper Unicode support
   - Added: Comprehensive Unicode test `test_set_text_unicode()`
   - Impact: Correct cursor placement for CJK, emoji, and other multi-byte characters

4. **Removed Unused Methods** (~15 methods across multiple files)
   - Eliminated dead code that was no longer referenced
   - Reduced overall SLOC from ~5,850 to 5,494 (356 lines removed in dead code cleanup)

### ✅ Shell Builtin Code Review Fixes (Commit 50c2f0f follow-up)
**Status**: All critical issues resolved - 233 tests passing, 0 clippy warnings

1. **Windows Compatibility Fix** (High Priority - FIXED)
   - Location: `src/executor/command.rs`
   - Issue: Hardcoded `sh -c` for shell execution breaks on Windows
   - Fix: Platform-specific shell detection via `get_platform_shell()`
     - Unix/Linux/macOS: `("sh", "-c")`
     - Windows: `("cmd", "/C")`
   - Added Windows Unix-only builtin check to prevent running Unix builtins on Windows
   - Impact: Cross-platform shell execution now works correctly

2. **Shell Builtin List Deduplication** (High Priority - FIXED)
   - Location: `src/input/shell_builtins.rs` + `src/executor/command.rs`
   - Issue: 45 builtins in handler, only 19 in executor - inconsistent and unmaintainable
   - Fix: Created single source of truth with `ShellBuiltinInfo` metadata structure
     - `requires_shell: bool` - whether builtin MUST run via shell
     - `unix_only: bool` - whether builtin is Unix/Linux-only
   - All code now queries `ShellBuiltinHandler::builtin_info()` for authoritative list
   - Added missing `[` and `test` to executor builtin recognition
   - Benefit: Consistency, maintainability, platform-aware execution

3. **Shell Operator Detection Duplication** (High Priority - FIXED)
   - Location: `src/input/handler.rs` + `src/input/shell_builtins.rs`
   - Issue: Shell operator detection logic duplicated in 3 places with inconsistent implementations
   - Fix: Centralized all detection to `CompiledPatterns::has_shell_operators()`
     - Uses precompiled regex for robust detection
     - Single source of truth eliminates inconsistencies
     - All handlers now use `crate::input::patterns::CompiledPatterns::get()`
   - Impact: Consistent shell operator detection, easier to maintain

## Implementation Status & Known Limitations

### ✅ Completed (Production-Ready)
- **SCAN Algorithm**: All 9 handlers implemented with performance optimizations
  - History expansion support (!!,  !$, !^, !*): Bash-compatible patterns with <20μs average overhead
  - Shell builtin support (45+): Punctuation (`.`, `:`, `[`, `[[`), evaluation (eval, exec), variables (export, unset, set), I/O (echo, printf), job control (jobs, fg, bg)
  - Execution via `sh -c` for proper builtin interpretation
  - Performance: <1μs handler overhead per builtin
- **History Expansion**: Full bash-style history expansion support
  - `!!` - Entire previous command
  - `!$` - Last argument (Bash-compatible: command itself if no args)
  - `!^` - First argument (fails if no args)
  - `!*` - All arguments (fails if no args)
  - Multiple expansions per input supported
  - Preserves shell operators in expanded output
  - Thread-safe via Arc<RwLock<Vec<String>>>
  - 16 comprehensive unit tests, all edge cases covered
- **Alias Support**: System and user alias loading + single-level expansion with security validation
  - Loads from: `/etc/bash.bashrc`, `/etc/bashrc`, `/etc/profile`, `/etc/profile.d/*.sh`, `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`
  - User aliases override system aliases (priority ordering)
  - Built-in `reload-aliases` command for runtime reloading via `spawn_blocking`
  - Security: Validates and rejects dangerous alias patterns (rm -rf /, mkfs, dd, fork bombs, etc.)
  - Performance: O(1) HashMap lookup, <1μs expansion overhead
- **Typo Detection**: Levenshtein distance-based suggestion system
- **Shell Operator Support**: Pipes, redirects, logical operators, subshells
- **Command Caching**: Thread-safe PATH verification with RwLock + poisoning recovery
- **Precompiled Patterns**: Zero runtime regex compilation overhead
- **Cross-Platform**: Windows/macOS/Linux support with platform-specific handlers
- **Benchmarking**: Performance benchmarks in `benches/scan_benchmark.rs`
- **Test Coverage**: 233 tests passing with comprehensive edge case coverage, 0 clippy warnings
- **Unicode Support**: Full character-count based cursor positioning for international users (CJK, emoji, etc.)
- **Interactive Command Blocking**: 43 commands blocked with user-friendly suggestions
- **Known Commands Module**: Single source of truth for 60+ DevOps commands
- **Shell Builtin Support**: 45+ builtins recognized without PATH verification
- **Clean Codebase**: Dead code removed (facade.rs, errors.rs) - 537 lines reduced, 8,292 SLOC

### ⚠️ Known Limitations (Deferred to M2/M3)
- **Auto-install**: Framework exists, prompts user but doesn't execute installation
- **LLM Backend**: `HttpLLMClient` exists but needs real endpoint/auth integration
- **Tab Completion**: Basic file/command completion only - no bash/zsh integration
- **Configuration**: No config file support - uses hardcoded defaults
- **Command History**: Session-only persistence - not saved to disk
- **Advanced Markdown**: Basic rendering only - tables/images deferred to M2/M3
- **Command Cache TTL**: No TTL/invalidation - commands installed during session require restart (manual `reload-aliases` required for new aliases discovered)
- **Alias Cache TTL**: No automatic TTL/invalidation - alias files changed externally require `reload-aliases` command
- **Typo Detection Performance**: O(n) algorithm - could be optimized to O(log n) with BK-tree
- **Regex Pattern Precision**: Some edge cases in multilingual pattern detection

## Windows-Specific Considerations

**Fixed: Double Input Issue** - On Windows, `crossterm` generates multiple events per keystroke (Press, Repeat, Release). This was causing duplicate character input. **Solution implemented**: Filter events to only process `KeyEventKind::Press` in `terminal/events.rs:41`. This ensures each keystroke is processed exactly once.

## Performance Benchmarking

Run benchmarks to verify performance targets:

```bash
# Run all benchmarks
cargo bench

# Run specific SCAN benchmarks
cargo bench scan_

# View benchmark results
open target/criterion/report/index.html  # macOS
xdg-open target/criterion/report/index.html  # Linux
```

**Performance Targets**:
- Average classification: <100μs
- Known command (cache hit): <1μs
- Typo detection: <100μs
- Natural language: <5μs
- PATH lookup (cache miss): 1-5ms (cached for subsequent calls)

## Documentation & References

### Internal Documentation
- **Project Brief**: `infraware_terminal_project_brief.md`
- **SCAN Architecture**: `docs/SCAN_ARCHITECTURE.md` (comprehensive SCAN algorithm reference)
- **Implementation Plan**: `docs/SCAN_IMPLEMENTATION_PLAN.md` (SCAN implementation phases)
- **README**: `README.md` (user-facing documentation)

### External References
- **ratatui**: https://ratatui.rs/ (TUI framework)
- **crossterm**: https://docs.rs/crossterm/latest/crossterm/ (terminal control)
- **tokio**: https://docs.rs/tokio/latest/tokio/ (async runtime)
- **regex**: https://docs.rs/regex/latest/regex/ (pattern matching)
- **which**: https://docs.rs/which/latest/which/ (command discovery)
- **strsim**: https://docs.rs/strsim/latest/strsim/ (Levenshtein distance)