# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Infraware Terminal** is a hybrid command interpreter with AI assistance for DevOps operations. It's NOT a traditional terminal emulator - it intelligently routes user input to either shell command execution or an LLM backend for natural language queries.

**Current Status**: M1 (Month 1) - Terminal Core MVP
**Tech Stack**: Rust + TUI (ratatui/crossterm)
**Target Users**: DevOps engineers working with cloud environments (AWS/Azure)

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

# Clean build artifacts
cargo clean
```

## Architecture

### Core Flow
```
User Input → InputClassifier → [Command Path | Natural Language Path]
              ↓                           ↓
         CommandExecutor             LLMClient
              ↓                           ↓
         Shell Output              ResponseRenderer
```

### Module Structure

**`terminal/`** - TUI rendering and state management
- `tui.rs`: ratatui rendering logic
- `state.rs`: Terminal state composition with buffer components
- `buffers.rs`: **SRP-compliant buffer components** (OutputBuffer, InputBuffer, CommandHistory)
- `events.rs`: Keyboard event handling

**`input/`** - Input classification and parsing (uses Chain of Responsibility pattern)
- `classifier.rs`: Legacy classifier (being phased out)
- `handler.rs`: **New Chain of Responsibility implementation** with handlers:
  - EmptyInputHandler
  - KnownCommandHandler (DevOps commands whitelist)
  - CommandSyntaxHandler (flags, pipes, paths)
  - NaturalLanguageHandler (multilingual: EN, IT, ES, FR, DE)
  - DefaultHandler (fallback)
- `parser.rs`: Shell command parsing using `shell-words` crate

**`executor/`** - Command execution (uses Facade and Strategy patterns)
- `command.rs`: Async command execution with stdout/stderr capture
- `install.rs`: Auto-install workflow
- `package_manager.rs`: **Strategy pattern** for package managers (apt, yum, dnf, pacman, brew, choco, winget)
- `facade.rs`: **Facade pattern** - simplified interface for command execution with auto-install
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
- `errors.rs`: Error types
- `message.rs`: Message formatting helpers

### Key Design Decisions

1. **Design Patterns Used**:
   - **Chain of Responsibility**: Input classification (`input/handler.rs`)
   - **Strategy Pattern**: Package managers (`executor/package_manager.rs`)
   - **Facade Pattern**: Command execution interface (`executor/facade.rs`)
   - **Builder Pattern**: Terminal construction (`main.rs` InfrawareTerminalBuilder)
   - **Single Responsibility Principle**: Orchestrators, buffer components

2. **Input Classification Strategy**: Chain of Responsibility with 5 handlers (in order):
   - EmptyInputHandler: catches empty/whitespace input
   - KnownCommandHandler: whitelist of DevOps commands (fastest path)
   - CommandSyntaxHandler: detects command syntax (flags, pipes, paths)
   - NaturalLanguageHandler: multilingual patterns (EN, IT, ES, FR, DE)
   - DefaultHandler: fallback to natural language

3. **Async Execution**: Uses tokio for non-blocking command execution to keep TUI responsive

4. **Cross-Platform Package Management**: Strategy pattern supports 7 package managers:
   - Linux: apt-get, yum, dnf, pacman
   - macOS: brew (highest priority)
   - Windows: choco, winget (winget preferred over choco)

5. **M1 Rendering Limits**: Basic markdown only - code blocks with syntax highlighting (rust, python, bash, json), simple inline formatting. Tables, images, and complex markdown deferred to M2/M3.

## Important Constraints

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

### Adding New Commands
When adding commands to the whitelist in `input/handler.rs`:
1. Add to `KnownCommandHandler::default_known_commands()` array
2. Add test cases to verify classification
3. Consider if auto-install should be supported via package managers

Note: The old `input/classifier.rs` is being phased out in favor of the Chain of Responsibility pattern in `input/handler.rs`.

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
  - `InputBuffer`: text input with cursor positioning (handles Unicode correctly)
  - `CommandHistory`: history navigation
- Use `TerminalMode` enum to track current state (Normal, ExecutingCommand, WaitingLLM, PromptingInstall)
- Always render after state changes
- Handle terminal resize events properly

### Working with Orchestrators
- Orchestrators separate workflow logic from the main event loop
- `CommandOrchestrator`: handles command execution + auto-install prompts
- `NaturalLanguageOrchestrator`: handles LLM queries + response rendering
- `TabCompletionHandler`: handles tab completion
- When adding new workflows, create a new orchestrator instead of adding to main loop

### Error Handling
- Use `anyhow::Result` for application errors
- Use `thiserror` for custom error types
- Provide user-friendly error messages in TUI output
- Don't crash on command failures - display error and continue

## Common Patterns

### Adding a New TerminalEvent
1. Add variant to `TerminalEvent` enum in `terminal/events.rs`
2. Handle event in `EventHandler::poll_event()`
3. Implement handler in `InfrawareTerminal::handle_event()` in `main.rs`
4. Update TUI rendering if needed

### Modifying Input Classification
1. Add/modify handlers in `input/handler.rs`
2. Chain handlers in correct order (order matters!)
3. Add comprehensive test cases in `handler.rs` tests section
4. Test with real-world edge cases (especially multilingual input)
5. Run integration tests to ensure no regression

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

## Known Issues & TODOs

- Auto-install feature framework exists but not fully implemented (prompts user but doesn't execute)
- LLM client: `HttpLLMClient` exists but needs real backend integration (endpoint/auth TBD)
- Tab completion is basic - doesn't integrate with bash/zsh completion systems
- No configuration file support yet (uses hardcoded defaults)
- Command history persists only during session (not saved to disk)

## Windows-Specific Considerations

**Fixed: Double Input Issue** - On Windows, `crossterm` generates multiple events per keystroke (Press, Repeat, Release). This was causing duplicate character input. **Solution implemented**: Filter events to only process `KeyEventKind::Press` in `terminal/events.rs:41`. This ensures each keystroke is processed exactly once.

## References

- Project Brief: `infraware_terminal_project_brief.md`
- README: `README.md`
- ratatui docs: https://ratatui.rs/
- crossterm guide: https://docs.rs/crossterm/latest/crossterm/
- le commit sempre senza icone e senza co-autho claude