# Terminal Module - PlantUML Diagrams

## Overview

This directory contains comprehensive UML class diagrams for the terminal module, documenting the latest code structure and design patterns. The diagrams reflect the current implementation of the TUI, state management, event handling, and buffer components.

## Diagram Files

### 1. `terminal-module-overview.puml`
**High-level architecture of all terminal module components**

Shows:
- `TerminalUI`: Main TUI facade wrapping ratatui Terminal
- `TerminalState`: Composite state container with separated buffers
- `OutputBuffer`: Scrollable output with ANSI caching and auto-scroll
- `InputBuffer`: User input with Unicode-safe cursor management
- `CommandHistory`: Command navigation with arrow keys
- `ThrobberAnimator`: Loading animation in dedicated thread
- `EventHandler`: Event mapping from crossterm to custom events
- `TerminalEvent`: Custom event enum
- `TerminalMode`: Terminal state machine enum
- `PendingInteraction`: HITL interaction types
- `ScrollbarInfo`: Scrollbar position and mouse interaction
- `ConfirmationType`: Shell confirmation types (rm -i, cp -i, etc.)

**Key insights:**
- Demonstrates Facade pattern (TerminalUI wraps ratatui)
- Shows Single Responsibility Principle (SRP) composition
- Illustrates separation of concerns in buffer components
- Includes design notes for each class

---

### 2. `terminal-tui-rendering-flow.puml`
**Detailed rendering flow and lifecycle management**

Shows:
- `TerminalUI` lifecycle: new() -> render() -> cleanup()
- `suspend()` and `resume()` for interactive commands
- Event polling pause flag for vim/nano support
- `render_frame()` main entry point
- `render_unified_content()` with 5 phases:
  1. **Phase 1: Build Content Lines** - assembles output, prompt, interaction
  2. **Phase 2: Calculate Scroll** - determines scroll position and max
  3. **Phase 3: Render Content** - creates Paragraph widget with scroll offset
  4. **Phase 4: Render Scrollbar** - draws scrollbar if content exceeds viewport
  5. **Phase 5: Position Cursor** - calculates cursor position on prompt line
- Prompt color mapping by TerminalMode
- Password mode masking

**Key insights:**
- Unified content design: output and prompt in same scrollable area
- Content starts at top, grows downward (Linux shell behavior)
- Smart auto-scroll: only when user was at bottom
- ANSI parsing done once in OutputBuffer (cached)
- Cursor positioning relative to visible viewport

---

### 3. `terminal-state-management.puml`
**Terminal state machine modes and HITL flow**

Shows:
- `TerminalMode` state machine with transitions:
  - Normal (awaiting input)
  - ExecutingCommand (running shell cmd)
  - WaitingLLM (querying backend)
  - PromptingInstall (ask to install - M2/M3)
  - AwaitingCommandApproval (HITL: y/n response)
  - AwaitingAnswer (HITL: free text response)
  - AwaitingMoreInput (multiline input with backslash/heredoc)
- Mode transition triggers (user action, command completion, etc.)
- `PendingInteraction` enum: CommandApproval and Question
- `ConfirmationType`: Shell interactive flags (rm -i, cp -i, etc.)
- Prompt generation with colors per mode
- Root mode transitions (sudo su, su, uid=0)
- Scroll state and ScrollbarInfo
- Input buffer and cursor management
- Command history navigation
- Throbber animation (10 FPS braille symbols)

**Key insights:**
- Clear state machine with documented transitions
- HITL separates approval (y/n) from questions (free text)
- Root mode affects prompt symbol ($ vs #)
- Throbber animation only shown in WaitingLLM mode
- Multiline mode tracks incomplete input (backslash, heredoc)

---

### 4. `terminal-event-handling.puml`
**Event polling, mapping, and context-aware handling**

Shows:
- `EventHandler` abstraction over crossterm
- Key event mapping with Windows compatibility
  - Filters Release/Repeat events (Windows fix)
  - Maps KeyCode + Modifiers -> TerminalEvent
- Mouse event mapping
  - Scroll wheel: ScrollUp/ScrollDown
  - Left button: MouseDown, MouseDrag, MouseUp
  - Right button: ignored (-> Unknown)
- `TerminalEvent` enum variants
- Key bindings reference (Ctrl+C, Enter, arrows, etc.)
- Context-aware handling by TerminalMode:
  - Normal: input, navigation, submit
  - ExecutingCommand: Ctrl+C to cancel
  - WaitingLLM: Ctrl+C to cancel query
  - AwaitingCommandApproval: y/n/Ctrl+C
  - AwaitingAnswer: text input and submit
  - AwaitingMoreInput: continue or cancel
- Platform considerations (Windows, Linux, Mac)
- Mouse scrollbar interaction workflow

**Key insights:**
- Windows-specific filtering prevents duplicate input
- Single abstraction layer (TerminalEvent) for all platforms
- Context-aware interpretation same event, different behavior
- Mouse events integrated for scrollbar drag/click
- Extensible: easy to add new events

---

### 5. `terminal-buffer-components.puml`
**Single Responsibility Principle (SRP) buffer design**

Shows:
- SRP architecture philosophy and benefits
- `OutputBuffer`: Scrolling, buffering, ANSI caching
  - Dual buffer: raw strings + parsed ratatui Lines
  - Smart auto-scroll (only if user at bottom)
  - Memory management (max 10K lines, trims 1K at a time)
  - Scroll position and visible window calculation
  - ANSI parsing cached (O(1) rendering)
- `InputBuffer`: User input with Unicode support
  - Character-based cursor position (not bytes)
  - Multi-byte UTF-8 safe (emoji, CJK)
  - Char <-> Byte conversion helpers
  - Text operations: insert, delete, movement
- `CommandHistory`: Command navigation
  - Arrow up/down navigation
  - Skips empty commands
  - Position tracking (None = new input)
  - Reset on submit
- `ThrobberAnimator`: Loading animation
  - Dedicated thread (non-blocking)
  - Atomic operations (thread-safe)
  - 10 FPS (100ms interval)
  - BRAILLE_DOUBLE symbols
  - Idempotent start() and stop()
- Composition in TerminalState (delegation pattern)
- Data flow through buffers (input -> output)
- Testing strategy and benefits

**Key insights:**
- Each buffer independently testable
- Thin wrapper delegation in TerminalState
- No God objects
- Clear separation of concerns
- Supports code reuse and modifications

---

## Design Patterns

### 1. **Facade Pattern** (TerminalUI)
- `TerminalUI` wraps ratatui Terminal
- Hides complexity of raw mode, alternate screen, mouse capture
- Provides simple methods: new(), render(), cleanup(), suspend(), resume()

### 2. **Composite Pattern** (TerminalState)
- Composes four buffer components (Output, Input, History, Throbber)
- Provides unified interface through delegation
- Each component handles one responsibility

### 3. **State Machine Pattern** (TerminalMode)
- Clear state transitions documented
- Mode determines behavior in event handling
- Prompt colors change per mode

### 4. **Strategy Pattern** (ConfirmationType)
- Different confirmation strategies for different commands (rm -i, cp -i, etc.)
- Used in PendingInteraction for flexible approval flow

### 5. **Single Responsibility Principle (SRP)**
- OutputBuffer: scrolling only
- InputBuffer: input handling only
- CommandHistory: navigation only
- ThrobberAnimator: animation only
- Clear boundaries, easy testing

### 6. **Observer Pattern** (Event Handler)
- EventHandler polls and reports events
- Main loop responds to events
- Decoupled event source from consumers

---

## Key Architectural Decisions

### 1. **Unified Content Rendering**
- Output and prompt in same scrollable area
- Content starts at top, grows downward (Linux shell style)
- No separate fixed prompt bar

### 2. **Smart Auto-Scroll**
- Only auto-scrolls if user was at bottom
- Preserves scroll position if user scrolled up
- Prevents jarring output jumps

### 3. **ANSI Parsing Optimization**
- Parse ANSI codes once when adding output
- Cache in parsed_buffer (ratatui Lines)
- Rendering uses cached parse (no re-parsing)
- O(1) rendering instead of O(N²)

### 4. **Event Polling Pause Flag**
- Used for interactive commands (vim, nano, less)
- Prevents event poller from stealing input
- TUI suspend/resume with RAII guard

### 5. **Throbber Animation Thread**
- Dedicated background thread for animation
- Non-blocking (doesn't affect event loop)
- Atomic operations for thread safety
- 10 FPS (100ms interval) for smooth animation

### 6. **Character-Based Cursor Position**
- InputBuffer tracks cursor in characters, not bytes
- Handles multi-byte UTF-8 (emoji, CJK) correctly
- Byte conversion internal to InputBuffer

### 7. **Root Mode Tracking**
- Entered via "sudo su", "su", or uid=0 check
- Changes prompt symbol: $ -> #
- Propagates to prompt generation

---

## SOLID Principles Application

### Single Responsibility (S)
- Each buffer class has one job
- OutputBuffer doesn't handle input
- InputBuffer doesn't handle output
- History doesn't handle scrolling

### Open/Closed (O)
- ThrobberAnimator can use different symbol sets
- TerminalMode extensible for new modes
- EventHandler mappable to new events

### Liskov Substitution (L)
- Buffer classes follow contracts
- Could replace with alternative implementations

### Interface Segregation (I)
- Only expose needed methods per class
- Don't force unwanted methods
- Focused public APIs

### Dependency Inversion (D)
- TerminalState depends on abstractions (public APIs)
- Not on implementation details
- Composition over inheritance

---

## Testing Strategy

The SRP design enables focused unit testing:

**OutputBuffer Tests:**
- add_line(), add_lines()
- scroll_up(), scroll_down(), scroll_to_end()
- Auto-scroll behavior
- Trim when exceeds max
- is_at_bottom() detection
- visible_window() slicing
- pop(), clear()

**InputBuffer Tests:**
- insert_char(), delete_char()
- move_cursor_left(), move_cursor_right()
- Unicode handling (emoji, CJK)
- take(), clear(), set_text()

**CommandHistory Tests:**
- add(), previous(), next()
- Position tracking
- Empty command skipping
- reset_position()

**ThrobberAnimator Tests:**
- start(), stop()
- is_running()
- symbol() cycling
- Thread safety with Arc/Atomic

**TerminalState Integration Tests:**
- Mode transitions
- HITL interactions
- Multiline input
- Root mode

---

## File Structure

```
src/terminal/
├── mod.rs                 # Module exports
├── tui.rs                 # TerminalUI (Facade)
├── state.rs               # TerminalState (Composite)
├── buffers.rs             # OutputBuffer, InputBuffer, CommandHistory
├── events.rs              # EventHandler, TerminalEvent
├── throbber.rs            # ThrobberAnimator
└── splash.rs              # SplashScreen

docs/uml/
├── terminal-module-overview.puml          # This diagram
├── terminal-tui-rendering-flow.puml       # Rendering flow
├── terminal-state-management.puml         # State machine
├── terminal-event-handling.puml           # Event processing
├── terminal-buffer-components.puml        # SRP design
└── TERMINAL_MODULE_DIAGRAMS.md            # This file
```

---

## Related Documentation

- `CLAUDE.md` - Project guidelines and architecture
- `docs/SCROLLING_ARCHITECTURE.md` - Detailed scrolling design
- `docs/INTERACTIVE_COMMANDS_ARCHITECTURE.md` - Interactive command handling
- `docs/SCAN_ARCHITECTURE.md` - SCAN input classification algorithm

---

## How to Update Diagrams

1. Modify code in `src/terminal/`
2. Update corresponding `.puml` file
3. Verify PlantUML syntax with online renderer
4. Commit diagrams with code changes
5. Update this README if architecture changes

---

## Viewing Diagrams

PlantUML files can be viewed with:
- Online: http://www.plantuml.com/plantuml/uml/
- VS Code: PlantUML extension
- IntelliJ: Built-in PlantUML plugin
- CLI: `plantuml *.puml`

---

## Summary

The terminal module demonstrates clean architecture with clear separation of concerns, SOLID principles, and design patterns. The diagrams document:

- How components relate and interact
- The data flow through the system
- State machine transitions
- Event handling pipeline
- Rendering flow and optimization
- Buffer responsibilities (SRP)

This makes the code easier to understand, test, modify, and extend.
