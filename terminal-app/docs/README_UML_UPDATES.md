# UML Documentation Update - December 2, 2025

## Overview

All UML diagrams have been updated to reflect the recent terminal scrolling implementation improvements. This document serves as an index to the updated diagrams and supporting documentation.

## Updated UML Diagrams

### 1. Terminal State and Buffers Diagram
**File**: `/home/crist/infraware-terminal/terminal-app/docs/uml/04-terminal-state-and-buffers.puml`

**Purpose**: Detailed class diagram showing terminal state management, output/input buffers, event handling, and rendering architecture.

**Key Updates**:
- Added `visible_lines` field to TerminalState
- Added `set_visible_lines()` and `visible_lines()` getter to TerminalState
- Updated `OutputBuffer.scroll_down()` to take `visible_lines` parameter
- Added `set_visible_lines()` to OutputBuffer
- Updated `TerminalUI.render()` signature to `&mut TerminalState`
- Enhanced EventHandler documentation with keyboard event mappings
- Added Ctrl+Up/Down, PageUp/PageDown event documentation
- Updated data flow notes with scrolling details

**Related Classes**:
- TerminalState (core state manager)
- OutputBuffer (scrollable display)
- InputBuffer (user input)
- CommandHistory (command navigation)
- TerminalUI (rendering engine)
- EventHandler (input processing)
- TerminalEvent (custom events)
- TerminalMode (application state)

**View This Diagram When**: Understanding state management, buffer interactions, event flow, and rendering pipeline.

---

### 2. Main Application Architecture
**File**: `/home/crist/infraware-terminal/terminal-app/docs/uml/00-main-application-architecture.puml`

**Purpose**: High-level architecture showing main application structure, builder pattern, and component relationships.

**Key Updates**:
- Added scrolling event handling to InfrawareTerminal responsibilities
- Documented visible_lines management in event handling notes
- Added reference to OutputBuffer bounds checking

**Related Classes**:
- InfrawareTerminal (main event loop)
- InfrawareTerminalBuilder (builder pattern)
- All major orchestrators and handlers

**View This Diagram When**: Understanding application structure, main component relationships, and high-level architecture.

---

### 3. Complete System Class Diagram
**File**: `/home/crist/infraware-terminal/terminal-app/docs/uml/06-complete-class-diagram.puml`

**Purpose**: Comprehensive class diagram showing all major components and their relationships.

**Key Updates**:
- Added `visible_lines` field and methods to TerminalState
- Updated OutputBuffer signatures for scroll operations
- Added `pending_interaction` field to TerminalState
- Added HITL modes to TerminalMode enum (AwaitingCommandApproval, AwaitingAnswer)
- Expanded TerminalEvent enum with all keyboard events
- Changed TerminalUI relationship to show mutable state parameter

**View This Diagram When**: Need complete system overview or designing new components.

---

## Supporting Documentation

### 1. UML Update Summary
**File**: `/home/crist/infraware-terminal/terminal-app/docs/UML_UPDATE_SUMMARY.md`

**Contains**:
- Detailed before/after comparisons for each change
- Design improvements explanation
- Implementation details documentation
- Testing & validation information
- Performance implications
- Future enhancement suggestions

**Read This For**: Understanding why each change was made and implications.

---

### 2. Scrolling Architecture Guide
**File**: `/home/crist/infraware-terminal/terminal-app/docs/SCROLLING_ARCHITECTURE.md`

**Contains**:
- Core component overview with visual diagrams
- Event flow architecture
- Rendering pipeline details
- Scrolling semantics and behaviors
- Window resize handling
- Auto-scroll behavior explanation
- Memory management in OutputBuffer
- Field lifecycle documentation
- Key invariants and edge cases
- Testing scenarios
- Performance characteristics table
- Related code locations

**Read This For**: Deep understanding of scrolling implementation, testing, or troubleshooting.

---

### 3. Quick Reference
**File**: `/home/crist/infraware-terminal/terminal-app/docs/QUICK_REFERENCE_SCROLLING.md`

**Contains**:
- At-a-glance overview
- Three main changes summary
- Control flow diagram
- Method signatures
- Event mapping table
- Buffer state examples
- Terminal resize handling
- Auto-scroll logic
- Important constants
- Testing checklist
- File references

**Read This For**: Quick lookup of specific implementation details.

---

## Source Code References

### Terminal Module Files

| File | Purpose | Key Components |
|------|---------|-----------------|
| `src/terminal/state.rs` | State management | TerminalState, TerminalMode, PendingInteraction |
| `src/terminal/buffers.rs` | Buffer components | OutputBuffer, InputBuffer, CommandHistory |
| `src/terminal/tui.rs` | TUI rendering | TerminalUI, render pipeline |
| `src/terminal/events.rs` | Event handling | EventHandler, TerminalEvent |

### Key Methods

| Method | Location | Signature |
|--------|----------|-----------|
| `set_visible_lines()` | TerminalState | `pub fn set_visible_lines(&mut self, lines: usize)` |
| `visible_lines()` | TerminalState | `pub const fn visible_lines(&self) -> usize` |
| `scroll_down()` | TerminalState | `pub fn scroll_down(&mut self)` |
| `scroll_down()` | OutputBuffer | `pub fn scroll_down(&mut self, visible_lines: usize)` |
| `set_visible_lines()` | OutputBuffer | `pub fn set_visible_lines(&mut self, visible_lines: usize)` |
| `render()` | TerminalUI | `pub fn render(&mut self, state: &mut TerminalState) -> Result<()>` |

---

## How the Scrolling Works

### Step-by-Step Example: Ctrl+Down Press

1. **Input Detection**: EventHandler detects Ctrl+Down key press
2. **Event Generation**: Maps to `TerminalEvent::ScrollDown`
3. **Event Routing**: InfrawareTerminal receives ScrollDown event
4. **State Update**: Calls `state.scroll_down()`
5. **Delegation**: TerminalState calls `output.scroll_down(visible_lines)`
6. **Bounds Check**: OutputBuffer calculates max_scroll and increments position
7. **Render**: Next frame calls `render(&mut state)`
8. **Dimension Update**: Render calculates visible_lines from terminal size
9. **Display**: Renders portion of buffer from scroll_position

---

## Key Architectural Changes

### Before (Previous Implementation)

```
scroll_down()
  └─ No parameter
  └─ OutputBuffer doesn't know viewport height
  └─ Can't calculate proper scroll bounds
```

### After (Current Implementation)

```
scroll_down(visible_lines)
  ├─ Parameter passed from TerminalState
  ├─ OutputBuffer knows viewport height
  ├─ Calculates: max_scroll = buffer.len() - visible_lines
  └─ Bounds-safe scrolling guaranteed
```

## Integration Points

### Event Loop Integration
```
InfrawareTerminal::run()
  ├─ Poll EventHandler
  ├─ Match TerminalEvent
  ├─ Handle ScrollUp/ScrollDown
  │  └─ Call state.scroll_up/down()
  ├─ Render output
  │  └─ Call ui.render(&mut state)
  └─ Repeat
```

### Render Pipeline Integration
```
TerminalUI::render(&mut state)
  ├─ Calculate visible_lines from terminal size
  ├─ Update state.set_visible_lines(lines)
  ├─ Render frame with updated state
  └─ OutputBuffer uses visible_lines for correct display range
```

## Documentation Cross-References

- **UML Diagram**: `04-terminal-state-and-buffers.puml` shows event flow → state update → render cycle
- **Architecture Guide**: `SCROLLING_ARCHITECTURE.md` details each step with code examples
- **Quick Reference**: `QUICK_REFERENCE_SCROLLING.md` provides quick lookup tables
- **Summary**: `UML_UPDATE_SUMMARY.md` explains design decisions

## Testing References

### Unit Tests Location
- **State Tests**: `tests/terminal_state_tests.rs`
- **Buffer Tests**: In `src/terminal/buffers.rs` (inline tests)
- **Event Tests**: In `src/terminal/events.rs` (inline tests)

### Test Verification
```bash
# Run all tests
cargo test

# Run terminal-specific tests
cargo test terminal

# Run with output
cargo test -- --nocapture
```

## Implementation Verification

All diagrams have been verified against source code:

```
Source Verification:
✓ TerminalState fields match state.rs
✓ OutputBuffer methods match buffers.rs
✓ TerminalUI signatures match tui.rs
✓ EventHandler details match events.rs
✓ Event mappings verified in events.rs
```

**Date Verified**: December 2, 2025
**Verified By**: Code review and cross-reference checks

---

## Navigation Guide

### If You Want To...

**...understand scrolling behavior**
→ Read `QUICK_REFERENCE_SCROLLING.md`

**...debug a scroll issue**
→ Read `SCROLLING_ARCHITECTURE.md` (Edge Cases section)

**...see the big picture**
→ View `00-main-application-architecture.puml`

**...understand state management**
→ View `04-terminal-state-and-buffers.puml`

**...see all components**
→ View `06-complete-class-diagram.puml`

**...understand design decisions**
→ Read `UML_UPDATE_SUMMARY.md`

**...implement a new feature**
→ Read `SCROLLING_ARCHITECTURE.md` (Implementation Details)

**...write tests**
→ Read `SCROLLING_ARCHITECTURE.md` (Testing Scenarios)

---

## Version Information

| Document | Version | Last Updated | Status |
|----------|---------|--------------|--------|
| 04-terminal-state-and-buffers.puml | 2.0 | Dec 2, 2025 | Current |
| 00-main-application-architecture.puml | 2.0 | Dec 2, 2025 | Current |
| 06-complete-class-diagram.puml | 2.0 | Dec 2, 2025 | Current |
| UML_UPDATE_SUMMARY.md | 1.0 | Dec 2, 2025 | Current |
| SCROLLING_ARCHITECTURE.md | 1.0 | Dec 2, 2025 | Current |
| QUICK_REFERENCE_SCROLLING.md | 1.0 | Dec 2, 2025 | Current |

---

## Contributing

When making changes to terminal scrolling:

1. **Update source code** in `src/terminal/*.rs`
2. **Update UML diagrams** in `docs/uml/`
3. **Update documentation** in `docs/`
4. **Verify changes** with test suite
5. **Update this index** if adding new docs

## Maintenance

Regular updates recommended when:
- Adding new event types
- Changing terminal state structure
- Modifying scroll behavior
- Adding new terminal modes
- Changing render pipeline

---

## Quick Links

- **Main Architecture**: `docs/uml/00-main-application-architecture.puml`
- **Terminal Details**: `docs/uml/04-terminal-state-and-buffers.puml`
- **Complete System**: `docs/uml/06-complete-class-diagram.puml`
- **Architecture Guide**: `docs/SCROLLING_ARCHITECTURE.md`
- **Quick Ref**: `docs/QUICK_REFERENCE_SCROLLING.md`
- **Summary**: `docs/UML_UPDATE_SUMMARY.md`

---

**Document Status**: Complete
**All Documentation Updated**: December 2, 2025
**Accuracy**: Verified against source code
