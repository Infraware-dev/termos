# Infraware Terminal

**A hybrid command interpreter with AI assistance for DevOps operations**

Infraware Terminal is a TUI-based terminal application that intelligently routes user input to either shell command execution or an LLM backend for natural language queries. It's designed specifically for DevOps engineers working with cloud environments (AWS/Azure).

## 🎯 Project Status

**Current Milestone:** M1 - Terminal Core MVP (Month 1)
**Version:** 0.1.0
**Tech Stack:** Rust + TUI (ratatui/crossterm)
**Code Quality:** Microsoft Pragmatic Rust Guidelines compliant
**Status:** M1 Complete + Backend Integration in Progress

The codebase is feature-complete for M1 with 224 tests passing and zero clippy warnings. Implementation follows the 4-week timeline and adheres to Microsoft's enterprise-scale Rust guidelines with strict compiler/clippy lints, Debug trait implementations on all public types, and #[expect] for all lint overrides.

## ✨ Features

### Implemented in M1 (Production-Ready)

- ✅ **History Expansion**: Bash-style history expansion (!!,  !$, !^, !*)
  - `!!` - Entire previous command
  - `!$` - Last argument (or command itself if no args, Bash-compatible)
  - `!^` - First argument
  - `!*` - All arguments
  - Multiple expansions per input: `printf '%s %s' !^ !$`
  - Preserves shell operators (pipes, redirects) in expanded output
  - <20μs average overhead, thread-safe
- ✅ **Alias Support**: System and user alias expansion with security validation
  - Loads from system files: `/etc/bash.bashrc`, `/etc/bashrc`, `/etc/profile`, `/etc/profile.d/*.sh`
  - Loads from user files: `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc`
  - User aliases override system aliases
  - O(1) HashMap lookup for performance (<1μs expansion overhead)
  - Built-in `reload-aliases` command for runtime reloading
  - Security: Rejects dangerous patterns (rm -rf /, mkfs, dd, fork bombs, etc.)
- ✅ **SCAN Algorithm**: Advanced input classification with 11-handler chain + alias + history expansion (<100μs avg)
  - Alias expansion before classification (single-level like Bash)
  - Shell builtin support (45+): `.`, `:`, `[`, `[[`, source, export, eval, exec, and more
  - Command recognition with PATH verification and caching
  - Typo detection with Levenshtein distance (e.g., "dokcer" → "docker")
  - Shell operator support (pipes, redirects, logical operators)
  - Glob pattern expansion (`*`, `?`, `[...]`, `{...}`) with shell execution
  - Language-agnostic classification (works for any language)
- ✅ **Command Execution**: Async shell command execution with stdout/stderr capture
- ✅ **Background Processes**: Execute commands in the background with `&` suffix (e.g., `sleep 10 &`)
  - Job tracking with 1-based job IDs and process IDs
  - `jobs` builtin command to list all background jobs
  - Real-time notifications when jobs complete
  - Non-blocking execution with independent job management
- ✅ **Shell Operator Support**: Full support for pipes (`|`), redirects (`>`/`<`), logical operators (`&&`/`||`), subshells
- ✅ **Performance Optimizations**: Precompiled regex patterns, thread-safe command caching, fail-fast lock poisoning recovery, periodic job checking (250ms)
- ✅ **Interactive Commands**: 28 commands with full TUI suspension (vim, nano, less, man, top, htop, sudo, etc.) + 31 blocked commands with helpful alternatives
- ✅ **Auto-Install Framework**: Detect missing commands and prompt for installation (execution deferred to M2)
- ✅ **LLM Integration**: Mock client ready, route natural language queries to AI backend
- ✅ **Syntax Highlighting**: Code blocks with syntax highlighting (Rust, Python, Bash, JSON)
- ✅ **Tab Completion**: Basic command and file path completion
- ✅ **Command History**: Navigate previous commands with arrow keys
- ✅ **Cross-Platform**: Windows, macOS, and Linux support with platform-specific optimizations
- ✅ **Benchmarking Suite**: Performance benchmarks for SCAN algorithm
- ✅ **Unicode Support**: Full Unicode support for international users (CJK, emoji, etc.) with character-count based cursor positioning
- ✅ **Code Quality**: 224 tests passing with comprehensive edge case coverage, 0 clippy warnings, Microsoft Pragmatic Rust Guidelines compliant (Debug on all public types, #[expect] for lint overrides, fail-fast lock poisoning recovery, static verification lints enabled)

### Coming in M2/M3

- Multi-shell support (Zsh, Fish)
- Advanced markdown rendering (tables, images)
- Telemetry and analytics
- Cloud provider integrations (AWS CLI, Azure CLI)
- Plugin system

## 🏗️ Architecture

### SCAN Algorithm (Shell-Command And Natural-language)

The core of Infraware Terminal is the **SCAN algorithm** - a high-performance input classification system using the Chain of Responsibility pattern:

```
User Input → Alias Expansion → InputClassifier (9-Handler Chain)
                (if matches)           ↓
                              ┌────────┼────────┐
                              ↓        ↓        ↓
                          Command    Typo?   Natural Language
                              ↓        ↓        ↓
                          Shell Exec Suggest LLM Backend
```

**11-Handler Chain** (executed in strict order):
1. **EmptyInputHandler** - Fast path for empty input (<1μs)
2. **HistoryExpansionHandler** - Bash-style history expansion: `!!`,  `!$`, `!^`, `!*` (~5μs)
3. **ApplicationBuiltinHandler** - App-specific commands: clear, exit, jobs, history, reload-aliases, reload-commands, auth-status (<1μs)
4. **ShellBuiltinHandler** - Shell builtins without PATH check: `.`, `:`, `[`, `[[`, source, export, eval, etc. (<1μs)
5. **PathCommandHandler** - Executable paths: `./script.sh`, `/usr/bin/cmd`, background processes with `&` (~10μs)
6. **KnownCommandHandler** - 60+ DevOps commands with PATH cache (<1μs hit)
7. **PathDiscoveryHandler** - Auto-discover newly installed commands (~1-5ms)
8. **CommandSyntaxHandler** - Language-agnostic: flags, pipes, redirects, glob patterns (~10μs)
9. **TypoDetectionHandler** - Levenshtein distance ≤2: "dokcer" → "docker" (~100μs, disabled by default)
10. **NaturalLanguageHandler** - Language-agnostic heuristics (universal patterns) (~5μs)
11. **DefaultHandler** - Fallback to LLM (<1μs)

**Key Features**:
- Average classification: <100μs
- Language-agnostic core algorithm (works for any language)
- Precompiled regex patterns (10-100x faster)
- Thread-safe locks with fail-fast poisoning recovery (Microsoft Rust Guidelines M-PANIC-IS-STOP)
- Panic-safe indexing on all array access (no unwrap() on array indices)
- English-first fast path with universal LLM fallback
- Periodic background job checking (250ms interval) to minimize lock contention

See `docs/SCAN_ARCHITECTURE.md` for complete documentation.

## 📂 Project Structure

```
infraware-terminal/
├── Cargo.toml
├── src/
│   ├── main.rs                    # Entry point + event loop
│   ├── lib.rs                     # Library exports
│   ├── terminal/                  # TUI rendering and state
│   │   ├── tui.rs                # ratatui rendering logic
│   │   ├── state.rs              # Terminal state management
│   │   ├── buffers.rs            # Buffer components (SRP) - Unicode-safe
│   │   └── events.rs             # Keyboard event handling
│   ├── input/                     # SCAN Algorithm
│   │   ├── classifier.rs         # InputClassifier coordinator
│   │   ├── handler.rs            # 11-handler Chain of Responsibility
│   │   ├── history_expansion.rs  # Bash-style history expansion (!!, !$, !^, !*)
│   │   ├── shell_builtins.rs     # Shell builtin recognition (., :, [, [[, etc.)
│   │   ├── application_builtins.rs # Application builtins (clear, exit, jobs, etc.)
│   │   ├── patterns.rs           # Precompiled RegexSet patterns
│   │   ├── discovery.rs          # PATH-aware command cache
│   │   ├── known_commands.rs     # Single source of truth for 60+ DevOps commands
│   │   ├── typo_detection.rs     # Levenshtein distance typo detection
│   │   └── parser.rs             # Shell command parsing
│   ├── executor/                  # Command execution
│   │   ├── command.rs            # Async command execution + background processes (&)
│   │   ├── job_manager.rs        # Background job tracking with JobManager
│   │   ├── package_manager.rs    # Strategy pattern (apt, yum, brew, etc.)
│   │   ├── install.rs            # Auto-install workflow
│   │   └── completion.rs         # Tab completion
│   ├── orchestrators/             # Workflow coordination
│   │   ├── command.rs            # Command execution workflow
│   │   ├── natural_language.rs   # LLM query workflow
│   │   └── tab_completion.rs     # Tab completion workflow
│   ├── llm/                       # LLM integration
│   │   ├── client.rs             # Mock & HTTP clients
│   │   └── renderer.rs           # Markdown rendering
│   └── utils/                     # Shared utilities
│       ├── ansi.rs               # ANSI color utilities
│       └── message.rs            # Message formatting
├── tests/                         # Test suites (224 tests)
│   ├── classifier_tests.rs       # SCAN algorithm tests
│   ├── executor_tests.rs         # Command execution tests
│   └── integration_tests.rs      # End-to-end tests
├── benches/                       # Performance benchmarks
│   └── scan_benchmark.rs         # SCAN algorithm benchmarks
└── docs/                          # Documentation
    ├── SCAN_ARCHITECTURE.md      # SCAN algorithm details
    └── SCAN_IMPLEMENTATION_PLAN.md
```

## 🚀 Getting Started

### Prerequisites

- Rust 1.70+ (2021 edition)
- Linux, macOS, or Windows
- Terminal with ANSI color support

#### Linux

On Linux systems, you need to install OpenSSL development dependencies:

```bash
sudo apt update && sudo apt install -y pkg-config libssl-dev
```

### Building

```bash
# Clone the repository
git clone <repository-url>
cd infraware-terminal

# Build the project
cargo build --release

# Run tests
cargo test

# Run the application
cargo run

# Run with debug logging
LOG_LEVEL=debug cargo run

# Run with trace logging (very verbose)
LOG_LEVEL=trace cargo run
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `LOG_LEVEL` | Log level: trace, debug, info, warn, error | `info` |
| `LOG_MAX_SIZE_MB` | Max log file size before rotation | `10` |
| `LOG_MAX_FILES` | Number of rotated log files to keep | `5` |
| `LOG_PATH` | Custom log directory path | Platform-specific |
| `INFRAWARE_BACKEND_URL` | Backend API endpoint | - |
| `BACKEND_API_KEY` | API key for LLM backend | - |

Log files are stored in:
- **Linux**: `~/.local/share/infraware-terminal/logs/`
- **macOS**: `~/Library/Logs/infraware-terminal/`
- **Windows**: `%APPDATA%\infraware-terminal\logs\`

### Usage

Once running, you can:

1. **Execute commands**: Type any shell command (e.g., `ls -la`, `docker ps`)
2. **Use interactive commands**: Full terminal control for editors and pagers (Unix/Linux/macOS):
   - Text editors: `vim file.txt`, `nano config.yml`, `emacs script.rs`
   - Pagers: `less output.log`, `man docker`, `info grep`
   - File managers: `mc`, `ranger`, `nnn`
   - System monitoring: `watch -n 1 'ps aux'`
3. **Run background processes**: Execute commands in the background with `&` suffix:
   - `sleep 10 &` - Run sleep in the background, terminal stays responsive
   - `long-running-task &` - Start any long-running task without blocking
   - `jobs` - List all background jobs with their status and process IDs
   - Auto-notification when jobs complete with exit status
4. **Use history expansion**: Use bash-style history patterns:
   - `!!` - Re-run previous command: `sudo !!`
   - `!$` - Use last argument: `vim !$` (if previous was `cat file.txt`)
   - `!^` - Use first argument: `echo !^` (if previous was `cat file1 file2`)
   - `!*` - Use all arguments: `find !*` (if previous was `ls -la /tmp`)
5. **Use glob patterns**: Commands with `*`, `?`, `[...]`, `{...}` automatically execute through shell:
   - `rm -rf file*` - Remove files matching pattern
   - `ls *.txt` - List all .txt files
   - `echo file{1..3}` - Brace expansion
   - `find /etc/host[s]` - Bracket patterns
6. **Use aliases**: Type user-defined aliases from `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc` (e.g., `ll` → expands to `ls -la`)
7. **Ask questions**: Type natural language queries (e.g., "how do I list files?")
8. **View history**: Type `history` to show all commands, or `history N` to show last N commands
9. **Navigate history**: Use ↑/↓ arrow keys
10. **Scroll output**: Navigate previous command output when it exceeds the visible area
11. **Tab completion**: Press Tab to complete commands/paths
12. **Reload aliases**: Type `reload-aliases` to refresh aliases from config files (useful if editing `.bashrc` during a session)
13. **Reload commands**: Type `reload-commands` to clear the command cache (useful after installing new commands during a session)
14. **Quit**: Press Ctrl+C or type `exit`

#### Keyboard Shortcuts

| Action | Key(s) | Description |
|--------|--------|-------------|
| **Input Navigation** | | |
| Move cursor left | ← | Move left in input line |
| Move cursor right | → | Move right in input line |
| Delete character | Backspace | Delete character before cursor |
| Submit input | Enter | Execute command or query |
| **History Navigation** | | |
| Previous command | ↑ | Navigate to previous command in history |
| Next command | ↓ | Navigate to next command in history |
| **Output Scrolling** | | |
| Scroll up | PageUp | Scroll output up one page |
| Scroll down | PageDown | Scroll output down one page |
| Scroll up (laptop) | Ctrl+↑ | Alternative scroll up for laptops (when Fn+PageUp is inconvenient) |
| Scroll down (laptop) | Ctrl+↓ | Alternative scroll down for laptops (when Fn+PageDown is inconvenient) |
| **Other** | | |
| Tab completion | Tab | Complete command or file path |
| Clear screen | Ctrl+L | Clear terminal output buffer |
| Quit/Cancel | Ctrl+C | Context-aware: cancel operations or clear input |

**Note**: The visual scrollbar appears on the right side of the output area when the content exceeds the visible space, showing your position in the output history.

#### Interactive Commands

Infraware Terminal supports full interactive command execution with complete terminal control. When you run an interactive command, the TUI temporarily suspends (returns to normal terminal), the command runs with full terminal access, and the TUI automatically resumes when you exit. A total of 28 interactive commands are supported.

**Supported Interactive Commands** (28 total):
- **Text Editors** (7): vim, nvim, nano, emacs, pico, ed, vi
- **Pagers** (5): less, more, most, man, info
- **File Managers** (5): mc, ranger, nnn, lf, vifm
- **System Monitors** (4): top, htop, btop, atop
- **Other Monitors** (3): iotop, iftop, nethogs
- **Privilege Escalation** (1): sudo
- **Process Watcher** (1): watch

**Usage Examples**:
```bash
# Text editors - full terminal control
vim file.txt          # Opens vim for editing
nano config.yml       # Opens nano editor
emacs script.py       # Opens emacs editor

# Pagers and documentation - scroll through content
less output.log       # Browse large log files
man docker            # View docker manual pages
info grep             # View grep documentation

# File managers - navigate file system
mc                    # Opens Norton Commander-style file manager
ranger                # Opens ranger file browser
nnn /tmp              # Opens nnn file manager in /tmp

# System monitoring - real-time process/system info
top                   # Real-time system monitor
htop                  # Interactive process viewer
iotop                 # Monitor I/O statistics

# Privilege escalation - requires password entry
sudo apt update       # Run command with sudo (prompts for password)
sudo visudo           # Edit sudoers file safely

# Process monitoring
watch -n 1 'ps aux'   # Monitor processes continuously
```

**Execution Workflow**:
1. Type the interactive command
2. The TUI suspends (temporarily leaves alternate screen, disables raw mode)
3. Command runs with full terminal access (inherits stdin/stdout/stderr)
4. When you exit the command, the TUI automatically resumes
5. Output is not captured (command runs in native terminal, not TUI buffer)

**Platform Support**:
- **Linux, macOS, Unix**: Fully supported with TUI suspension/resumption
- **Windows**: Not supported - returns helpful error message

**Blocked Interactive Commands** (31 total):
Some commands require persistent network or complex TTY sessions that cannot be supported:

```
Network Tools (4):     ssh, telnet, ftp, sftp - Use in separate terminal window
Multiplexers (2):      tmux, screen - Use outside Infraware Terminal
REPLs (5):             python, python3, node, irb, ipython
                       → Pass code with -c flag: python -c "print(1+1)"
Databases (5):         mysql, psql, sqlite3, mongo, redis-cli
                       → Use connection flags or scripts
Debuggers (3):         gdb, lldb, pdb
System Monitors (3):   iotop, iftop, nethogs - Require root, use alt-commands
Admin Tools (2):       passwd, visudo - Use external terminal for safety
Terminal Browsers (3): w3m, lynx, links
```

These commands show helpful error messages with alternatives when you try to run them.

#### Alias Support

Infraware Terminal automatically loads and expands your shell aliases:

**System Aliases** (loaded first):
- `/etc/bash.bashrc` (Debian/Ubuntu)
- `/etc/bashrc` (RedHat/CentOS/Fedora)
- `/etc/profile`
- `/etc/profile.d/*.sh`

**User Aliases** (override system, loaded second):
- `~/.bashrc`
- `~/.bash_aliases`
- `~/.zshrc`

**Examples**:
```
# If your ~/.bashrc contains:
alias ll='ls -la'
alias gs='git status'

# You can use them directly:
ll                    # Expands to: ls -la
gs                    # Expands to: git status
ll -h | grep test     # Expands and preserves arguments
```

**Runtime Reload**:
If you edit your shell config files during a session, use `reload-aliases` to refresh:
```
reload-aliases        # Reloads all aliases from system and user config files
```

**Security**: Dangerous alias patterns are automatically rejected (e.g., `alias rm='rm -rf /'`)

## 🧪 Testing & Benchmarking

### Running Tests

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test classifier_tests
cargo test --test executor_tests
cargo test --test integration_tests

# Run tests for specific module
cargo test classifier
cargo test typo_detection

# Run with output
cargo test -- --nocapture

# Run code coverage (requires cargo-llvm-cov)
cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info
```

### Running Benchmarks

```bash
# Run all benchmarks
cargo bench

# Run SCAN algorithm benchmarks only
cargo bench scan_

# View benchmark results
open target/criterion/report/index.html  # macOS
xdg-open target/criterion/report/index.html  # Linux
```

**Performance Targets**:
- Average SCAN classification: <100μs
- Known command (cache hit): <1μs
- Typo detection: <100μs (disabled by default)
- Natural language: <5μs
- PATH lookup (cache miss): 1-5ms
- Background job check (read path, no jobs): <1μs
- Job polling interval: 250ms (balances responsiveness vs lock contention)

## 📝 Development Status

### M1 Implementation - Complete ✅

**Week 1: TUI Foundation** ✅
- [x] Project setup with complete module structure
- [x] Basic TUI with ratatui
- [x] Keyboard event capture
- [x] Output buffer management
- [x] Terminal state composition (SRP-compliant buffers)

**Week 2-3: SCAN Algorithm Implementation** ✅
- [x] 8-handler Chain of Responsibility implementation
- [x] Precompiled RegexSet patterns (10-100x performance improvement)
- [x] PATH-aware command discovery with thread-safe caching
- [x] Levenshtein distance typo detection
- [x] Shell operator support (pipes, redirects, logical operators)
- [x] Command parser with shell-words integration
- [x] Async command executor with stdout/stderr capture
- [x] Cross-platform package manager support (7 managers)
- [x] Auto-install framework (prompt logic implemented)

**Week 4: Testing & Optimization** ✅
- [x] Comprehensive test suite (224 tests passing with edge case coverage)
- [x] History expansion tests (comprehensive unit tests covering all patterns)
- [x] Panic safety improvements (safe indexing throughout, no unwrap on array access)
- [x] Unicode fix for international users (character-count based cursor positioning)
- [x] Performance benchmarking suite
- [x] Integration tests for end-to-end workflows
- [x] Cross-platform testing (Ubuntu, Windows, macOS)
- [x] CI/CD pipeline (fmt, clippy, coverage ≥75%)
- [x] Documentation (CLAUDE.md, SCAN_ARCHITECTURE.md)
- [x] Zero clippy warnings
- [x] Dead code cleanup (removed facade.rs and errors.rs, 537 lines reduced)

**Code Quality Improvements** ✅
- [x] Microsoft Pragmatic Rust Guidelines compliance
  - [x] Debug trait implementations on 5 complex types (InfrawareTerminal, CommandCache, CompiledPatterns, ClassifierChain, KnownCommandHandler)
  - [x] Debug derives on 9 marker structs
  - [x] Replaced 31 `#[allow]` with `#[expect]` for better lint management
  - [x] Configured Microsoft-recommended lints in Cargo.toml (missing_debug_implementations, redundant_imports, unsafe_op_in_unsafe_fn, etc.)
  - [x] Static verification: All 224 tests passing, 0 clippy warnings, 0 compiler warnings

### Next Steps (M2/M3)
- [ ] Real LLM backend integration (endpoint/auth configuration)
- [ ] Auto-install execution (currently prompts only)
- [ ] Advanced markdown rendering (tables, images)
- [ ] Configuration file support (.infraware-terminal.toml)
- [ ] Command history persistence to disk
- [ ] Multi-shell support (Zsh, Fish)
- [ ] Cloud provider integrations (AWS CLI, Azure CLI)

## ⚠️ Known Limitations

### M1 Constraints (By Design)

- **Interactive Commands**: 28 commands supported with full TUI suspension. Some commands that require persistent TTY sessions are blocked (ssh, tmux, screen, python REPL, etc.)
  - Supported: vim, nano, less, man, mc, ranger, top, htop, sudo, and more (see usage section for complete list)
  - Blocked: 31 commands requiring long-running sessions or network access. Use alternatives or separate terminal
  - Platform: Unix/Linux/macOS only (Windows shows error message)
- **Alias Cache TTL**: No automatic TTL/invalidation - alias files modified externally during session require manual `reload-aliases` command to be recognized
- **Command Cache TTL**: No automatic TTL/invalidation - commands installed during a session require `reload-commands` to be recognized
- **Tab Completion**: Basic file and command completion only - no integration with bash/zsh completion systems
- **Command History**: Session-only persistence - history is not saved to disk when the terminal closes (accessible via `history` command)
- **Configuration**: Uses hardcoded defaults - no config file support yet
- **Advanced Markdown**: Only basic formatting with syntax highlighting - tables, images deferred to M2

### Future Improvements (M2/M3)

- Automatic command cache invalidation with TTL
- Full bash/zsh completion integration
- Persistent command history across sessions
- Configuration file support (.infraware-terminal.toml)
- Advanced markdown rendering (tables, lists, images)
- Multi-shell support (Zsh, Fish)
- Cloud provider integrations (AWS CLI, Azure CLI enhancements)

## 🔧 Configuration

Configuration will be added in future milestones. For M1, the terminal uses sensible defaults.

Planned configuration options:
- LLM backend URL and authentication
- Known commands whitelist
- Color schemes
- Key bindings

## 🤝 Contributing

This is a 3-month contractor project. For questions or issues:

1. Check the [project brief](infraware_terminal_project_brief.md)
2. Review the architecture documentation
3. Run tests to ensure your changes work
4. Follow Rust best practices and conventions

## 📄 License

[To be determined]

## 🙏 Acknowledgments

- [ratatui](https://github.com/ratatui-org/ratatui) - Terminal UI framework
- [crossterm](https://github.com/crossterm-rs/crossterm) - Cross-platform terminal control
- [syntect](https://github.com/trishume/syntect) - Syntax highlighting

## 📞 Support

For technical questions:
- Architecture decisions → Technical Lead
- LLM backend integration → Backend Team
- Product requirements → Product Manager
- Timeline concerns → Project Manager

---

**Note**: This is M1 (Milestone 1) of a 3-month project. The codebase is designed for extensibility and will evolve significantly in M2 and M3.
