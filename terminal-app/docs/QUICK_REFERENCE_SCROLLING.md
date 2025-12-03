# Quick Reference: Terminal Scrolling Implementation

## At a Glance

```
TerminalState::visible_lines (usize)
    │
    ├─ Set during: TerminalUI::render(&mut state)
    ├─ Used by: OutputBuffer::scroll_down(visible_lines)
    └─ Getter: state.visible_lines()

OutputBuffer::scroll_down(visible_lines: usize)
    │
    ├─ Calculates: max_scroll = buffer.len() - visible_lines
    ├─ Checks: if scroll_position < max_scroll
    └─ Updates: scroll_position += 1

Keyboard → Event → Handler → State Mutation → Render
  Ctrl+Up      ScrollUp     state.scroll_up()    Updated output
  Ctrl+Down    ScrollDown   state.scroll_down()  Updated output
  PageUp       ScrollUp     (same)               (same)
  PageDown     ScrollDown   (same)               (same)
```

## Three Main Changes

### 1. TerminalState Now Stores Viewport Height

```rust
// NEW in TerminalState
- visible_lines: usize
+ set_visible_lines(lines: usize)
+ visible_lines() -> usize
```

**Why**: OutputBuffer needs to know viewport height to calculate max scroll position.

### 2. OutputBuffer.scroll_down() Takes Parameter

```rust
// BEFORE
pub fn scroll_down(&mut self)

// AFTER
pub fn scroll_down(&mut self, visible_lines: usize)
```

**Why**: Can now calculate bounds correctly: `max_scroll = buffer.len() - visible_lines`

### 3. TerminalUI.render() Takes Mutable State

```rust
// BEFORE
pub fn render(&self, state: &TerminalState) -> Result<()>

// AFTER
pub fn render(&mut self, state: &mut TerminalState) -> Result<()>
```

**Why**: Needs to update `state.visible_lines` based on actual terminal size.

## Control Flow

### User Presses Ctrl+Down

```
1. EventHandler::map_key_event(Down + Ctrl)
   → TerminalEvent::ScrollDown

2. InfrawareTerminal::handle_event(ScrollDown)
   → state.scroll_down()

3. TerminalState::scroll_down()
   → output.scroll_down(self.visible_lines)

4. OutputBuffer::scroll_down(visible_lines)
   → Clamp scroll_position to max_scroll
   → scroll_position += 1

5. Next render cycle shows new scroll position
```

## Method Signatures

### TerminalState

```rust
pub fn scroll_up(&mut self) {
    self.output.scroll_up();
}

pub fn scroll_down(&mut self) {
    self.output.scroll_down(self.visible_lines);
}

pub fn set_visible_lines(&mut self, lines: usize) {
    self.visible_lines = lines;
    self.output.set_visible_lines(lines);
}

pub const fn visible_lines(&self) -> usize {
    self.visible_lines
}
```

### OutputBuffer

```rust
pub const fn scroll_up(&mut self) {
    if self.scroll_position > 0 {
        self.scroll_position -= 1;
    }
}

pub fn scroll_down(&mut self, visible_lines: usize) {
    let max_scroll = self.buffer.len().saturating_sub(visible_lines);
    if self.scroll_position < max_scroll {
        self.scroll_position += 1;
    }
}

pub fn set_visible_lines(&mut self, visible_lines: usize) {
    let max_scroll = self.buffer.len().saturating_sub(visible_lines);
    if self.scroll_position > max_scroll {
        self.scroll_position = max_scroll;
    }
}
```

### TerminalUI

```rust
pub fn render(&mut self, state: &mut TerminalState) -> Result<()> {
    let size = self.terminal.size()?;
    let output_height = size.height.saturating_sub(4);
    let visible_lines = output_height.saturating_sub(2) as usize;
    state.set_visible_lines(visible_lines);

    self.terminal.draw(|frame| {
        render_frame(frame, state);
    })?;
    Ok(())
}
```

## Event Mapping

| Key Combination | Event | Effect |
|-----------------|-------|--------|
| Ctrl+Up | ScrollUp | Move view up 1 line |
| Ctrl+Down | ScrollDown | Move view down 1 line |
| PageUp | ScrollUp | Move view up 1 line |
| PageDown | ScrollDown | Move view down 1 line |
| Up (no mod) | HistoryPrevious | Previous command |
| Down (no mod) | HistoryNext | Next command |

## Buffer States

### Typical Buffer State

```
buffer: Vec<String> with 100 lines
scroll_position: 50
visible_lines: 10 (from terminal size)
max_scroll: 100 - 10 = 90

Displayed: lines[50..60] (10 lines starting at scroll_position)
Can scroll: UP (to 0), DOWN (to 90)
```

### Buffer Smaller Than Viewport

```
buffer: 5 lines
scroll_position: 0
visible_lines: 10
max_scroll: 5 - 10 = 0 (saturating_sub)

Displayed: lines[0..5] + empty space
Can scroll: NO (already at max)
```

### Terminal Resize Handling

```
Before: visible_lines = 10, scroll_position = 8
After terminal shrinks to 5 lines:
  - set_visible_lines(5) called
  - max_scroll = buffer.len() - 5
  - If scroll_position > max_scroll: clamp to max_scroll
  - Example: scroll_position 8 > new max_scroll 5 → clamped to 5
```

## Auto-Scroll Logic

```
// When new output added
add_output(line)
  ↓
add_line(line)
  ├─ buffer.push(line)
  ├─ trim_if_needed()     [if > 10,000 lines]
  └─ auto_scroll_to_bottom()
     └─ scroll_position = buffer.len()
        [Show last visible_lines when rendering]

Result: New output always visible at bottom
```

## Important Constants

```rust
MAX_OUTPUT_LINES: usize = 10_000     // Max buffer size
TRIM_LINES: usize = 1_000            // Headroom when trimming
DEFAULT_VISIBLE_LINES: usize = 20    // Initial value
```

## Testing Checklist

- [ ] Scroll down to last line of buffer
- [ ] Scroll up from bottom to top
- [ ] Add new output while scrolled up (should not auto-scroll)
- [ ] Add new output while at bottom (should remain at bottom)
- [ ] Resize terminal smaller (scroll should clamp)
- [ ] Resize terminal larger (scroll should remain valid)
- [ ] Buffer trim during scroll (scroll_position adjusted)
- [ ] Empty buffer scrolling (no-op)
- [ ] Single line buffer (no scroll movement)

## Files to Review

1. **UML Diagrams**:
   - `/home/crist/infraware-terminal/terminal-app/docs/uml/04-terminal-state-and-buffers.puml`
   - `/home/crist/infraware-terminal/terminal-app/docs/uml/06-complete-class-diagram.puml`

2. **Implementation**:
   - `/home/crist/infraware-terminal/terminal-app/src/terminal/state.rs`
   - `/home/crist/infraware-terminal/terminal-app/src/terminal/buffers.rs`
   - `/home/crist/infraware-terminal/terminal-app/src/terminal/tui.rs`
   - `/home/crist/infraware-terminal/terminal-app/src/terminal/events.rs`

3. **Documentation**:
   - `/home/crist/infraware-terminal/terminal-app/docs/UML_UPDATE_SUMMARY.md`
   - `/home/crist/infraware-terminal/terminal-app/docs/SCROLLING_ARCHITECTURE.md`

## Key Takeaways

1. **Viewport Height Managed**: TerminalState stores visible_lines
2. **Safe Scrolling**: OutputBuffer uses visible_lines for bounds checks
3. **Mutable Render**: render(&mut state) updates visible_lines
4. **Event-Driven**: Ctrl+Up/Down and PageUp/Down trigger scrolling
5. **Auto-Scroll Smart**: Respects user's scroll position when appropriate
6. **Memory Bounded**: Trims at 10,000 lines with 1,000 line headroom

---

**Last Updated**: December 2, 2025
**Documentation Status**: Complete
