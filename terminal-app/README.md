# Infraware Terminal

**A hybrid command interpreter with AI assistance for DevOps operations**

Infraware Terminal is a TUI-based terminal application that intelligently routes user input to either shell command execution or an LLM backend for natural language queries. It's designed specifically for DevOps engineers working with cloud environments (AWS/Azure).

## 🎯 Project Status

**Current Milestone:** M1 - Terminal Core MVP (Month 1)
**Version:** 0.1.0
**Tech Stack:** Rust + TUI (ratatui/crossterm)

This is the initial project setup with the complete module structure. Implementation is following the 4-week timeline outlined in the project brief.

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
- ✅ **SCAN Algorithm**: Advanced input classification with 9-handler chain + alias + history expansion (<100μs avg)
  - Alias expansion before classification (single-level like Bash)
  - Shell builtin support (45+): `.`, `:`, `[`, `[[`, source, export, eval, exec, and more
  - Command recognition with PATH verification and caching
  - Typo detection with Levenshtein distance (e.g., "dokcer" → "docker")
  - Shell operator support (pipes, redirects, logical operators)
  - English natural language patterns with LLM fallback for multilingual
- ✅ **Command Execution**: Async shell command execution with stdout/stderr capture
- ✅ **Shell Operator Support**: Full support for pipes (`|`), redirects (`>`/`<`), logical operators (`&&`/`||`), subshells
- ✅ **Performance Optimizations**: Precompiled regex patterns, thread-safe command caching, RwLock poisoning recovery
- ✅ **Interactive Command Blocking**: Safely blocks 43+ TTY-required commands (vim, top, python, ssh, etc.) with helpful alternatives
- ✅ **Auto-Install Framework**: Detect missing commands and prompt for installation (execution deferred to M2)
- ✅ **LLM Integration**: Mock client ready, route natural language queries to AI backend
- ✅ **Syntax Highlighting**: Code blocks with syntax highlighting (Rust, Python, Bash, JSON)
- ✅ **Tab Completion**: Basic command and file path completion
- ✅ **Command History**: Navigate previous commands with arrow keys
- ✅ **Cross-Platform**: Windows, macOS, and Linux support with platform-specific optimizations
- ✅ **Benchmarking Suite**: Performance benchmarks for SCAN algorithm
- ✅ **Code Quality**: 229 tests passing, 0 clippy warnings, serial tests for shared state, production-ready code

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
User Input → Alias Expansion → InputClassifier (8-Handler Chain)
                (if matches)           ↓
                              ┌────────┼────────┐
                              ↓        ↓        ↓
                          Command    Typo?   Natural Language
                              ↓        ↓        ↓
                          Shell Exec Suggest LLM Backend
```

**9-Handler Chain** (executed in strict order):
1. **EmptyInputHandler** - Fast path for empty input (<1μs)
2. **HistoryExpansionHandler** - Bash-style history expansion: `!!`,  `!$`, `!^`, `!*` (~1-5μs)
3. **ShellBuiltinHandler** - Shell builtins without PATH check: `.`, `:`, `[`, `[[`, source, export, eval, etc. (<1μs)
4. **PathCommandHandler** - Executable paths: `./script.sh`, `/usr/bin/cmd` (~10μs)
5. **KnownCommandHandler** - 60+ DevOps commands with PATH cache (<1μs hit)
6. **CommandSyntaxHandler** - Flags, pipes, redirects detection (~10μs)
7. **TypoDetectionHandler** - Levenshtein distance ≤2: "dokcer" → "docker" (~100μs)
8. **NaturalLanguageHandler** - English patterns (precompiled regex) (~5μs)
9. **DefaultHandler** - Fallback to natural language (<1μs)

**Key Features**:
- Average classification: <100μs
- Precompiled regex patterns (10-100x faster)
- Thread-safe command cache (RwLock)
- English-first with LLM fallback for multilingual support

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
│   │   ├── buffers.rs            # Buffer components (SRP)
│   │   └── events.rs             # Keyboard event handling
│   ├── input/                     # SCAN Algorithm
│   │   ├── classifier.rs         # InputClassifier coordinator
│   │   ├── handler.rs            # 8-handler Chain of Responsibility
│   │   ├── shell_builtins.rs     # Shell builtin recognition (., :, [, [[, etc.)
│   │   ├── patterns.rs           # Precompiled RegexSet patterns
│   │   ├── discovery.rs          # PATH-aware command cache
│   │   ├── typo_detection.rs     # Levenshtein distance typo detection
│   │   └── parser.rs             # Shell command parsing
│   ├── executor/                  # Command execution
│   │   ├── command.rs            # Async command execution
│   │   ├── facade.rs             # Facade pattern interface
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
│       ├── errors.rs             # Error types
│       └── message.rs            # Message formatting
├── tests/                         # Test suites
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
```

### Usage

Once running, you can:

1. **Execute commands**: Type any shell command (e.g., `ls -la`, `docker ps`)
2. **Use history expansion**: Use bash-style history patterns:
   - `!!` - Re-run previous command: `sudo !!`
   - `!$` - Use last argument: `vim !$` (if previous was `cat file.txt`)
   - `!^` - Use first argument: `echo !^` (if previous was `cat file1 file2`)
   - `!*` - Use all arguments: `find !*` (if previous was `ls -la /tmp`)
3. **Use aliases**: Type user-defined aliases from `~/.bashrc`, `~/.bash_aliases`, `~/.zshrc` (e.g., `ll` → expands to `ls -la`)
4. **Ask questions**: Type natural language queries (e.g., "how do I list files?")
5. **Navigate history**: Use ↑/↓ arrow keys
6. **Tab completion**: Press Tab to complete commands/paths
7. **Reload aliases**: Type `reload-aliases` to refresh aliases from config files (useful if editing `.bashrc` during a session)
8. **Quit**: Press Ctrl+C or Ctrl+D

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
- Typo detection: <100μs

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
- [x] Comprehensive test suite (245 tests passing)
- [x] History expansion tests (16 unit tests covering all edge cases)
- [x] Performance benchmarking suite
- [x] Integration tests for end-to-end workflows
- [x] Cross-platform testing (Ubuntu, Windows, macOS)
- [x] CI/CD pipeline (fmt, clippy, coverage ≥75%)
- [x] Documentation (CLAUDE.md, SCAN_ARCHITECTURE.md)
- [x] Zero clippy warnings

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

- **Interactive Commands Blocked**: 43+ commands that require TTY are blocked for safety (vim, top, python REPL, ssh, less, man, etc.)
  - Use command-specific alternatives: e.g., `cat` instead of `less`, `ps aux` instead of `top`, `python -c` for one-liners
- **Alias Cache TTL**: No automatic TTL/invalidation - alias files modified externally during session require manual `reload-aliases` command to be recognized
- **Command Cache TTL**: No automatic TTL/invalidation - commands installed during a session require terminal restart to be recognized
- **Tab Completion**: Basic file and command completion only - no integration with bash/zsh completion systems
- **Command History**: Session-only persistence - history is not saved to disk when the terminal closes
- **Configuration**: Uses hardcoded defaults - no config file support yet
- **Advanced Markdown**: Only basic formatting with syntax highlighting - tables, images deferred to M2

### Future Improvements (M2/M3)

- Command cache invalidation and TTL support
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
