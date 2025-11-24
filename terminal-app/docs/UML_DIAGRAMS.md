# Infraware Terminal - UML Class Diagrams

This directory contains comprehensive UML diagrams in PlantUML format, documenting the architecture of the Infraware Terminal application.

## Overview

The Infraware Terminal is a hybrid command interpreter with AI assistance for DevOps operations. It intelligently routes user input to either shell command execution or an LLM backend for natural language queries.

**Key Metrics:**
- 32 Rust source files
- 5,494 lines of code
- 7 major modules
- 6+ design patterns applied
- 245+ unit tests

## Diagrams

### 1. `architecture.puml` - System-Wide Architecture (11 KB)

**Purpose:** High-level overview of all modules and their relationships.

**Shows:**
- 7 major modules: input, executor, terminal, orchestrators, llm, utils
- Main application structure (InfrawareTerminal, InfrawareTerminalBuilder)
- Module composition and data flow
- Key design patterns at system level

**Key Relationships:**
- InfrawareTerminal composes all major components
- InputClassifier feeds into CommandOrchestrator or NaturalLanguageOrchestrator
- Output rendered via TerminalUI from TerminalState

**Use Case:**
- Understanding overall system architecture
- Identifying module boundaries
- Following high-level data flow
- Explaining system to stakeholders

---

### 2. `input_module.puml` - SCAN Algorithm & Chain of Responsibility (11 KB)

**Purpose:** Detailed view of input classification (the SCAN algorithm).

**Shows:**
- InputClassifier with ClassifierChain
- Complete 10-handler chain with ordering
- Handler responsibilities and performance characteristics
- Supporting services: CommandCache, CompiledPatterns
- History expansion details (!!,  !$, !^, !*)
- Shell builtin recognition (45+ commands)

**Handler Chain (in order):**
1. EmptyInputHandler - <1μs
2. HistoryExpansionHandler - ~1-5μs (Bash history expansion)
3. ApplicationBuiltinHandler - <1μs (clear, reload-aliases, reload-commands)
4. ShellBuiltinHandler - <1μs (., :, [, [[, export, etc.)
5. PathCommandHandler - ~10μs (./script.sh, /usr/bin/cmd)
6. KnownCommandHandler - <1μs cache hit (60+ DevOps whitelist)
7. CommandSyntaxHandler - ~10μs (flags, pipes, redirects)
8. TypoDetectionHandler - ~100μs (Levenshtein distance ≤2)
9. NaturalLanguageHandler - ~0.5μs (language-agnostic heuristics)
10. DefaultHandler - <1μs (fallback to LLM)

**Performance Optimizations:**
- Precompiled RegexSet (10-100x faster)
- Thread-safe caching with RwLock
- 70% of inputs hit KnownCommandHandler
- Average classification: <100μs

**Use Case:**
- Understanding input classification logic
- Performance analysis
- Adding new handlers
- Debugging classification issues

---

### 3. `executor_module.puml` - Command Execution & Package Management (8.1 KB)

**Purpose:** Command execution and package management (Strategy pattern).

**Shows:**
- CommandExecutor with async execution
- CommandOutput structure
- PackageManager trait (Strategy pattern)
- 7 package manager implementations:
  - Linux: Apt, Yum, Dnf, Pacman
  - macOS: Brew (highest priority)
  - Windows: Choco, Winget (Winget preferred)
- PackageInstaller with manager selection
- CompletionProvider for tab completion
- 43 interactive commands blocklist

**Strategy Pattern Benefits:**
- Easy to add new managers without modifying existing code
- Runtime selection by priority and availability
- Platform-aware (Windows/Linux/macOS)
- Consistent interface across managers

**Package Manager Priorities:**
- VERY_HIGH (90): Brew
- HIGH (85): Dnf, Winget
- MEDIUM (80): Apt, Yum, Pacman
- LOW (70): Choco

**Interactive Commands Blocked:**
vim, nano, python, top, htop, less, man, ssh, tmux, gdb, mysql, psql, etc. (43 total)
Reason: Require TTY, would break in non-interactive mode

**Use Case:**
- Understanding command execution flow
- Package manager selection logic
- Adding new package managers
- Understanding interactive command blocking

---

### 4. `terminal_module.puml` - TUI State & Buffer Components (9.2 KB)

**Purpose:** Terminal UI state management with SRP-compliant buffer components.

**Shows:**
- TerminalState composition
- Three SRP-compliant buffer components:
  - OutputBuffer: Scrollable output (auto-trim at 10,000 lines)
  - InputBuffer: Text input with cursor (Unicode-aware, grapheme-safe)
  - CommandHistory: History navigation
- EventHandler for keyboard events
- TerminalUI for rendering with ratatui
- TerminalMode enum (Normal, ExecutingCommand, WaitingLLM)

**Single Responsibility Principle:**
- OutputBuffer: Only handles output display and scrolling
- InputBuffer: Only handles text input and cursor positioning
- CommandHistory: Only handles history navigation
- TerminalState: Coordinates buffer interactions

**Key Features:**
- Unicode support with grapheme boundary handling (fixes emoji cursor bug)
- Memory management with auto-trimming
- 100ms event polling timeout
- Non-blocking async execution

**Use Case:**
- Understanding terminal state management
- Buffer component interactions
- Adding UI features
- Debugging rendering issues

---

### 5. `executor_module.puml` - Command Execution & Package Management

(See section 3 above)

---

### 6. `orchestrators_module.puml` - Workflow Coordination (8.4 KB)

**Purpose:** Specialized orchestrators for different workflow types.

**Shows:**
- CommandOrchestrator: Command execution workflow
- NaturalLanguageOrchestrator: LLM query workflow
- TabCompletionHandler: Tab completion workflow
- Supporting services (CommandExecutor, LLMClient, etc.)

**Responsibilities:**
- CommandOrchestrator:
  1. Handle special built-in commands (clear, reload-aliases)
  2. Verify command exists (skip shell builtins, history expansion)
  3. Execute via CommandExecutor
  4. Display output in TerminalState
  5. Re-render TerminalUI

- NaturalLanguageOrchestrator:
  1. Display "Querying AI assistant..." message
  2. Render waiting state
  3. Query LLM backend
  4. Render markdown response with syntax highlighting
  5. Display in TerminalState
  6. Re-render TerminalUI

- TabCompletionHandler:
  1. Extract partial word from input
  2. Complete command or file path
  3. Update input buffer
  4. Re-render TerminalUI

**Single Responsibility Principle:**
Each orchestrator handles ONE workflow type, keeping main.rs clean and focused.

**Use Case:**
- Understanding workflow coordination
- Adding new workflows
- Error handling patterns
- State management

---

### 7. `llm_module.puml` - Natural Language Processing & Response Rendering (8.2 KB)

**Purpose:** LLM client abstraction and response rendering.

**Shows:**
- LLMClientTrait interface
- Two implementations: HttpLLMClient, MockLLMClient
- LLMRequest and LLMResponse data models
- ResponseRenderer for markdown and syntax highlighting
- SyntaxHighlighter for code blocks
- Integration with NaturalLanguageOrchestrator

**Supported Languages (Syntax Highlighting):**
Rust, Python, Bash/Shell, JSON, JavaScript/TypeScript, YAML, Go, C/C++

**Markdown Support (M1):**
- Code blocks with syntax highlighting
- Inline code formatting
- Basic text formatting (bold, italic)
- Lists (bullet points)
- Headers (# ## ###)

**Not Yet Implemented (M2/M3):**
- Tables
- Images
- Links
- LaTeX/Math rendering
- Mermaid diagrams

**Dependency Injection:**
- Production: HttpLLMClient (real endpoint TBD)
- Testing: MockLLMClient (deterministic responses)
- No code changes needed to swap implementations

**Use Case:**
- Understanding LLM integration
- Adding new client implementations
- Response rendering
- Testing with mocks

---

### 8. `design_patterns.puml` - Design Patterns Reference (14 KB)

**Purpose:** Comprehensive overview of all design patterns used in the system.

**Patterns Documented:**

1. **Chain of Responsibility** (Input Classification)
   - 10 handlers in strict order
   - Flexible handler composition
   - Each handler can pass to next

2. **Strategy Pattern** (Package Managers)
   - 7 package manager implementations
   - Runtime selection by priority
   - Cross-platform support

3. **Builder Pattern** (Terminal Construction)
   - Flexible, testable object construction
   - Dependency injection support
   - Sensible defaults

4. **Single Responsibility Principle** (Buffers & Orchestrators)
   - Each component has one reason to change
   - Improved testability and maintainability
   - Better reusability

5. **Dependency Injection** (LLM Client)
   - Mock for testing, real for production
   - No code changes to swap implementations
   - Flexible configurations

6. **Lazy Singleton** (Patterns & Cache)
   - Zero-cost initialization
   - Thread-safe global state
   - Performance optimized

**Use Case:**
- Learning design patterns in Rust
- Understanding architectural decisions
- Reference for implementing similar patterns
- Design pattern education

---

### 9. `data_flow.puml` - Main Event Loop & Data Flow (7.7 KB)

**Purpose:** Detailed flow diagram of the main event loop and data transformations.

**Shows:**
- Complete main event loop cycle
- Event handling for all event types
- Input classification path
- Command execution path
- LLM query path
- State updates and re-rendering
- Alias expansion and history management

**Main Loop Steps:**
1. Poll for terminal events (100ms timeout)
2. Process event (keyboard, mouse, resize)
3. Update TerminalState based on event
4. If Submit:
   - Classify input (SCAN algorithm)
   - Route to CommandOrchestrator or NaturalLanguageOrchestrator
   - Update OutputBuffer
5. Re-render TerminalUI
6. Loop until quit

**Performance Characteristics:**
- Event latency: <100ms typical
- Classification: <100μs average
- Command execution: Async, non-blocking
- LLM query: 30 second timeout

**Use Case:**
- Understanding main event loop
- Performance analysis
- Debugging data flow issues
- User interaction modeling

---

## How to View the Diagrams

### Option 1: Online PlantUML Editor
1. Go to https://www.plantuml.com/plantuml/uml/
2. Copy the contents of any .puml file
3. Paste into the editor
4. View the diagram

### Option 2: Local PlantUML Installation
```bash
# Install PlantUML (macOS)
brew install plantuml

# Install PlantUML (Linux)
sudo apt install plantuml

# Generate PNG from .puml file
plantuml architecture.puml

# Generate SVG (better for zoom)
plantuml -tsvg architecture.puml
```

### Option 3: VS Code Plugin
1. Install "PlantUML" extension by jebbs
2. Open any .puml file
3. Press `Alt+D` to preview
4. Export to PNG/SVG

### Option 4: Visual Studio Code with Live Preview
```bash
# Install extension
code --install-extension jebbs.plantuml

# Or use the PlantUML Diagram Viewer
code --install-extension evilz.vscode-plantuml
```

---

## Architecture Summary

### Core Loop
```
User Input → Alias Expansion → InputClassifier → [Command | Natural Language]
              (CommandCache)   (SCAN Algorithm)         ↓              ↓
                                 (10 handlers)   CommandOrchestrator  NLOrchestrator
                                                          ↓                   ↓
                                                  CommandExecutor       LLMClient
                                                          ↓                   ↓
                                                    CommandOutput      ResponseRenderer
                                                          ↓                   ↓
                                                    TerminalState (OutputBuffer)
                                                          ↓
                                                      TerminalUI.render()
```

### Module Responsibilities

| Module | Responsibility | Design Pattern |
|--------|-----------------|-----------------|
| **input** | Classify user input as commands or natural language | Chain of Responsibility |
| **executor** | Execute shell commands and manage packages | Strategy (package managers) |
| **terminal** | TUI state management and rendering | SRP (buffer components) |
| **orchestrators** | Coordinate workflows (command, LLM, completion) | SRP (single workflow each) |
| **llm** | LLM client abstraction and response rendering | Dependency Injection |
| **utils** | Message formatting and utilities | - |
| **main.rs** | Application entry and event loop | Builder Pattern |

### Performance Targets
- Input classification: <100μs
- Known command (cache hit): <1μs
- Typo detection: <100μs
- Command execution: Async, non-blocking
- LLM query: 30 second timeout

---

## Adding New Features

### Adding a New Input Handler
1. Implement InputHandler trait
2. Add to ClassifierChain in InputClassifier::new()
3. Document performance characteristics
4. Add comprehensive test coverage

### Adding a New Package Manager
1. Implement PackageManager trait
2. Add to PackageInstaller::detect_package_manager()
3. Set appropriate priority
4. Test on target platform

### Adding a New Orchestrator
1. Create new orchestrator struct
2. Implement workflow coordination
3. Add to InfrawareTerminal event handling
4. Document responsibilities

### Adding Markdown Features
1. Update ResponseRenderer
2. Add tests for new format
3. Ensure ANSI code compatibility
4. Test with real LLM responses

---

## References

### Rust Ecosystem
- **ratatui**: https://ratatui.rs/ (TUI framework)
- **crossterm**: https://docs.rs/crossterm/ (terminal control)
- **tokio**: https://docs.rs/tokio/ (async runtime)
- **regex**: https://docs.rs/regex/ (pattern matching)
- **once_cell**: https://docs.rs/once_cell/ (lazy statics)
- **async_trait**: https://docs.rs/async_trait/ (async trait support)

### Design Patterns
- Chain of Responsibility (GoF)
- Strategy Pattern (GoF)
- Builder Pattern (GoF)
- Single Responsibility Principle (SOLID)
- Dependency Injection (SOLID)

### PlantUML
- **Online Editor**: https://www.plantuml.com/plantuml/uml/
- **Documentation**: https://plantuml.com/
- **Examples**: https://plantuml.com/diagrams

---

## Diagram File Sizes

| Diagram | Size | Lines | Complexity |
|---------|------|-------|------------|
| architecture.puml | 11 KB | ~400 | High |
| input_module.puml | 11 KB | ~450 | Very High |
| executor_module.puml | 8.1 KB | ~300 | Medium |
| terminal_module.puml | 9.2 KB | ~350 | Medium |
| orchestrators_module.puml | 8.4 KB | ~320 | Medium |
| llm_module.puml | 8.2 KB | ~300 | Medium |
| design_patterns.puml | 14 KB | ~500 | Very High |
| data_flow.puml | 7.7 KB | ~400 | High |
| **Total** | **77.6 KB** | **~3,100** | - |

---

## Notes

- All diagrams are auto-generated documentation that stays in sync with the codebase
- Diagrams are self-contained and compilable with PlantUML
- Performance characteristics and design rationale are documented inline
- Examples and code snippets illustrate key concepts
- Color coding helps identify patterns and component types

---

**Generated**: 2025-11-20
**PlantUML Version**: 1.2024.12+
**For**: Infraware Terminal Project (M1 Milestone - Terminal Core MVP)
