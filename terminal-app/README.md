# Infraware Terminal

**A hybrid command interpreter with AI assistance for DevOps operations**

Infraware Terminal is a TUI-based terminal application that intelligently routes user input to either shell command execution or an LLM backend for natural language queries. It's designed specifically for DevOps engineers working with cloud environments (AWS/Azure).

## 🎯 Project Status

**Current Milestone:** M1 - Terminal Core MVP (Month 1)
**Version:** 0.1.0
**Tech Stack:** Rust + TUI (ratatui/crossterm)

This is the initial project setup with the complete module structure. Implementation is following the 4-week timeline outlined in the project brief.

## ✨ Features

### Planned for M1 (Current Milestone)

- ✅ **Smart Input Classification**: Automatically detects if input is a shell command or natural language
- ✅ **Command Execution**: Execute shell commands with full stdout/stderr capture
- ✅ **Auto-Install**: Detect missing commands and offer to install them (framework ready)
- ✅ **LLM Integration**: Route natural language queries to AI backend (mock implementation)
- ✅ **Syntax Highlighting**: Code blocks with syntax highlighting in LLM responses
- ✅ **Tab Completion**: Basic command and file path completion
- ✅ **Command History**: Navigate previous commands with arrow keys
- ✅ **TUI Interface**: Clean, responsive terminal UI with ratatui

### Coming in M2/M3

- Multi-shell support (Zsh, Fish)
- Advanced markdown rendering (tables, images)
- Telemetry and analytics
- Cloud provider integrations (AWS CLI, Azure CLI)
- Plugin system

## 🏗️ Architecture

```
┌─────────────────────────────────────┐
│         TUI Frontend (Rust)         │
│    (ratatui/crossterm based)        │
└──────────────┬──────────────────────┘
               │
    ┌──────────▼──────────┐
    │   Input Classifier   │
    │  (command vs phrase) │
    └──────────┬───────────┘
               │
       ┌───────┴────────┐
       │                │
  ┌────▼─────┐    ┌────▼─────┐
  │ Command  │    │   LLM    │
  │ Executor │    │  Router  │
  └──────────┘    └──────────┘
```

## 📂 Project Structure

```
infraware-terminal/
├── Cargo.toml
├── src/
│   ├── main.rs              # Entry point + event loop
│   ├── lib.rs               # Library exports
│   ├── terminal/
│   │   ├── mod.rs
│   │   ├── tui.rs          # ratatui rendering logic
│   │   ├── state.rs        # Terminal state management
│   │   └── events.rs       # Keyboard event handling
│   ├── input/
│   │   ├── mod.rs
│   │   ├── classifier.rs   # Command vs natural language
│   │   └── parser.rs       # Shell command parsing
│   ├── executor/
│   │   ├── mod.rs
│   │   ├── command.rs      # Command execution
│   │   ├── install.rs      # Auto-install logic
│   │   └── completion.rs   # Tab completion
│   ├── llm/
│   │   ├── mod.rs
│   │   ├── client.rs       # LLM API client
│   │   └── renderer.rs     # Response formatting
│   └── utils/
│       ├── mod.rs
│       ├── ansi.rs         # ANSI color utilities
│       └── errors.rs       # Error types
└── tests/
    ├── classifier_tests.rs
    ├── executor_tests.rs
    └── integration_tests.rs
```

## 🚀 Getting Started

### Prerequisites

- Rust 1.70+ (2021 edition)
- Linux, macOS, or Windows
- Terminal with ANSI color support

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
2. **Ask questions**: Type natural language queries (e.g., "how do I list files?")
3. **Navigate history**: Use ↑/↓ arrow keys
4. **Tab completion**: Press Tab to complete commands/paths
5. **Quit**: Press Ctrl+C or Ctrl+D

## 🧪 Testing

```bash
# Run all tests
cargo test

# Run specific test suite
cargo test --test classifier_tests
cargo test --test executor_tests
cargo test --test integration_tests

# Run with output
cargo test -- --nocapture
```

## 📝 Development Roadmap

### Week 1: TUI Foundation ✅
- [x] Project setup
- [x] Basic TUI with ratatui
- [x] Keyboard event capture
- [x] Output buffer management

### Week 2: Command Classification & Execution (In Progress)
- [x] Input classifier
- [x] Command parser
- [x] Basic command executor
- [ ] Error handling improvements
- [ ] Integration testing

### Week 3: Auto-Install & LLM Integration (Upcoming)
- [ ] Command existence check
- [ ] Auto-install flow
- [ ] Real LLM client (replace mock)
- [ ] Natural language routing

### Week 4: Polish & Testing (Upcoming)
- [ ] Advanced markdown rendering
- [ ] Cross-platform testing
- [ ] Bug fixes
- [ ] Documentation

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
