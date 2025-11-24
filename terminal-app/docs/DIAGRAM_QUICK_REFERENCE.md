# Infraware Terminal UML Diagrams - Quick Reference

## What Problem Do These Diagrams Solve?

The Infraware Terminal is a complex system with multiple interacting subsystems. These diagrams help you:

1. **Understand the architecture** - See how modules interact without reading 32 source files
2. **Navigate the codebase** - Know where to look for specific features
3. **Maintain code quality** - Understand design patterns and architectural decisions
4. **Add new features** - Know where to integrate new functionality
5. **Debug issues** - Trace data flow from user input to output

## Start Here

**First time exploring the codebase?**

Read in this order:

1. **`architecture.puml`** (5 min read)
   - Get the big picture
   - Understand 7 major modules
   - See how they connect

2. **`data_flow.puml`** (5 min read)
   - Understand the main event loop
   - Follow user input from keystroke to output
   - See state transformations

3. **Pick a module you care about** (10-15 min read)

## Diagram Reference by Task

### "I want to understand..."

| Topic | Start Here | Then Read |
|-------|-----------|-----------|
| How user input becomes commands or queries | `architecture.puml` → `input_module.puml` | `data_flow.puml` |
| How commands get executed | `executor_module.puml` | `architecture.puml` |
| How the UI renders | `terminal_module.puml` | `data_flow.puml` |
| How the SCAN algorithm works | `input_module.puml` | `design_patterns.puml` |
| How new features are orchestrated | `orchestrators_module.puml` | `architecture.puml` |
| How the LLM integration works | `llm_module.puml` | `data_flow.puml` |
| What design patterns are used | `design_patterns.puml` | All diagrams |
| The complete flow from input to output | `data_flow.puml` | `architecture.puml` |

### "I want to add..."

| Feature | Start Here | Then Read |
|---------|-----------|-----------|
| A new input handler | `input_module.puml` | `design_patterns.puml` |
| A new package manager | `executor_module.puml` | `design_patterns.puml` |
| A new workflow/orchestrator | `orchestrators_module.puml` | `architecture.puml` |
| Markdown rendering feature | `llm_module.puml` | `architecture.puml` |
| New terminal UI feature | `terminal_module.puml` | `data_flow.puml` |

### "I need to debug..."

| Issue | Start Here | Then Read |
|-------|-----------|-----------|
| Input classification not working | `input_module.puml` | `data_flow.puml` |
| Command not executing | `executor_module.puml` | `architecture.puml` |
| LLM not responding | `llm_module.puml` | `data_flow.puml` |
| UI rendering issue | `terminal_module.puml` | `data_flow.puml` |
| Performance issue | `design_patterns.puml` | Module diagram |

## Diagram Contents at a Glance

### 1. architecture.puml
**What**: 7 modules and their relationships
**Key Classes**: InfrawareTerminal, InputClassifier, CommandExecutor, TerminalUI
**Best For**: Understanding the big picture
**Read Time**: 5-10 minutes

### 2. input_module.puml
**What**: SCAN algorithm with 10-handler chain
**Key Classes**: ClassifierChain, InputHandler, CommandCache, CompiledPatterns
**Best For**: Understanding input classification
**Read Time**: 10-15 minutes
**Performance**: <100μs average, <1μs for known commands
**Quality**: Microsoft Pragmatic Rust Guidelines compliant

### 3. executor_module.puml
**What**: Command execution and package managers
**Key Classes**: CommandExecutor, PackageManager, PackageInstaller
**Best For**: Understanding command execution
**Read Time**: 5-10 minutes
**Pattern**: Strategy pattern for 7 package managers

### 4. terminal_module.puml
**What**: TUI state and buffer components
**Key Classes**: TerminalState, OutputBuffer, InputBuffer, CommandHistory
**Best For**: Understanding terminal state
**Read Time**: 5-10 minutes
**Note**: Includes Unicode grapheme fix details

### 5. orchestrators_module.puml
**What**: Workflow coordination
**Key Classes**: CommandOrchestrator, NaturalLanguageOrchestrator, TabCompletionHandler
**Best For**: Understanding workflow orchestration
**Read Time**: 5 minutes
**Pattern**: SRP - each orchestrator handles one workflow

### 6. llm_module.puml
**What**: LLM client and response rendering
**Key Classes**: LLMClientTrait, HttpLLMClient, MockLLMClient, ResponseRenderer
**Best For**: Understanding LLM integration
**Read Time**: 5-10 minutes
**Pattern**: Dependency Injection for mock/real client swap

### 7. design_patterns.puml
**What**: All design patterns used in system
**Patterns**: Chain of Responsibility, Strategy, Builder, SRP, DI, Lazy Singleton
**Best For**: Learning Rust design patterns
**Read Time**: 15-20 minutes

### 8. data_flow.puml
**What**: Main event loop and complete data flow
**Key Steps**: Event → Classify → Execute/Query → Render
**Best For**: Understanding complete flow
**Read Time**: 10-15 minutes

## Quick Lookup

### "What file contains...?"

| Concept | File Location |
|---------|---------------|
| 10 handlers in SCAN algorithm | `src/input/handler.rs` + `src/input/classifier.rs` |
| History expansion (!!,  !$, !^, !*) | `src/input/history_expansion.rs` |
| Shell builtins (45+) | `src/input/shell_builtins.rs` |
| Package managers (7 types) | `src/executor/package_manager.rs` |
| Command execution | `src/executor/command.rs` |
| Terminal state | `src/terminal/state.rs` |
| Buffer components | `src/terminal/buffers.rs` |
| Event handling | `src/terminal/events.rs` |
| Command workflow | `src/orchestrators/command.rs` |
| LLM workflow | `src/orchestrators/natural_language.rs` |
| LLM client | `src/llm/client.rs` |
| Response rendering | `src/llm/renderer.rs` |
| Main event loop | `src/main.rs` |

## Design Pattern Quick Reference

| Pattern | Used For | Example |
|---------|----------|---------|
| **Chain of Responsibility** | Input classification | 10 handlers in SCAN algorithm |
| **Strategy Pattern** | Package managers | Apt, Yum, Brew, Choco, Winget, etc. |
| **Builder Pattern** | Terminal construction | InfrawareTerminalBuilder |
| **Single Responsibility** | Buffer components | OutputBuffer, InputBuffer, CommandHistory |
| **Dependency Injection** | LLM client | Mock for testing, Http for production |
| **Lazy Singleton** | Performance | CompiledPatterns, CommandCache |

## Performance Quick Reference

| Operation | Time | Notes |
|-----------|------|-------|
| Input classification | <100μs | Average, with all optimization techniques |
| Known command lookup | <1μs | Cache hit via RwLock |
| Command typo detection | ~100μs | Levenshtein distance |
| Natural language detection | ~5μs | Precompiled regex |
| Command execution | Async | Non-blocking, 30s timeout |
| LLM query | 30s timeout | Network dependent |
| Event polling | 100ms | Non-blocking with timeout |

## Key Architectural Decisions

### 1. SCAN Algorithm (Chain of Responsibility)
- **Why**: Flexible, extensible input classification
- **Benefit**: Easy to add handlers without breaking existing code
- **Performance**: Fast paths first, <100μs average

### 2. Package Manager Strategy Pattern
- **Why**: Support 7 different package managers
- **Benefit**: Add new managers without modifying existing code
- **Cross-platform**: Windows, Linux, macOS support

### 3. Buffer Components (SRP)
- **Why**: Separate concerns (output, input, history)
- **Benefit**: Testable, maintainable, reusable components
- **Trade-off**: More classes but cleaner interfaces

### 4. Orchestrators (SRP)
- **Why**: Separate workflow logic from main loop
- **Benefit**: Clean main.rs, testable workflows
- **Scaling**: Easy to add new workflows

### 5. Dependency Injection (LLM Client)
- **Why**: Swap mock/real client without code changes
- **Benefit**: Testable, flexible configurations
- **Extension**: Easy to add OpenAI, Claude, etc.

### 6. Lazy Singleton (Patterns & Cache)
- **Why**: Expensive resources need initialization once
- **Benefit**: Zero-cost abstraction, thread-safe
- **Performance**: 10-100x faster than runtime compilation

## Unicode Bug Fix Note

**InputBuffer** had a Unicode handling bug that was fixed:

- **Problem**: Direct char indexing broke on multi-byte Unicode characters (é, 中, emoji 👍)
- **Solution**: Use `grapheme_indices()` for proper grapheme boundary detection
- **Impact**: Cursor movement now works correctly on all Unicode

See `src/terminal/buffers.rs` for implementation details.

## Testing

All diagrams are self-contained and compilable with PlantUML:

```bash
# Install PlantUML
brew install plantuml  # or apt install plantuml

# Generate PNG
plantuml architecture.puml

# Generate SVG (better for viewing)
plantuml -tsvg architecture.puml

# View online
# Go to https://www.plantuml.com/plantuml/uml/
# Paste .puml file contents
```

## Where to Go From Here

1. **View a diagram**: Use PlantUML online editor or local tool
2. **Read the code**: Cross-reference with actual source files
3. **Understand flows**: Follow data from input to output
4. **Learn patterns**: Study design pattern implementation
5. **Add features**: Use diagrams as reference when coding

## File Organization

```
docs/
├── architecture.puml              # System-wide overview
├── input_module.puml              # SCAN algorithm detail
├── executor_module.puml           # Command execution detail
├── terminal_module.puml           # TUI state detail
├── orchestrators_module.puml      # Workflow coordination
├── llm_module.puml                # LLM integration detail
├── design_patterns.puml           # Design patterns reference
├── data_flow.puml                 # Event loop and data flow
├── UML_DIAGRAMS.md               # Complete reference
├── DIAGRAM_QUICK_REFERENCE.md    # This file
├── SCAN_ARCHITECTURE.md          # SCAN algorithm deep dive
└── SCAN_IMPLEMENTATION_PLAN.md   # Implementation guide
```

## Need Help?

- **Understanding a diagram**: Read the inline notes in the .puml file
- **Finding code**: Use the "Quick Lookup" table above
- **Design patterns**: See `design_patterns.puml`
- **Performance**: Check relevant module diagram's performance section
- **Adding features**: See "I want to add..." table above

---

**Total Diagram Coverage**: 3,100+ lines of PlantUML documentation
**Generated**: 2025-11-20
**For**: Infraware Terminal M1 (Terminal Core MVP)
