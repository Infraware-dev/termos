# Infraware Terminal Documentation Index

## Documentation Structure

```
docs/
├── INDEX.md                              # This file - navigation guide
├── SUPPORTED_COMMANDS.md                 # All commands with test references
├── SCAN_ARCHITECTURE.md                  # SCAN algorithm deep dive
├── INTERACTIVE_COMMANDS_ARCHITECTURE.md  # Interactive command handling
├── SCROLLING_ARCHITECTURE.md             # Output scrolling implementation
├── design-patterns/                      # Design pattern documentation
│   ├── design-patterns.md                # Design patterns deep dive
│   └── chain-of-responsibility.md        # CoR pattern example
└── uml/                                  # All UML diagrams
    ├── TERMINAL_MODULE_DIAGRAMS.md       # Terminal module documentation
    ├── THROBBER_ANIMATION_DIAGRAMS.md    # Throbber animation documentation
    ├── BACKGROUND_PROCESSES_DIAGRAMS.md  # Background processes documentation
    └── *.puml                            # PlantUML diagram files
```

## Quick Navigation

### Understanding the System
1. View `uml/00-main-application-architecture.puml`
2. View `uml/07-data-flow-pipeline.puml`
3. Choose a module diagram

### Core Documentation

| Document | Description |
|----------|-------------|
| [SUPPORTED_COMMANDS.md](SUPPORTED_COMMANDS.md) | All supported commands with test references |
| [SCAN_ARCHITECTURE.md](SCAN_ARCHITECTURE.md) | Complete SCAN algorithm explanation |
| [INTERACTIVE_COMMANDS_ARCHITECTURE.md](INTERACTIVE_COMMANDS_ARCHITECTURE.md) | TUI suspend/resume for interactive commands |
| [SCROLLING_ARCHITECTURE.md](SCROLLING_ARCHITECTURE.md) | Output buffer scrolling with visual scrollbar |
| [design-patterns.md](design-patterns/design-patterns.md) | Design patterns used in the codebase |

### UML Documentation

| Document | Description |
|----------|-------------|
| [TERMINAL_MODULE_DIAGRAMS.md](uml/TERMINAL_MODULE_DIAGRAMS.md) | Terminal module architecture diagrams |
| [THROBBER_ANIMATION_DIAGRAMS.md](uml/THROBBER_ANIMATION_DIAGRAMS.md) | Throbber animation system diagrams |
| [BACKGROUND_PROCESSES_DIAGRAMS.md](uml/BACKGROUND_PROCESSES_DIAGRAMS.md) | Background process support diagrams |

### Key Diagrams

| Diagram | Purpose |
|---------|---------|
| `uml/00-main-application-architecture.puml` | System overview |
| `uml/01-scan-algorithm-10-handlers.puml` | SCAN classification chain |
| `uml/03-executor-module.puml` | Command execution |
| `uml/04-terminal-state-and-buffers.puml` | TUI state management |
| `uml/05-orchestrators.puml` | Workflow coordination |
| `uml/06-complete-class-diagram.puml` | Full class diagram |
| `uml/terminal-module-overview.puml` | Terminal module components |

## How to View Diagrams

### Option 1: Online Editor (Easiest)
1. Visit https://www.plantuml.com/plantuml/uml/
2. Copy contents of any `.puml` file
3. Paste into editor and view

### Option 2: VS Code Extension
```bash
code --install-extension jebbs.plantuml
# Open any .puml file, press Alt+D to preview
```

### Option 3: Local Installation
```bash
# Linux
sudo apt install plantuml

# Generate SVG
plantuml -tsvg docs/uml/00-main-application-architecture.puml
```

## By Topic

### SCAN Algorithm
1. Read `SCAN_ARCHITECTURE.md`
2. View `uml/01-scan-algorithm-10-handlers.puml`
3. Study `src/input/handler.rs`

### Command Execution
1. View `uml/03-executor-module.puml`
2. View `uml/10-executor-module-with-background-support.puml`
3. Study `src/executor/command.rs`

### Interactive Commands
1. Read `INTERACTIVE_COMMANDS_ARCHITECTURE.md`
2. View `uml/interactive_command_flow.puml`
3. Study `src/terminal/tui.rs` (suspend/resume)

### Background Processes
1. Read `uml/BACKGROUND_PROCESSES_DIAGRAMS.md`
2. View `uml/08-job-manager-class-diagram.puml`
3. View `uml/09-background-command-execution-sequence.puml`
4. Study `src/executor/job_manager.rs`

### Terminal UI & Scrolling
1. Read `SCROLLING_ARCHITECTURE.md`
2. Read `uml/TERMINAL_MODULE_DIAGRAMS.md`
3. View `uml/terminal-module-overview.puml`
4. Study `src/terminal/buffers.rs`

### Throbber Animation
1. Read `uml/THROBBER_ANIMATION_DIAGRAMS.md`
2. View `uml/throbber-animator-class-diagram.puml`
3. Study `src/terminal/throbber.rs`

### Design Patterns
1. Read `design-patterns/design-patterns.md`
2. View `uml/design_patterns.puml`
3. See `design-patterns/chain-of-responsibility.md`
