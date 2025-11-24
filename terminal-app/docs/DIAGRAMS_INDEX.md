# Interactive Commands Architecture - Diagrams Index

This document provides an overview of the PlantUML diagrams that visualize the interactive command architecture for Infraware Terminal.

## Quick Navigation

| Diagram | File | Purpose | Audience |
|---------|------|---------|----------|
| **Flow Diagram** | `interactive_command_flow.puml` | Complete user input → execution flow | Everyone |
| **Class Diagram** | `command_executor.puml` | CommandExecutor structure and methods | Developers |
| **State Diagram** | `terminal_ui_architecture.puml` | TUI state management and suspend/resume | Developers |
| **Decision Tree** | `command_execution_decision_tree.puml` | CommandOrchestrator decision logic | Developers |
| **Comparison** | `interactive_vs_noninteractive.puml` | Interactive vs non-interactive paths | Developers |
| **Orchestration** | `orchestration_architecture.puml` | Orchestrator coordination and SRP | Architects |

---

## 1. Interactive Command Flow (interactive_command_flow.puml)

**Type**: Sequence Diagram
**Size**: 3.7 KB
**Best for**: Understanding the complete flow from user input to command execution

### What It Shows

1. **Non-Interactive Command Flow**:
   - Input classification
   - 9-handler chain execution
   - Command execution with output capture
   - Output rendering to TUI

2. **Interactive Command Flow**:
   - Input classification
   - TUI suspension (show_cursor, flush, leave alternate screen, disable raw mode)
   - Command execution via spawn_blocking (unguarded process)
   - TUI resumption (enable raw mode, enter alternate screen, clear)
   - Output display

### Key Insights

- Both flows follow the same classification → orchestrator → executor pattern
- TUI suspension enables full terminal access for interactive commands
- No output is captured for interactive commands (by design)
- RAII Guard ensures TUI is always restored

### Use Cases

- Onboarding new developers
- Understanding how user input flows through the system
- Explaining why interactive commands need TUI suspension
- Training material for architecture reviews

---

## 2. Command Executor (command_executor.puml)

**Type**: Class Diagram
**Size**: 5.5 KB
**Best for**: Understanding CommandExecutor's structure and methods

### What It Shows

1. **CommandOutput Structure**:
   - stdout, stderr, exit_code fields
   - is_success() and combined_output() methods

2. **CommandExecutor Static Methods**:
   - execute() - Non-interactive execution
   - execute_interactive() - Interactive execution
   - requires_interactive() - Interactivity check
   - command_exists() - PATH verification

3. **Command Classification**:
   - REQUIRES_INTERACTIVE list (vim, less, top, etc.)
   - INTERACTIVE_BLOCKED list (ssh, python, node, etc.)
   - ALL_INTERACTIVE set (union of both)

4. **TerminalUI Integration**:
   - suspend() and resume() methods
   - RAII Guard pattern

5. **ShellBuiltinHandler Integration**:
   - Shell builtin recognition (45+ builtins)
   - Windows-specific handling

### Key Insights

- All execution goes through CommandExecutor (single point of control)
- Interactivity check is O(1) HashSet lookup
- Platform-specific code is clearly marked
- RAII Guard ensures cleanup even on panic

### Use Cases

- Code review preparation
- Understanding execution paths
- Modifying execution logic
- Adding new commands to whitelist

---

## 3. Terminal UI Architecture (terminal_ui_architecture.puml)

**Type**: State Diagram with Component Structure
**Size**: 6.3 KB
**Best for**: Understanding TUI state management and suspend/resume mechanics

### What It Shows

1. **TerminalUI Class**:
   - new(), render(), clear(), cleanup()
   - suspend() and resume() methods with detailed steps
   - Lifecycle management

2. **TerminalState Components** (SRP-compliant):
   - OutputBuffer (scrollable output, max 10k lines)
   - InputBuffer (user input with cursor)
   - CommandHistory (bidirectional navigation)
   - TerminalMode (Normal/ExecutingCommand/WaitingLLM/PromptingInstall)

3. **Suspend/Resume Flow**:
   - Detailed steps for prepare terminal
   - Detailed steps for restore terminal
   - Both shown side-by-side for comparison

4. **Crossterm/Ratatui Integration**:
   - CrosstermBackend for terminal control
   - RataTuiTerminal for rendering

### Key Insights

- SRP applied to TerminalState (3 separate buffer components)
- Suspend and resume are symmetric operations
- Each step is necessary for correct behavior
- Platform-specific through crossterm abstraction

### Use Cases

- Understanding TUI initialization
- Learning buffer component architecture
- Debugging suspend/resume issues
- Understanding Unicode handling (InputBuffer uses char count)

---

## 4. Command Execution Decision Tree (command_execution_decision_tree.puml)

**Type**: Activity Diagram
**Size**: 3.1 KB
**Best for**: Understanding CommandOrchestrator's decision logic

### What It Shows

1. **Built-in Command Handling**:
   - clear: Clear output and terminal
   - reload-aliases: Load system aliases asynchronously

2. **Command Existence Verification**:
   - Skip check for shell builtins
   - Skip check for history expansion
   - Skip check for shell operators
   - Provide user-friendly error if missing

3. **Interactivity Decision**:
   - If requires_interactive() → execute_interactive()
   - Else → execute() with output capture

4. **Output Handling**:
   - Capture stdout and stderr
   - Colorize stderr if command failed
   - Benign failure detection (exit 1 vs exit 2+)

### Key Insights

- Built-in commands are handled before executor (they affect TUI state)
- Early validation prevents unnecessary work
- Exit code 1 is semantic (benign) vs exit 2+ (real error)
- Shell operators are handled specially (pass to shell via original_input)

### Use Cases

- Understanding error messages
- Debugging command execution issues
- Adding new built-in commands
- Understanding exit code handling

---

## 5. Interactive vs Non-Interactive (interactive_vs_noninteractive.puml)

**Type**: Comparison Diagram with Detailed Flows
**Size**: 7.6 KB
**Best for**: Understanding the critical differences between execution modes

### What It Shows

1. **Non-Interactive Path**:
   - INTERACTIVE_BLOCKED check
   - Execution path selection (builtin, operators, direct)
   - Output capture and handling
   - Display logic (stdout/stderr formatting)

2. **Interactive Path**:
   - RAII Guard pattern
   - suspend() with 4 detailed steps
   - spawn_blocking() for actual command
   - resume() with 3 detailed steps
   - Platform-specific Windows handling

3. **Detailed Substeps**:
   - suspend_details: show_cursor, flush, LeaveAlternateScreen, disable_raw_mode
   - resume_details: enable_raw_mode, EnterAlternateScreen, clear
   - use_cases: vim, less, man, mc, top, sudo

### Key Insights

- Both paths go through CommandExecutor but with different strategies
- suspend() and resume() are mirrors of each other
- Platform check happens late (after classification)
- RAII Guard is essential for safety

### Use Cases

- Decision-making for new command classification
- Understanding why certain commands are blocked
- Learning about TUI suspension mechanics
- Training on platform-specific handling

---

## 6. Orchestration Architecture (orchestration_architecture.puml)

**Type**: Component Diagram with Responsibilities
**Size**: 9.3 KB
**Best for**: Understanding the overall architecture and SRP

### What It Shows

1. **InfrawareTerminal** (Event Loop Orchestrator):
   - Manages event loop
   - Routes events to handlers
   - Coordinates between UI and orchestrators
   - Builder pattern for construction

2. **CommandOrchestrator**:
   - Command execution workflow
   - Built-in command handling
   - Interactive/non-interactive routing
   - Output formatting and display

3. **NaturalLanguageOrchestrator**:
   - LLM query workflow
   - Abstract LLMClientTrait (pluggable)
   - ResponseRenderer for formatting
   - Markdown rendering (basic, M1 scope)

4. **TabCompletionHandler**:
   - Tab completion workflow
   - Command name completion
   - File path completion
   - History matching

5. **InputClassifier**:
   - Chain of Responsibility (9 handlers)
   - Alias expansion (pre-classification)
   - InputType determination

6. **Workflow Routing**:
   - Command workflow
   - Natural language workflow
   - Tab completion workflow
   - Main event loop

### Key Insights

- Single Responsibility Principle applied throughout
- Each orchestrator handles one workflow
- Builder pattern enables testability
- Composition over inheritance
- Clear separation of concerns

### Use Cases

- Architecture reviews
- Understanding high-level design
- Adding new workflows
- System documentation

---

## How to Use These Diagrams

### For Code Reviews
1. Start with **Orchestration Architecture** for overall structure
2. Dive into **Command Executor** for implementation details
3. Reference **Decision Tree** for logic flow
4. Use **Interactive vs Non-Interactive** for edge cases

### For Onboarding
1. Begin with **Interactive Command Flow** for complete picture
2. Study **Terminal UI Architecture** for state management
3. Review **Orchestration Architecture** for responsibilities
4. Deep-dive into specific diagram for area of interest

### For Troubleshooting
1. **Command Execution Decision Tree** - Is logic correct?
2. **Interactive vs Non-Interactive** - Which path is taken?
3. **Terminal UI Architecture** - Is state correct?
4. **Command Executor** - Is method called correctly?

### For Documentation
1. Use **Interactive Command Flow** in README
2. Reference **Orchestration Architecture** in design docs
3. Include **Interactive vs Non-Interactive** in API docs
4. Link **Command Executor** in method documentation

---

## Viewing the Diagrams

### PlantUML Viewing Options

1. **Online Viewers**:
   - PlantUML Online: https://www.plantuml.com/plantuml/uml/
   - Visual Studio Code (with PlantUML extension)
   - JetBrains IDEs (built-in support)

2. **Local Viewing**:
   ```bash
   # Install PlantUML
   brew install plantuml  # macOS
   sudo apt install plantuml  # Ubuntu

   # Generate PNG
   plantuml docs/interactive_command_flow.puml

   # View result
   open docs/interactive_command_flow.png  # macOS
   xdg-open docs/interactive_command_flow.png  # Linux
   ```

3. **VS Code Extension**:
   - Install "PlantUML" extension
   - Right-click .puml file → "Preview PlantUML"
   - Supports auto-preview on save

---

## Related Documentation

- **INTERACTIVE_COMMANDS_ARCHITECTURE.md** - Comprehensive architecture documentation
- **SCAN_ARCHITECTURE.md** - Input classification chain details
- **CLAUDE.md** - Project guidelines and constraints
- **Source Code**:
  - `/home/crist/infraware-terminal/terminal-app/src/executor/command.rs` - CommandExecutor
  - `/home/crist/infraware-terminal/terminal-app/src/orchestrators/command.rs` - CommandOrchestrator
  - `/home/crist/infraware-terminal/terminal-app/src/terminal/tui.rs` - TerminalUI
  - `/home/crist/infraware-terminal/terminal-app/src/main.rs` - InfrawareTerminal

---

## Diagram Maintenance

These diagrams are auto-documented in the source code. When modifying the following, remember to update the corresponding diagrams:

| Change | Diagrams to Update |
|--------|-------------------|
| Add/remove REQUIRES_INTERACTIVE command | command_executor.puml, command_execution_decision_tree.puml |
| Add/remove INTERACTIVE_BLOCKED command | interactive_vs_noninteractive.puml, command_executor.puml |
| Modify suspend() or resume() steps | terminal_ui_architecture.puml, interactive_vs_noninteractive.puml |
| Change CommandOrchestrator logic | command_execution_decision_tree.puml, orchestration_architecture.puml |
| Add new orchestrator | orchestration_architecture.puml |
| Modify InputClassifier chain | orchestration_architecture.puml |

---

## Summary

These six diagrams provide a complete view of the interactive command architecture:

1. **Flow Diagram** - "What is the complete user journey?"
2. **Class Diagram** - "What are the main components?"
3. **State Diagram** - "How is state managed?"
4. **Decision Tree** - "What decisions are made?"
5. **Comparison** - "What's the difference between modes?"
6. **Orchestration** - "How do components work together?"

Together, they form a comprehensive visual reference for understanding, implementing, and maintaining the interactive command system.
