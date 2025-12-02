# Terminal Scrolling Architecture

## Overview

The terminal scrolling system implements a clean separation of concerns between viewport management, buffer scrolling, and event handling. This document provides a high-level architectural overview.

## Core Components

### 1. TerminalState (Viewport Manager)

**Location**: `src/terminal/state.rs`

```
┌─────────────────────────────────────┐
│     TerminalState                   │
├─────────────────────────────────────┤
│ Fields:                             │
│ - output: OutputBuffer              │
│ - input: InputBuffer                │
│ - history: CommandHistory           │
│ - mode: TerminalMode                │
│ - visible_lines: usize    [NEW]     │
│ - pending_interaction: Option       │
├─────────────────────────────────────┤
│ Key Methods:                        │
│ + scroll_up()                       │
│ + scroll_down()                     │
│ + set_visible_lines(lines)  [NEW]   │
│ + visible_lines() -> usize  [NEW]   │
└─────────────────────────────────────┘
         ↓                    ↑
         │   delegates to     │
         │                    │
         ↓                    │
┌─────────────────────────────────────┐
│     OutputBuffer                    │
├─────────────────────────────────────┤
│ Fields:                             │
│ - buffer: Vec<String>               │
│ - scroll_position: usize            │
├─────────────────────────────────────┤
│ Key Methods:                        │
│ + scroll_up()                       │
│ + scroll_down(visible_lines) [UPD]  │
│ + set_visible_lines(lines)  [NEW]   │
│ + auto_scroll_to_bottom()           │
└─────────────────────────────────────┘
```

### 2. Event Flow

```
Keyboard Input
    │
    ↓
┌──────────────────┐
│  EventHandler    │
│  (events.rs)     │
└──────────────────┘
    │
    ├─ Key::Up + Ctrl ──→ TerminalEvent::ScrollUp
    ├─ Key::Down + Ctrl ─→ TerminalEvent::ScrollDown
    ├─ Key::PageUp ──────→ TerminalEvent::ScrollUp
    ├─ Key::PageDown ────→ TerminalEvent::ScrollDown
    └─ Key::Up/Down ─────→ TerminalEvent::HistoryPrevious/Next
    │
    ↓
┌──────────────────────────┐
│  InfrawareTerminal       │
│  (main.rs)               │
│  handle_event()          │
└──────────────────────────┘
    │
    ├─ ScrollUp ──→ state.scroll_up()
    ├─ ScrollDown ─→ state.scroll_down()
    │   (uses stored visible_lines)
    └─ Other ──────→ other handlers
    │
    ↓
┌──────────────────────────┐
│  TerminalUI::render()    │
│  (tui.rs)                │
│  MUTABLE BORROW          │
└──────────────────────────┘
    │
    ├─ Calculate visible_lines from terminal size
    ├─ Call state.set_visible_lines(lines)
    ├─ Sync OutputBuffer scroll position if needed
    └─ Render UI with scrolled content
```

### 3. Rendering Pipeline

```
TerminalUI::render(&mut state)
│
├─ Step 1: Calculate Dimensions
│  size = terminal.size()?
│  output_height = size.height.saturating_sub(4)
│  visible_lines = output_height.saturating_sub(2) as usize
│
├─ Step 2: Update State
│  state.set_visible_lines(visible_lines)
│  ├─ Updates visible_lines field
│  └─ (OutputBuffer can now use correct bounds)
│
├─ Step 3: Render Frame
│  terminal.draw(|frame| {
│      render_frame(frame, state)
│  })
│  ├─ Gets visible_lines from state
│  ├─ Calculates display range from scroll_position
│  ├─ Renders output lines[scroll_pos..scroll_pos+visible]
│  ├─ Renders input buffer with cursor
│  └─ Renders scrollbar indicator
│
└─ Complete: UI reflects current scroll position
```

## Scrolling Semantics

### When User Presses Ctrl+Down

1. **EventHandler** detects `Key::Down + Ctrl` → generates `TerminalEvent::ScrollDown`

2. **InfrawareTerminal** receives event:
   ```rust
   TerminalEvent::ScrollDown => {
       state.scroll_down()
   }
   ```

3. **TerminalState::scroll_down()** (defined in state.rs):
   ```rust
   pub fn scroll_down(&mut self) {
       self.output.scroll_down(self.visible_lines);
   }
   ```
   - Passes the stored `visible_lines` value to OutputBuffer
   - This ensures scroll knows the viewport height

4. **OutputBuffer::scroll_down(visible_lines)** (defined in buffers.rs):
   ```rust
   pub fn scroll_down(&mut self, visible_lines: usize) {
       let max_scroll = self.buffer.len().saturating_sub(visible_lines);
       if self.scroll_position < max_scroll {
           self.scroll_position += 1;
       }
   }
   ```
   - Calculates maximum allowable scroll position
   - Clamps to prevent scrolling past content
   - Increments position if within bounds

5. **TerminalUI::render()** called next frame:
   - Recalculates `visible_lines` from terminal size
   - Updates state with current `visible_lines`
   - Renders visible portion of buffer

### Window Resize Scenario

```
Resize event detected (u16, u16)
    ↓
InfrawareTerminal::handle_event(Resize)
    ↓
[No direct state update - defer to render]
    ↓
TerminalUI::render(&mut state)
    │
    ├─ New size = terminal.size()?
    ├─ New visible_lines = recalculate(new_size)
    ├─ state.set_visible_lines(new_visible_lines)
    │  └─ [Clamps scroll_position to new bounds]
    ├─ state.output.set_visible_lines(new_visible_lines)
    │  └─ [Syncs OutputBuffer with new bounds]
    └─ Render with properly adjusted scroll
```

## Auto-Scroll Behavior

### When Output Added

```
state.add_output("new line")
    ↓
output.add_line("new line")
    ├─ buffer.push("new line")
    ├─ trim_if_needed()  [if > 10,000 lines]
    │  └─ Adjusts scroll_position if trimmed
    └─ auto_scroll_to_bottom()
       └─ scroll_position = buffer.len()
          [Shows last visible_lines of output]
```

**Result**: New output automatically scrolls to bottom, unless user manually scrolled up.

### When User Manually Scrolls Up

```
User presses Ctrl+Up
    ↓
scroll_position decreases (state.scroll_up())
    ↓
User types command output → new line added
    ↓
auto_scroll_to_bottom() sets:
    scroll_position = buffer.len()
    ↓
Next render shows bottom of output
[User must scroll up again to see old content]
```

**Behavior**: Auto-scroll respects user's scroll position - pressing Ctrl+Up temporarily "freezes" scroll to review history.

## Memory Management

### Buffer Trimming

```
OutputBuffer capacity: 10,000 lines maximum

When buffer.len() > 10,000:
├─ lines_to_remove = buffer.len() - 10,000 + 1,000
│  (1,000 line headroom to reduce trim frequency)
├─ buffer.drain(0..lines_to_remove)
│  [Remove oldest lines]
└─ scroll_position = scroll_position.saturating_sub(lines_to_remove)
   [Adjust scroll if trimmed content was above current view]
```

**Example**:
- Buffer has 11,000 lines, scroll_position = 9,000
- Trim removes oldest 2,000 lines
- New buffer has 9,000 lines
- scroll_position becomes 7,000

## Field Lifecycle

### visible_lines

```
Default: 20 lines (set in TerminalState::new())
    ↓
During render: Recalculated from terminal size
    ↓
state.set_visible_lines(calculated)
    ↓
Stored for use in scroll operations
    ↓
Next event loop uses updated value
```

**Lifetime**: Per-frame - recalculated every render cycle

### scroll_position

```
Initial: 0 (empty buffer)
    ↓
On add_line: → buffer.len() (auto-scroll to bottom)
    ↓
On scroll_up: → scroll_position - 1
    ↓
On scroll_down: → scroll_position + 1 (bounded by max_scroll)
    ↓
On set_visible_lines: Clamped to new max_scroll
    ↓
On trim: Adjusted if trimmed content affected scroll
```

**Lifetime**: Persistent across frames until reset

## Key Invariants

1. **scroll_position ≤ buffer.len()** - Never points past end

2. **visible_lines ≤ output_height** - Reflects viewport

3. **max_scroll = buffer.len() - visible_lines**
   - scroll_position stays ≤ max_scroll
   - Unless buffer smaller than viewport (then 0)

4. **Auto-scroll = (scroll_position == buffer.len())**
   - Newly added content appears at bottom
   - Unless user explicitly scrolled up

## Testing Scenarios

### Test: Basic Scrolling

```rust
let mut state = TerminalState::new();
state.set_visible_lines(5);

// Add 10 lines
for i in 0..10 {
    state.add_output(format!("line {}", i));
}

// scroll_position should be 10 (at bottom)
assert_eq!(state.output.scroll_position(), 10);

// Scroll up 3 lines
state.scroll_up();
state.scroll_up();
state.scroll_up();
assert_eq!(state.output.scroll_position(), 7);

// Scroll down - should clamp to max_scroll (10 - 5 = 5)
// Wait, actually max_scroll = 10 - 5 = 5, we're at 7, so no change
state.scroll_down();
// scroll_position should be 8 after one scroll_down
```

### Test: Viewport Resize

```rust
let mut state = TerminalState::new();
state.set_visible_lines(10);

// Add 20 lines
for i in 0..20 {
    state.add_output(format!("line {}", i));
}

// Scroll to top
for _ in 0..20 {
    state.scroll_up();
}
assert_eq!(state.output.scroll_position(), 0);

// Simulate terminal shrinking to 5 visible lines
state.set_visible_lines(5);

// Scroll position should clamp to new max_scroll (20 - 5 = 15)
// So position 0 stays valid
assert_eq!(state.output.scroll_position(), 0);
```

## Edge Cases Handled

1. **Empty Buffer + Scroll Down**: No effect (max_scroll = 0)

2. **Buffer Smaller Than Viewport**: scroll_position clamped to 0

3. **Terminal Resize Larger**: May expose scroll position below buffer end (clamped)

4. **Terminal Resize Smaller**: scroll_position clamped to new max_scroll

5. **Buffer Trim During Scroll**: scroll_position adjusted proportionally

## Performance Characteristics

| Operation | Time | Space |
|-----------|------|-------|
| scroll_up() | O(1) | O(1) |
| scroll_down(visible_lines) | O(1) | O(1) |
| set_visible_lines() | O(1) | O(1) |
| add_line() | O(1)* | O(1)* |
| trim_if_needed() | O(n)** | - |

*Amortized - buffer is Vec<String>
**Only when buffer exceeds 10,000 lines

## Related Code Locations

| Component | File | Key Functions |
|-----------|------|----------------|
| TerminalState | `src/terminal/state.rs` | scroll_up/down, set_visible_lines |
| OutputBuffer | `src/terminal/buffers.rs` | scroll_down(visible_lines), set_visible_lines |
| TerminalUI | `src/terminal/tui.rs` | render(&mut state) |
| EventHandler | `src/terminal/events.rs` | map_key_event, poll_event |
| Main Loop | `src/main.rs` | InfrawareTerminal::handle_event |

---

**Last Updated**: December 2, 2025
**Status**: Complete - All changes implemented and documented
