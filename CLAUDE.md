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
- `state.rs`: Terminal state (input/output buffers, mode, cursor)
- `events.rs`: Keyboard event handling

**`input/`** - Input classification and parsing
- `classifier.rs`: **Critical component** - distinguishes commands from natural language using:
  - Whitelist of known DevOps commands (kubectl, docker, aws, terraform, etc.)
  - Heuristics for command syntax (flags, pipes, paths)
  - Multilingual natural language patterns (English, Italian, Spanish, French, German)
- `parser.rs`: Shell command parsing using `shell-words` crate

**`executor/`** - Command execution
- `command.rs`: Async command execution with stdout/stderr capture
- `install.rs`: Auto-install missing commands via package managers
- `completion.rs`: Tab completion for commands and file paths

**`llm/`** - LLM integration
- `client.rs`: LLM API client (currently MockLLMClient)
- `renderer.rs`: Markdown response formatting with syntax highlighting

**`utils/`** - Shared utilities
- `ansi.rs`: ANSI color utilities
- `errors.rs`: Error types

### Key Design Decisions

1. **Input Classification Strategy**: The classifier uses a three-tier approach:
   - First checks known commands whitelist (fastest path)
   - Then applies command syntax heuristics (flags, pipes, etc.)
   - Finally applies natural language heuristics (question words, articles, polite expressions)
   - Default behavior: treat ambiguous input as natural language

2. **Async Execution**: Uses tokio for non-blocking command execution to keep TUI responsive

3. **Multilingual Support**: Classifier recognizes natural language in 5 languages to improve UX for international DevOps teams

4. **M1 Rendering Limits**: Basic markdown only - code blocks with syntax highlighting (rust, python, bash, json), simple inline formatting. Tables, images, and complex markdown deferred to M2/M3.

## Important Constraints

### Git Commits
- **NEVER include Co-Authored-By in commit messages** (user preference)
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
When adding commands to the whitelist in `input/classifier.rs`:
1. Add to `default_known_commands()` array
2. Add test cases to verify classification
3. Consider if auto-install should be supported

### Working with LLM Integration
- Current implementation uses `MockLLMClient` for testing
- Real LLM backend integration pending (endpoint/auth TBD)
- When implementing real client, ensure proper error handling for network timeouts

### TUI State Management
- Terminal state lives in `TerminalState` struct
- Use `TerminalMode` enum to track current state (Normal, ExecutingCommand, WaitingLLM, PromptingInstall)
- Always render after state changes
- Handle terminal resize events properly

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
1. Update heuristics in `InputClassifier` methods
2. Add comprehensive test cases in `classifier.rs` tests
3. Test with real-world edge cases
4. Run integration tests to ensure no regression

### Adding Syntax Highlighting
1. Update `ResponseRenderer::highlight_code()` in `llm/renderer.rs`
2. Use `syntect` crate with appropriate syntax set
3. Test with code samples in different languages
4. Ensure ANSI escape codes render correctly in TUI

## Known Issues & TODOs

- Auto-install feature framework exists but not fully implemented (prompts user but doesn't execute)
- LLM client is mock implementation - needs real backend integration
- Tab completion is basic - doesn't integrate with bash/zsh completion systems
- No configuration file support yet (uses hardcoded defaults)
- Command history persists only during session (not saved to disk)

## References

- Project Brief: `infraware_terminal_project_brief.md`
- README: `README.md`
- ratatui docs: https://ratatui.rs/
- crossterm guide: https://docs.rs/crossterm/latest/crossterm/
- fai sempre cargo fmt prima di commit