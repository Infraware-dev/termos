# UML Diagrams - Quick Reference Index

## Diagram Selection Guide

| Diagram | Type | Purpose | Best For |
|---------|------|---------|----------|
| **00-architecture-overview** | Class/Component | System-wide view of all major components | Onboarding, documentation, system design |
| **01-pty-module** | Class | PTY subsystem architecture | PTY implementation, I/O design, testing |
| **02-terminal-module** | Class | Terminal emulation components | Terminal features, scrolling, colors |
| **03-state-machine** | State | Application mode transitions | LLM integration, state handling |
| **04-data-flow** | Sequence | Complete I/O data paths | Performance analysis, debugging |
| **05-render-pipeline** | Activity | egui rendering optimization | Rendering performance, frame timing |

## Component Cross-Reference

### InfrawareApp
- **Diagrams**: 00 (central), 04 (sequence), 05 (render)
- **Key methods**: update(), poll_pty_output(), send_to_pty(), render_terminal()
- **Location**: src/app.rs

### PTY Module
- **Diagrams**: 00 (subsystem), 01 (detail), 04 (data flow)
- **Components**: PtyManager, PtySession, PtyReader, PtyWriter
- **Location**: src/pty/{manager,session,io}.rs

### Terminal Module
- **Diagrams**: 00 (subsystem), 02 (detail), 04 (data flow)
- **Components**: TerminalGrid, TerminalHandler, Cell, Color
- **Location**: src/terminal/{grid,handler,cell}.rs

### Rendering
- **Diagrams**: 00 (subsystem), 05 (detail)
- **Functions**: render_backgrounds(), render_text_runs(), render_cursor()
- **Location**: src/ui/renderer.rs

### State Management
- **Diagrams**: 00 (state overview), 03 (state machine)
- **Component**: AppMode enum
- **Location**: src/state.rs

### Keyboard Input
- **Diagrams**: 00 (input subsystem), 04 (data flow)
- **Component**: KeyboardHandler
- **Location**: src/input/keyboard.rs

## Data Flow Paths

### Keyboard Input Flow
```
User keyboard event
  ↓
egui::Context (Event)
  ↓
KeyboardHandler::process()
  ↓
KeyboardAction enum
  ↓
InfrawareApp::handle_keyboard()
  ↓
PtyWriter::write_bytes()
  ↓
PTY kernel buffer
  ↓
Shell (bash/zsh)
```
**Diagrams**: 00, 04 | **Code**: src/app.rs (handle_keyboard), src/input/keyboard.rs

### Shell Output Flow
```
Shell process
  ↓
PTY kernel buffer
  ↓
PtyReader (background thread)
  ↓
mpsc::sync_channel
  ↓
InfrawareApp::poll_pty_output()
  ↓
vte::Parser::advance()
  ↓
TerminalHandler::Perform trait
  ↓
TerminalGrid updates
  ↓
egui rendering
  ↓
Display
```
**Diagrams**: 00, 04 | **Code**: src/app.rs (poll_pty_output), src/terminal/handler.rs

### Rendering Flow
```
TerminalGrid (live state)
  ↓
InfrawareApp::render_terminal()
  ↓
Single-pass iteration
  ↓
Batch backgrounds/text/decorations
  ↓
render_backgrounds() → rect_filled()
render_text_runs() → text()
render_decorations() → line()
render_cursor() → block cursor
render_scrollbar() → thumb
  ↓
egui::Painter (drawing commands)
  ↓
Display frame
```
**Diagrams**: 00, 05 | **Code**: src/app.rs (render_terminal)

## Key Concepts

### Single-Pass Rendering (Diagram 05)
**Problem**: Traditional rendering iterates grid 3 times
- 1st pass: backgrounds
- 2nd pass: text
- 3rd pass: decorations

**Solution**: Single iteration with batching
- Collect backgrounds (batch same color)
- Collect text runs (batch same color)
- Collect decorations (no batch, rare)
- Render in z-order

**Benefits**: Faster, better cache usage, fewer draw calls

### Backpressure & Rate Limiting (Diagram 04)
**Reader thread**: Blocks on PTY read() until data available
- No polling overhead
- Sleeps when idle
- Wakes on kernel data

**Channel**: sync_channel with capacity 32
- If full, thread blocks
- Creates backpressure on kernel
- PTY write buffer fills
- Shell cat command blocks

**Rate limiting**: MAX_BYTES_PER_FRAME (~4KB per frame)
- Prevents frame drops from huge output
- Allows Ctrl+C to work even during cat /dev/zero

### State Machine Safety (Diagram 03)
**Type-safe transitions**:
```rust
// Compile error - can_transition_to() returns bool
let state = AppMode::Normal;
state.direct_to_approval();  // ❌ No such method

// Correct - use transition()
let new = state.transition(QueryLLM)?;  // ✓ Normal → WaitingLLM
```

**Valid transitions enforced**:
- Normal → WaitingLLM
- WaitingLLM → Normal/AwaitingApproval/AwaitingAnswer
- AwaitingApproval/AwaitingAnswer → Normal
- Any → Normal (Cancel)

### Color Model (Diagram 02)
**Named colors**: 16 ANSI colors
```
Standard: Black, Red, Green, Yellow, Blue, Magenta, Cyan, White
Bright:   BrightBlack, BrightRed, ... BrightWhite
Default:  Foreground, Background
```

**Indexed palette**: 256 colors
```
0-15:     ANSI (above)
16-231:   6×6×6 RGB cube (216 colors)
232-255:  Grayscale (24 shades)
```

**True color**: 24-bit RGB
```
Supported via SGR 38;2;R;G;B (fg) and 48;2;R;G;B (bg)
```

### Terminal Grid Architecture (Diagram 02)
**Visible grid**: Screen-sized [row][col] for display
```
cells: Vec<Vec<Cell>>  (e.g., 24 rows × 80 cols)
```

**Scrollback buffer**: Historical lines
```
scrollback: Vec<Vec<Cell>>  (max 10,000 lines, FIFO)
```

**Scroll offset**: View positioning
```
0 = viewing bottom (live)
N = scrolled up N lines into history
```

**Viewport computation**:
```
visible_rows() = scrollback[-scroll_offset:] + cells[0:scroll_offset]
```

## Performance Targets

| Operation | Target | Code |
|-----------|--------|------|
| Frame rate (focused) | 30-60 FPS | Depends on PTY output |
| Frame rate (focused, idle) | 2 FPS | cursor blink interval |
| Frame rate (unfocused) | 2 FPS | BACKGROUND_REPAINT constant |
| Keyboard latency | <1 frame | Processed each frame |
| PTY read latency | <1 frame | Background thread |
| Render time (single row) | <500µs | Single-pass iteration |
| Total frame time | <16ms | egui frame budget |

## Common Modifications

### Add SGR Color Code
**File**: src/terminal/handler.rs
**Method**: TerminalHandler::process_sgr()
**Pattern**: Match code, call grid.set_fg()/set_bg()

### Add Keyboard Shortcut
**File**: src/input/keyboard.rs
**Method**: KeyboardHandler::process_ctrl_keys() or process_other_keys()
**Pattern**: Check egui Key, return KeyboardAction

### Add Terminal Feature
**File**: src/terminal/grid.rs or handler.rs
**Method**: TerminalGrid or TerminalHandler methods
**Pattern**: Implement vte::Perform callbacks

### Optimize Rendering
**File**: src/app.rs
**Method**: InfrawareApp::render_terminal()
**Pattern**: Modify batching logic in cell iteration

## Testing Considerations

### PTY Testing
- **Unit tests**: Mock PtyWrite/PtyControl traits
- **Integration tests**: Real shell execution (skip in CI)
- **Manual**: Run app, verify signal delivery

### Terminal Testing
- **Unit tests**: TerminalGrid cell operations
- **Escape sequence tests**: SGR codes, cursor movement
- **Manual**: vim, less, colored output

### Rendering Testing
- **Visual**: Compare before/after batching changes
- **Performance**: cargo bench render
- **Manual**: Various terminal sizes, complex layouts

---

## Related Files

### Configuration
- **Size**: src/config/size.rs (DEFAULT_ROWS, DEFAULT_COLS)
- **Rendering**: src/config/rendering.rs (CHAR_WIDTH, CHAR_HEIGHT, FONT_SIZE)
- **Timing**: src/config/timing.rs (CURSOR_BLINK_INTERVAL, SHELL_INIT_DELAY)
- **PTY**: src/config/pty.rs (CHANNEL_CAPACITY, DEFAULT_PTY_SIZE)

### Main Entry Points
- **App**: src/app.rs - InfrawareApp struct
- **Main**: src/main.rs - eframe::run_native()
- **State**: src/state.rs - AppMode enum

### Modules
- **pty**: src/pty/{mod,manager,session,io,traits}.rs
- **terminal**: src/terminal/{mod,handler,grid,cell}.rs
- **ui**: src/ui/{mod,renderer,theme}.rs
- **input**: src/input/{mod,keyboard}.rs

---

**Quick Links**:
- [README.md](README.md) - Detailed diagram descriptions
- [GitHub PlantUML](https://github.com/features/actions) - Live rendering
- [PlantUML Editor](http://www.plantuml.com/plantuml/uml/) - Online viewer

**Last Updated**: 2025-12-31
