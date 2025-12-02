# UML Diagram Update Summary: Terminal Scrolling Implementation

**Date**: December 2, 2025
**Scope**: Update UML diagrams to reflect recent terminal scrolling refactoring

## Overview

This document summarizes the UML diagram updates that reflect the terminal scrolling implementation improvements. The changes introduce proper visible lines management, better scroll bounds checking, and mutable state handling in the render pipeline.

## Files Updated

### 1. `/home/crist/infraware-terminal/terminal-app/docs/uml/04-terminal-state-and-buffers.puml`

**Primary diagram for terminal state architecture**

#### Key Changes to `TerminalState` class:

```plaintext
BEFORE:
+ scroll_down(): void

AFTER:
- visible_lines: usize (new private field)
+ set_visible_lines(lines: usize): void (new method)
+ visible_lines() -> usize (new getter)
+ pending_interaction: Option<PendingInteraction> (added to field list)
```

**Impact**: TerminalState now manages viewport height, which is critical for proper scroll boundary calculations.

#### Key Changes to `OutputBuffer` class:

```plaintext
BEFORE:
- scroll_offset: usize
- max_lines: usize
+ scroll_down(): void

AFTER:
- buffer: Vec<String>
- scroll_position: usize
+ scroll_down(visible_lines: usize): void (parameter added)
+ set_visible_lines(visible_lines: usize): void (new method)
+ pop(): Option<String> (new method)
```

**Rationale**:
- `scroll_down()` now takes `visible_lines` parameter for proper bounds checking
- `set_visible_lines()` ensures scroll position is clamped when terminal resizes
- Methods use accurate naming (`buffer`, `scroll_position`) matching implementation

#### Key Changes to `TerminalUI` class:

```plaintext
BEFORE:
+ render(state): Result<()>

AFTER:
+ render(&mut state): Result<()>
```

**Why**: The render method now mutates state to update `visible_lines` based on actual terminal size before rendering.

#### Key Changes to `EventHandler` class:

```plaintext
BEFORE:
- events: crossterm::event::EventReader

AFTER:
(removed unnecessary field detail)
- map_event(event): TerminalEvent (new private method)
- map_key_event(key): TerminalEvent (new private method)
```

**Enhancements to `TerminalEvent` enum**:

```plaintext
ADDED:
+ CtrlC (context-aware Ctrl+C)
+ ScrollUp with Ctrl+Up mapping
+ ScrollDown with Ctrl+Down mapping
+ PageUp/PageDown mappings
```

**Event Mapping Documentation**:
- `Key::Up + Ctrl` → `ScrollUp` (laptop-friendly scroll)
- `Key::Down + Ctrl` → `ScrollDown` (laptop-friendly scroll)
- `Key::PageUp` → `ScrollUp` (traditional scroll)
- `Key::PageDown` → `ScrollDown` (traditional scroll)
- Unmodified Up/Down still map to History navigation

#### Updated Notes Section

Added detailed explanation of scrolling implementation:

```
Scrolling Implementation
- Keyboard Events: Ctrl+Up/Down or PageUp/Down → ScrollUp/Down
- Handler: InfrawareTerminal calls state.scroll_up/down()
- TerminalState: Delegates to OutputBuffer.scroll_down(visible_lines)
- OutputBuffer: Uses visible_lines for bounds checking
- Render: Updates visible_lines from terminal size
```

---

### 2. `/home/crist/infraware-terminal/terminal-app/docs/uml/00-main-application-architecture.puml`

**Main application architecture overview**

#### Updated Note for `InfrawareTerminal`:

Added scrolling context to class responsibilities:

```plaintext
**Event Handling**
- ScrollUp/ScrollDown events
- Calls state.scroll_up()/scroll_down()
- TerminalState manages visible_lines
- OutputBuffer uses visible_lines for bounds
```

---

### 3. `/home/crist/infraware-terminal/terminal-app/docs/uml/06-complete-class-diagram.puml`

**Complete system class diagram with all components**

#### Terminal UI & State Section Updates:

**`TerminalUI` class**:
```plaintext
BEFORE:
+ render(state): Result<()>

AFTER:
+ render(&mut state): Result<()>
```

**`TerminalState` class** - Added fields:
```plaintext
+ pending_interaction: Option<PendingInteraction>
- visible_lines: usize
```

**`TerminalState` class** - Added methods:
```plaintext
+ scroll_up(): void
+ scroll_down(): void
+ set_visible_lines(lines): void
+ visible_lines() -> usize
```

**`OutputBuffer` class** - Updated signature:
```plaintext
BEFORE:
+ scroll_up()
+ scroll_down()

AFTER:
+ scroll_up(): void
+ scroll_down(visible_lines): void
+ set_visible_lines(visible_lines): void
+ pop(): Option<String>
```

**`TerminalMode` enum** - Added modes:
```plaintext
+ AwaitingCommandApproval (HITL approval y/n)
+ AwaitingAnswer (HITL free-form answer)
```

**`TerminalEvent` enum** - Added events:
```plaintext
+ DeleteChar
+ MoveCursorLeft
+ MoveCursorRight
+ ClearScreen
+ CtrlC
+ ScrollUp
+ ScrollDown
+ Resize(u16, u16)
```

**Relationship update**:
```plaintext
BEFORE:
TerminalUI --> TerminalState : renders

AFTER:
TerminalUI --> TerminalState : renders (&mut)
```

---

## Design Improvements Reflected

### 1. **Proper Viewport Height Management**

- `TerminalState.visible_lines` stores the calculated viewport height
- Updated during each render based on actual terminal size
- Passed to `OutputBuffer` for accurate scroll boundary calculations

### 2. **Bounds-Safe Scrolling**

- `OutputBuffer.scroll_down(visible_lines)` now receives context it needs
- Calculation: `max_scroll = buffer.len().saturating_sub(visible_lines)`
- Prevents scrolling past the bottom of the buffer

### 3. **Mutable State in Render**

- `render(&mut TerminalState)` signature documents that state is modified
- Clearly indicates `visible_lines` is set during rendering
- Better API clarity for callers

### 4. **Event-Driven Scrolling**

- Keyboard shortcuts properly documented:
  - `Ctrl+Up/Down` for laptops without Page keys
  - `PageUp/PageDown` for traditional keyboards
  - Clear distinction from history navigation (unmodified Up/Down)

### 5. **HITL State Management**

- `pending_interaction` field documents human-in-the-loop interactions
- New terminal modes for command approval and free-form questions
- Architecture supports complex LLM workflows

---

## Implementation Details Documented

### Scrolling Flow

```
User presses Ctrl+Down
    ↓
EventHandler generates ScrollDown event
    ↓
InfrawareTerminal.handle_event() receives ScrollDown
    ↓
Calls state.scroll_down()
    ↓
TerminalState delegates to output.scroll_down(visible_lines)
    ↓
OutputBuffer.scroll_down():
  1. Calculate max_scroll = buffer.len() - visible_lines
  2. Clamp scroll_position < max_scroll
  3. Increment scroll_position
    ↓
TerminalUI.render() called
    ↓
Calculates visible_lines from terminal size
    ↓
Calls state.set_visible_lines(lines)
    ↓
Renders output buffer with scrolled content
```

### Memory Management in OutputBuffer

- `MAX_OUTPUT_LINES: 10,000` - prevents excessive memory usage
- `TRIM_LINES: 1,000` - headroom during trim to reduce frequency
- `scroll_position` adjusted when buffer trimmed
- `auto_scroll_to_bottom()` sets position to `buffer.len()` for new output

---

## Testing & Validation

The updated diagrams accurately represent the current codebase:

- **Source file**: `/home/crist/infraware-terminal/terminal-app/src/terminal/state.rs`
- **Source file**: `/home/crist/infraware-terminal/terminal-app/src/terminal/buffers.rs`
- **Source file**: `/home/crist/infraware-terminal/terminal-app/src/terminal/tui.rs`
- **Source file**: `/home/crist/infraware-terminal/terminal-app/src/terminal/events.rs`

All method signatures, field types, and relationships have been verified against the implementation.

---

## Performance Implications (Documented)

### Scrolling Operations

- **scroll_up()**: O(1) - single decrement with bounds check
- **scroll_down(visible_lines)**: O(1) - single increment with calculation
- **set_visible_lines()**: O(1) - clamp operation only
- **Render**: O(visible_lines) - only renders viewport height lines

### Buffer Operations

- **add_line()**: O(1) amortized - append + auto-scroll
- **trim_if_needed()**: O(n) - only when buffer exceeds 10,000 lines
- **pop()**: O(1) - single removal with scroll adjustment

---

## Future Diagram Enhancements (Notes)

Potential areas for additional detail in future updates:

1. **Sequence Diagrams**: Detailed scroll event handling sequence
2. **Activity Diagrams**: Viewport calculation and rendering lifecycle
3. **State Diagrams**: Terminal mode transitions including HITL states
4. **Component Diagram**: Module interactions (terminal, input, executor, orchestrators)

---

## Verification Commands

To regenerate diagrams from PlantUML sources (if rendering tools available):

```bash
# Check PlantUML syntax
plantuml -syntax docs/uml/04-terminal-state-and-buffers.puml
plantuml -syntax docs/uml/00-main-application-architecture.puml
plantuml -syntax docs/uml/06-complete-class-diagram.puml

# Generate PNG/SVG visualizations
plantuml docs/uml/04-terminal-state-and-buffers.puml -o output/
plantuml docs/uml/00-main-application-architecture.puml -o output/
plantuml docs/uml/06-complete-class-diagram.puml -o output/
```

---

## Summary

All three core UML diagrams have been updated to accurately reflect the terminal scrolling implementation. The changes emphasize:

1. **Proper viewport management** via `visible_lines` in TerminalState
2. **Safe scrolling** via bounds-checked `scroll_down(visible_lines)`
3. **Mutable state handling** in the render pipeline
4. **Event mapping clarity** for keyboard shortcuts
5. **HITL workflow support** with new terminal modes

The diagrams now serve as accurate documentation of the current codebase architecture and can be used for onboarding, design discussions, and future feature planning.
