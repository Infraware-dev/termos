# Infraware Terminal - UML Architecture Diagrams (Updated)

This directory contains comprehensive PlantUML diagrams documenting the Infraware Terminal architecture, with recent updates reflecting Microsoft Rust Guidelines compliance.

## Recent Updates

**Date**: November 24, 2025
**Focus**: Debug trait implementation compliance
**Status**: All 5 complex types now implement Debug per Microsoft guidelines

### Debug Implementations Added
1. **InfrawareTerminal**: Main application struct
2. **InfrawareTerminalBuilder**: Builder pattern implementation
3. **CompiledPatterns**: Precompiled regex patterns
4. **CommandCache**: Command discovery cache
5. **ClassifierChain**: SCAN algorithm handler chain
6. **KnownCommandHandler**: Known command handler
7. **InputClassifier**: Input classification orchestrator
8. **TerminalUI**: TUI wrapper
9. **HttpLLMClient**: HTTP-based LLM client

## Diagram Index

### Core Architecture

#### **00-main-application-architecture.puml**
- **Focus**: Main application structure with Builder pattern
- **Audience**: System architects, integration engineers
- **Key Concepts**:
  - InfrawareTerminal composition
  - InfrawareTerminalBuilder with dependency injection
  - Component ownership and lifecycle
  - Event flow coordination
- **Best For**: Understanding how all components fit together

#### **01-scan-algorithm-10-handlers.puml**
- **Focus**: SCAN algorithm with complete 10-handler chain
- **Audience**: Input classification engineers, algorithm designers
- **Key Concepts**:
  - 10-handler Chain of Responsibility pattern
  - Performance optimizations (<100μs target)
  - Handler ordering (fast paths first)
  - Debug trait implementation in ClassifierChain
- **Handlers Documented**:
  1. EmptyInputHandler - Fast path
  2. HistoryExpansionHandler - Bash-style !! expansion
  3. ApplicationBuiltinHandler - clear, reload-* commands
  4. ShellBuiltinHandler - 45+ builtins
  5. PathCommandHandler - ./script.sh detection
  6. KnownCommandHandler - 60+ DevOps commands
  7. CommandSyntaxHandler - Flag/pipe detection
  8. TypoDetectionHandler - Levenshtein distance
  9. NaturalLanguageHandler - Language-agnostic heuristics
  10. DefaultHandler - Fallback catch-all
- **Best For**: Understanding input classification pipeline

#### **02-patterns-and-caches.puml**
- **Focus**: Pattern compilation and command caching infrastructure
- **Audience**: Performance engineers, cache designers
- **Key Concepts**:
  - CompiledPatterns (Debug trait implementation)
  - CommandCache (Debug trait implementation)
  - Global LazyLock instances
  - Thread-safe caching with RwLock
  - Alias expansion system
  - Performance implications (O(1) vs O(n) lookups)
- **Best For**: Understanding caching strategy and performance characteristics

#### **03-orchestrators-and-workflows.puml**
- **Focus**: Orchestrator pattern and workflow coordination (SRP)
- **Audience**: Workflow engineers, application architects
- **Key Concepts**:
  - CommandOrchestrator: Command execution workflow
  - NaturalLanguageOrchestrator: LLM query workflow
  - TabCompletionHandler: Tab completion workflow
  - Single Responsibility Principle (SRP)
  - Integration with CommandExecutor and PackageInstaller
  - Strategy pattern for package managers (7 managers)
- **Best For**: Understanding workflow separation and orchestration

#### **04-terminal-state-and-buffers.puml**
- **Focus**: Terminal state management with buffer composition
- **Audience**: UI engineers, state management specialists
- **Key Concepts**:
  - TerminalState composition with specialized buffers
  - OutputBuffer: Display with scrolling
  - InputBuffer: Cursor-aware input
  - CommandHistory: History navigation with synchronization
  - TerminalMode: Mode tracking (Normal, ExecutingCommand, WaitingLLM)
  - EventHandler: Event polling and translation
  - TerminalUI: Ratatui rendering with suspend/resume
  - Windows-specific event handling
- **Best For**: Understanding terminal state and event handling

#### **05-llm-integration.puml**
- **Focus**: LLM client architecture and response rendering
- **Audience**: LLM integration engineers, backend developers
- **Key Concepts**:
  - LLMClientTrait: Trait design for multiple backends
  - MockLLMClient: Development/testing client (Debug trait)
  - HttpLLMClient: Production HTTP client (Debug trait)
  - ResponseRenderer: Markdown formatting with syntax highlighting
  - NaturalLanguageOrchestrator: Query processing
  - Builder pattern: Dependency injection
  - Environment configuration (INFRAWARE_LLM_URL)
- **Best For**: Understanding LLM integration and extensibility

#### **06-complete-class-diagram.puml**
- **Focus**: Complete system class diagram with all relationships
- **Audience**: System integrators, code reviewers
- **Key Concepts**:
  - All major classes and traits
  - Complete dependency graph
  - Interface implementations
  - Enum definitions
  - Composition vs aggregation relationships
- **Best For**: Comprehensive system overview

## Architecture Overview

### Core Flow
```
User Input → Alias Expansion → InputClassifier (SCAN) →
  [Command Path | Natural Language Path]
      ↓                              ↓
CommandOrchestrator          NaturalLanguageOrchestrator
      ↓                              ↓
CommandExecutor              LLMClient (Mock/HTTP)
      ↓                              ↓
Terminal Output         ResponseRenderer
```

### Key Design Patterns

#### Chain of Responsibility (Input Classification)
- **Location**: `src/input/handler.rs`, `src/input/classifier.rs`
- **Implementation**: 10-handler chain for command vs NL classification
- **Performance**: <100μs average, <1μs cache hits
- **Diagram**: 01-scan-algorithm-10-handlers.puml

#### Builder Pattern (Application Construction)
- **Location**: `src/main.rs`
- **Implementation**: InfrawareTerminalBuilder for dependency injection
- **Flexibility**: Swap LLM clients, orchestrators, renderers
- **Diagram**: 00-main-application-architecture.puml

#### Strategy Pattern (Package Managers)
- **Location**: `src/executor/package_manager.rs`
- **Implementation**: 7 package managers (apt, yum, dnf, pacman, brew, choco, winget)
- **Selection**: Priority-based detection
- **Diagram**: 03-orchestrators-and-workflows.puml

#### Trait-Based Dependency Injection (LLM Clients)
- **Location**: `src/llm/client.rs`
- **Implementation**: LLMClientTrait with MockLLMClient and HttpLLMClient
- **Extensibility**: Add new LLM backends without code changes
- **Diagram**: 05-llm-integration.puml

#### Single Responsibility Principle (Orchestrators)
- **Location**: `src/orchestrators/`
- **Implementation**: CommandOrchestrator, NaturalLanguageOrchestrator, TabCompletionHandler
- **Benefit**: Clear separation of concerns, easier testing
- **Diagram**: 03-orchestrators-and-workflows.puml

## Microsoft Rust Guidelines Compliance

All complex types now implement the Debug trait per Microsoft's Rust Guidelines:

### Debug Implementations

1. **InfrawareTerminal**
   - Returns structured debug with field names
   - Hides internal complexity of nested orchestrators
   - File: `src/main.rs`

2. **InfrawareTerminalBuilder**
   - Returns `Some/None` flags for option fields
   - Safe to log without exposing internals
   - File: `src/main.rs`

3. **CompiledPatterns**
   - Returns `<RegexSet>` placeholders
   - Prevents huge regex output
   - File: `src/input/patterns.rs`

4. **CommandCache**
   - Returns counts instead of full sets/maps
   - File: `src/input/discovery.rs`

5. **ClassifierChain**
   - Returns handler count only
   - File: `src/input/handler.rs`

6. **KnownCommandHandler**
   - Returns known_commands_count
   - File: `src/input/handler.rs`

7. **InputClassifier**
   - Returns `<ClassifierChain>` placeholder
   - File: `src/input/classifier.rs`

8. **TerminalUI**
   - Returns `<Terminal>` placeholder
   - File: `src/terminal/tui.rs`

9. **HttpLLMClient**
   - Returns base_url and `<reqwest::Client>` placeholder
   - File: `src/llm/client.rs`

## Performance Characteristics

### SCAN Algorithm
- Average: <100μs
- Fast path (empty input): <1μs
- Cache hit (KnownCommand): <1μs
- Cache miss (PATH lookup): 1-5ms
- Typo detection: ~100μs

### Cache Performance
- CommandCache read hit: O(1) hash lookup
- CommandCache write miss: O(PATH_length) via `which`
- Alias expansion: O(1) HashMap lookup
- Pattern matching: ~10-100x faster with precompiled RegexSet

### Optimizations
1. **Pattern Precompilation**: Global LazyLock<CompiledPatterns>
2. **Command Caching**: RwLock<CommandCache> with hit/miss sets
3. **Fast Paths First**: Empty, history, appbuiltins handled before expensive checks
4. **Short-Circuit Evaluation**: Handler chain exits on first match

## Module Structure

```
src/
├── main.rs                          # InfrawareTerminal + Builder
├── input/
│   ├── classifier.rs                # InputClassifier + InputType
│   ├── handler.rs                   # 10-handler Chain of Responsibility
│   ├── patterns.rs                  # CompiledPatterns (global LazyLock)
│   ├── discovery.rs                 # CommandCache (global LazyLock)
│   ├── history_expansion.rs         # HistoryExpansionHandler
│   ├── application_builtins.rs      # ApplicationBuiltinHandler
│   ├── shell_builtins.rs            # ShellBuiltinHandler
│   ├── known_commands.rs            # 60+ DevOps commands
│   ├── typo_detection.rs            # Levenshtein distance detection
│   └── parser.rs                    # Shell-words parsing
├── executor/
│   ├── command.rs                   # CommandExecutor
│   ├── package_manager.rs           # 7 PackageManager implementations
│   └── install.rs                   # PackageInstaller
├── orchestrators/
│   ├── command.rs                   # CommandOrchestrator
│   ├── natural_language.rs          # NaturalLanguageOrchestrator
│   └── tab_completion.rs            # TabCompletionHandler
├── terminal/
│   ├── tui.rs                       # TerminalUI (ratatui)
│   ├── state.rs                     # TerminalState
│   ├── buffers.rs                   # OutputBuffer, InputBuffer, CommandHistory
│   ├── events.rs                    # EventHandler, TerminalEvent
│   └── splash.rs                    # SplashScreen animation
├── llm/
│   ├── client.rs                    # LLMClientTrait, MockLLMClient, HttpLLMClient
│   └── renderer.rs                  # ResponseRenderer (markdown formatting)
└── utils/
    └── message.rs                   # MessageFormatter
```

## Testing and Verification

### Test Coverage
- 496 total tests (M1 complete)
- SCAN algorithm: 150+ tests (`cargo test --test classifier_tests`)
- Executor: 100+ tests (`cargo test --test executor_tests`)
- Integration: 50+ tests (`cargo test --test integration_tests`)

### Performance Benchmarks
```bash
cargo bench scan_          # SCAN algorithm benchmarks
```

### Compliance Verification
```bash
cargo fmt --all --check   # Code formatting
cargo clippy --all-targets --all-features -- -D warnings  # Linting
cargo llvm-cov            # 75% coverage requirement
```

## Roadmap (M2/M3)

### M2: Production Readiness
- Real LLM endpoint integration (HttpLLMClient with auth)
- Streaming responses
- Command history persistence (~/.infraware_history)
- Auto-install prompts and execution
- Bash/zsh completion integration
- Response caching

### M3: Advanced Features
- Config file support (~/.infrawarerc)
- Custom command aliases
- Plugin system
- Distributed tracing
- Performance metrics collection

## Quick Reference: Finding Your Diagram

| Task | Diagram |
|------|---------|
| Understand overall system | 00-main-application.puml |
| Study input classification | 01-scan-algorithm-10-handlers.puml |
| Optimize cache strategy | 02-patterns-and-caches.puml |
| Add new workflow | 03-orchestrators-and-workflows.puml |
| Modify terminal UI | 04-terminal-state-and-buffers.puml |
| Integrate LLM backend | 05-llm-integration.puml |
| Complete reference | 06-complete-class-diagram.puml |

## File Locations

All diagrams are located in: `/home/crist/infraware-terminal/terminal-app/docs/uml/`

Generated from source code in: `/home/crist/infraware-terminal/terminal-app/src/`

## Diagram Rendering

To render these diagrams to images:
```bash
# Using PlantUML command-line
plantuml -Tpng docs/uml/*.puml

# Using online viewer
https://www.plantuml.com/plantuml/uml/

# Using VS Code PlantUML extension
ms-vscode.plantuml
```

## Document History

| Date | Changes |
|------|---------|
| Nov 24, 2025 | Updated for Debug trait implementations, added 00-main-application-architecture.puml, updated 01-scan-algorithm-10-handlers.puml, added 02-patterns-and-caches.puml, regenerated all diagrams |
| Nov 20, 2025 | Initial comprehensive architecture documentation |

---

**Status**: M1 Complete, Production-Ready
**Last Updated**: November 24, 2025
**Maintainer**: Architecture Documentation
