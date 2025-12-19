# Terminal Scrolling Architecture

## Overview

The terminal scrolling system implements a clean separation of concerns between viewport management, buffer scrolling, visual scrollbar rendering, and event handling. This document provides a high-level architectural overview including mouse wheel support and visual scrollbar indicators.

## Core Components

### 1. TerminalState (Viewport Manager)

**Location**: `src/terminal/state.rs`

```
┌─────────────────────────────────────────┐
│     TerminalState                       │
├─────────────────────────────────────────┤
│ Fields:                                 │
│ - output: OutputBuffer                  │
│ - input: InputBuffer                    │
│ - history: CommandHistory               │
│ - mode: TerminalMode                    │
│ - visible_lines: usize                  │
│ - pending_interaction: Option           │
│ - scrollbar_info: Option<ScrollbarInfo> │
├─────────────────────────────────────────┤
│ Key Methods:                            │
│ + scroll_up()                           │
│ + scroll_down()                         │
│ + scroll_to_end()                       │
│ + set_visible_lines(lines)              │
│ + visible_lines() -> usize              │
└─────────────────────────────────────────┘
         ↓                    ↑
         │   delegates to     │
         │                    │
         ↓                    │
┌─────────────────────────────────────────┐
│     OutputBuffer                        │
├─────────────────────────────────────────┤
│ Fields:                                 │
│ - buffer: Vec<String>                   │
│ - parsed_buffer: Vec<Line<'static>>     │
│ - scroll_position: usize                │
│ - visible_lines: usize                  │
│ - extra_lines: usize                    │
├─────────────────────────────────────────┤
│ Key Methods:                            │
│ + scroll_up()                           │
│ + scroll_down()                         │
│ + scroll_to_end()                       │
│ + set_visible_lines(lines)              │
│ + set_extra_lines(lines)                │
│ + set_scroll_position_exact(pos)        │
│ + auto_scroll_to_bottom()               │
└─────────────────────────────────────────┘
```

### 2. ScrollbarInfo (Mouse Interaction)

**Location**: `src/terminal/state.rs`

```rust
pub struct ScrollbarInfo {
    pub column: u16,        // Rightmost column where scrollbar is rendered
    pub height: u16,        // Total height of scrollbar area
    pub total_lines: usize, // Total lines in output buffer
    pub visible_lines: usize, // Visible lines in output area
}
```

**Key Methods**:
- `is_on_scrollbar(column)` - Check if mouse position is on scrollbar
- `row_to_scroll_position(row)` - Convert mouse row to scroll position

### 3. Event Flow

```
Input Events
    │
    ↓
┌──────────────────┐
│  EventHandler    │
│  (events.rs)     │
└──────────────────┘
    │
    ├─ Key::Up + Ctrl ──────→ TerminalEvent::ScrollUp
    ├─ Key::Down + Ctrl ────→ TerminalEvent::ScrollDown
    ├─ Key::PageUp ─────────→ TerminalEvent::ScrollUp
    ├─ Key::PageDown ───────→ TerminalEvent::ScrollDown
    ├─ Mouse::ScrollUp ─────→ TerminalEvent::ScrollUp      [NEW]
    ├─ Mouse::ScrollDown ───→ TerminalEvent::ScrollDown    [NEW]
    └─ Key::Up/Down ────────→ TerminalEvent::HistoryPrevious/Next
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
    ├─ Char input ─→ state.scroll_to_end()  [NEW: auto-scroll on typing]
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
    ├─ Render UI with scrolled content
    └─ Render Scrollbar widget if content > viewport  [NEW]
```

### 4. Rendering Pipeline with Scrollbar

```
TerminalUI::render(&mut state)
│
├─ Step 1: Build All Content Lines
│  all_lines = output.parsed_lines() + interaction + prompt
│  output_line_count = output lines only
│  extra_lines = total - output_line_count
│
├─ Step 2: Calculate Scroll
│  total_lines = all_lines.len()
│  visible_height = area.height
│  max_scroll = total_lines - visible_height
│  effective_scroll = scroll_position.min(max_scroll)
│  [Clamp and sync back to buffer if needed]
│
├─ Step 3: Render Content with Scroll
│  Paragraph::new(all_lines).scroll((effective_scroll, 0))
│  [Ratatui handles the scroll offset]
│
├─ Step 4: Render Scrollbar (if needed)
│  if total_lines > visible_height:
│    ├─ Store ScrollbarInfo for mouse interaction
│    ├─ ScrollbarState::default()
│    │    .content_length(max_scroll.max(1))  [KEY FIX]
│    │    .position(effective_scroll)
│    └─ render_stateful_widget(Scrollbar, area, &mut state)
│
└─ Step 5: Position Cursor
   Calculate prompt line position on screen
   Set cursor at prompt + input width
```

## Scrollbar Position Fix

### The Problem

Ratatui calculates scrollbar thumb position as:
```
thumb_position = position / content_length
```

Using `content_length(total_lines)` with `position(effective_scroll)` gives wrong results:
- Example: 33 lines, 30 visible, scrolled to bottom (position=3)
- Wrong: 3/33 = 9% → thumb near top
- Expected: 3/3 = 100% → thumb at bottom

### The Solution

```rust
// content_length = max_scroll (number of scrollable positions: 0..=max_scroll)
let scrollbar_content_length = max_scroll.max(1); // Avoid division by zero
let mut scrollbar_state = ScrollbarState::default()
    .content_length(scrollbar_content_length)
    .position(effective_scroll);
```

Now: 3/3 = 100% → thumb correctly at bottom

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
       └─ scroll_position = max_scroll
          [Shows last visible_lines of output]
```

**Result**: New output automatically scrolls to bottom.

### When User Types (NEW)

```
User types character
    ↓
handle_event(Char(c))
    ├─ state.insert_char(c)
    └─ state.scroll_to_end()  [NEW]
        └─ output.scroll_to_end()
            └─ scroll_position = usize::MAX
               [Clamped to max_scroll on next render]
```

**Result**: Typing always brings prompt back into view, even if user was scrolled up.

### When User Manually Scrolls Up

```
User presses Ctrl+Up or MouseScrollUp
    ↓
scroll_position decreases
    ↓
[User can now review history]
    ↓
User types → scroll_to_end() → back to bottom
```

## Mouse Wheel Support

### Event Mapping (events.rs)

```rust
fn map_mouse_event(event: MouseEvent) -> TerminalEvent {
    match event.kind {
        MouseEventKind::ScrollUp => TerminalEvent::ScrollUp,
        MouseEventKind::ScrollDown => TerminalEvent::ScrollDown,
        _ => TerminalEvent::Unknown,
    }
}
```

### Mouse Capture (tui.rs)

```rust
// In TerminalUI::new()
execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

// In cleanup()
execute!(self.terminal.backend_mut(), DisableMouseCapture, LeaveAlternateScreen)?;
```

## Extra Lines Tracking

### Purpose

The unified content area includes:
1. Output buffer lines
2. Pending interaction lines (command approval, questions)
3. Prompt line

The `extra_lines` field tracks non-output content to calculate correct scroll bounds:

```rust
// In tui.rs render_unified_content()
let extra_lines = total_lines.saturating_sub(output_line_count);
state.output.set_extra_lines(extra_lines);
```

### Usage in OutputBuffer

```rust
fn effective_total_lines(&self) -> usize {
    self.parsed_buffer.len() + self.extra_lines
}
```

## Key Invariants

1. **scroll_position ≤ max_scroll** - Never points past scrollable range

2. **max_scroll = total_lines - visible_lines** - Maximum scroll position

3. **effective_scroll = scroll_position.min(max_scroll)** - Clamped for rendering

4. **Scrollbar appears only when total_lines > visible_height**

5. **Scrollbar thumb position = effective_scroll / max_scroll** - Correct at all positions

6. **Typing triggers scroll_to_end()** - Prompt always visible when user types

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

## Performance Characteristics

| Operation | Time | Space |
|-----------|------|-------|
| scroll_up() | O(1) | O(1) |
| scroll_down() | O(1) | O(1) |
| scroll_to_end() | O(1) | O(1) |
| set_visible_lines() | O(1) | O(1) |
| add_line() | O(1)* | O(1)* |
| trim_if_needed() | O(n)** | - |
| Scrollbar render | O(1) | O(1) |

*Amortized - buffer is Vec<String>
**Only when buffer exceeds 10,000 lines

## Related Code Locations

| Component | File | Key Functions |
|-----------|------|----------------|
| TerminalState | `src/terminal/state.rs` | scroll_up/down/to_end, set_visible_lines |
| OutputBuffer | `src/terminal/buffers.rs` | scroll_down, set_visible_lines, set_extra_lines |
| ScrollbarInfo | `src/terminal/state.rs` | is_on_scrollbar, row_to_scroll_position |
| TerminalUI | `src/terminal/tui.rs` | render(&mut state), Scrollbar widget |
| EventHandler | `src/terminal/events.rs` | map_key_event, map_mouse_event |
| Main Loop | `src/main.rs` | InfrawareTerminal::handle_event |

## Scrollbar Visual Style

```rust
let scrollbar = Scrollbar::new(ScrollbarOrientation::VerticalRight)
    .begin_symbol(Some("↑"))  // Top arrow
    .end_symbol(Some("↓"));   // Bottom arrow
```

The scrollbar uses Ratatui's default track (│) and thumb (█) symbols.

---

**Last Updated**: December 14, 2025
**Status**: Complete - Visual scrollbar, mouse wheel, and auto-scroll implemented
