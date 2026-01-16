# Infraware Terminal - UML Architecture Diagrams

This directory contains comprehensive UML diagrams documenting the architecture of the Infraware Terminal emulator—a PTY-based egui application with VTE parsing, asynchronous I/O, optimized rendering, and RigEngine integration for LLM-assisted DevOps workflows.

## Diagram Overview

### 00-architecture-overview.puml
**High-level system architecture showing all major components and their RigEngine integration.**

- **egui/eframe Application**: Main InfrawareApp struct implementing eframe::App
- **Application State**: AppMode enum with RigEngine-aware state machine (includes ExecutingCommand state)
- **PTY Subsystem**: Complete PTY module with PtyManager, PtySession, PtyReader/Writer
- **VTE Parser Pipeline**: vte::Parser and TerminalHandler implementing Perform trait
- **Terminal State**: TerminalGrid with Cell, Color, and CellAttrs structures
- **egui Rendering**: Theme and rendering functions (backgrounds, text, cursor, scrollbar)
- **Input Handling**: KeyboardHandler for keyboard event processing
- **LLM Backend**: RigEngine with HITL tool execution for command approval

**Key relationships (with RigEngine):**
- InfrawareApp owns all major components and coordinates with RigEngine backend
- RigEngine HITL flow: Tool call → AwaitingApproval → ExecutingCommand → needs_continuation decision
- Data flows: Keyboard → PTY Writer → Shell / Shell output → mpsc channel → InfrawareApp → VTE Parser → TerminalHandler → TerminalGrid → egui Rendering
- Thread model: Main thread (eframe), PTY reader thread (dedicated I/O), Tokio runtime (async tasks, backend SSE)

**When to use:** Overview documentation, RigEngine integration architecture, system design reviews.

---

### 01-pty-module.puml
**Detailed PTY module architecture with I/O primitives and lifecycle management.**

- **PtyManager**: High-level API for spawning and managing persistent shell session
- **PtySession**: Represents active PTY with master/slave pair and child process
- **PtyReader**: Async reader using dedicated background thread
- **PtyWriter**: Dual-mode (async/sync) writer for both async and sync contexts
- **Traits**: PtyWrite and PtyControl for dependency injection and testing
- **Configuration**: PtySessionConfig builder for customizable PTY spawning

**Key designs:**
- **Reader thread**: Spawned as dedicated std::thread, communicates via tokio channel
- **Writer mutex**: std::sync::Mutex (not tokio) for compatibility with both async and sync
- **Signal handling**: SIGINT delivery via /proc/<pid>/stat for foreground process group
- **Resize handling**: SIGWINCH sent through portable_pty abstraction

**Implementation notes:**
- PtyReader spawns reader thread once, owns atomic stop flag
- PtyWriter uses Arc<Mutex> for shared ownership
- PtyManager provides high-level API (shell detection, configuration)
- Portable_pty abstraction handles platform differences (Unix/Windows)

**When to use:** PTY implementation details, I/O design, testing with mocks, concurrency model.

---

### 02-terminal-module.puml
**Terminal emulation state structure with VTE integration and cell attributes.**

- **TerminalHandler**: Implements vte::Perform trait, processes escape sequences
- **TerminalGrid**: Main terminal state with cells, scrollback, cursor, scroll region
- **Cell**: Individual terminal cell with character and formatting
- **Color**: Comprehensive color model (named, indexed, RGB) with egui conversion
- **CellAttrs**: Cell attributes (bold, italic, underline, reverse, etc.)

**Grid architecture:**
- **Visible grid**: Screen-sized grid [row][col] for direct display
- **Scrollback buffer**: Historical lines (max 10,000), FIFO on overflow
- **Scroll offset**: 0 = bottom (live), >0 = scrolled into history
- **Viewport**: Computed from scroll_offset, combines scrollback + visible grid

**Terminal features:**
- **Cursor state**: Position (row, col), visibility, wrap pending
- **Scroll region**: Top/bottom bounds for scrolling operations
- **Attributes**: Accumulated from SGR codes (bold, italic, underline, etc.)
- **Alt screen**: Vim/less mode with saved main screen state
- **Saved cursor**: DECSC/DECRC state save/restore

**Color support:**
- **Named ANSI colors**: 16 basic colors (normal + bright)
- **256-color palette**: Extended palette (6×6×6 RGB cube + grayscale)
- **True color (24-bit)**: Full RGB with SGR 38/48 extended parameters
- **Default colors**: Foreground and background customizable

**When to use:** Terminal emulation details, scrolling architecture, color model, VTE integration.

---

### 03-state-machine.puml
**Application state machine with RigEngine support and command execution flow.**

- **AppMode enum**: Five states (Normal, WaitingLLM, AwaitingApproval, ExecutingCommand, AwaitingAnswer)
- **AppModeEvent enum**: Events triggering state transitions
- **State transitions**: Visualized as directed graph with edge labels

**States:**
1. **Normal**: Default state, user can type, terminal live
2. **WaitingLLM**: Querying RigEngine backend, terminal frozen, animated throbber
3. **AwaitingApproval**: RigEngine requested command approval via tool call (HITL), waiting for y/n
4. **ExecutingCommand**: Command approved by user, executing in PTY, capturing output (NEW with RigEngine)
5. **AwaitingAnswer**: RigEngine asked question, waiting for text input

**Valid transitions (RigEngine flow):**
- Normal → WaitingLLM (user types `? query`)
- WaitingLLM → Normal (no further action needed)
- WaitingLLM → AwaitingApproval (tool call: execute_shell_command)
- WaitingLLM → AwaitingAnswer (tool call: ask_user)
- AwaitingApproval → ExecutingCommand (user approved, command sent to PTY)
- ExecutingCommand → WaitingLLM (needs_continuation=true, agent continues with output)
- ExecutingCommand → Normal (needs_continuation=false, output is final answer)
- AwaitingAnswer → WaitingLLM (user answered, agent continues)
- Any state → Normal (Cancel - Ctrl+C)

**Implementation details:**
- **Enum variants carry data**: AwaitingApproval stores command+message
- **Consumed on transition**: Data moved, not cloned
- **Type safety**: can_transition_to() validates before transition
- **Atomic transitions**: transition() method returns Result<AppMode>
- **Idempotent**: Same-state transitions allowed (e.g., Normal → Normal)

**When to use:** LLM integration design, state machine documentation, error handling flow.

---

### 04-data-flow.puml
**Sequence diagram showing complete data flow paths through the system.**

**Keyboard input path:**
```
User → egui::Context → KeyboardHandler → InfrawareApp
→ PtyWriter → PTY kernel → Shell
```

**Shell output path:**
```
Shell → PTY kernel → mpsc channel → InfrawareApp
→ vte::Parser → TerminalHandler → TerminalGrid
```

**Rendering path:**
```
TerminalGrid → egui rendering functions
→ Painter → egui::Context → display
```

**Key interactions:**
1. **Keyboard processing**: Event extraction, modifier checking, text input
2. **PTY output handling**: Polling (rate-limited), byte accumulation, VTE parsing
3. **Terminal state updates**: SGR processing, cursor movement, scrolling
4. **Rendering**: Grid access, batching, single-pass iteration

**Performance optimizations:**
- **Blocking read**: Dedicated thread sleeps until data available (no polling)
- **Backpressure**: sync_channel with capacity 32 (kernel buffer fills if reader slow)
- **Rate limiting**: MAX_BYTES_PER_FRAME per frame (~4KB typical)
- **Pauseable**: 500ms pause after Ctrl+C to let kernel process signal
- **Reactive repaint**: Only repaint if data or user input

**When to use:** Data flow understanding, performance analysis, debugging I/O issues.

---

### 05-render-pipeline.puml
**Activity diagram documenting the egui rendering pipeline with optimizations.**

**Render phases:**
1. **Startup & State**: Theme application, focus tracking, cursor blink
2. **Input & PTY**: Keyboard polling, PTY output, VTE parsing
3. **Layout**: Available space calculation, terminal size computation
4. **Resize**: PTY resize with debounce, coordinate caching
5. **Single-pass rendering**: Per-row cell iteration with batching
6. **Z-order drawing**: Backgrounds → text → decorations → cursor → scrollbar
7. **Mouse input**: Wheel scrolling and potential drag support
8. **Reactive repaint**: Smart scheduling based on changes

**Single-pass rendering:**
```
Traditional (3 passes):
  For each cell: render background
  For each cell: render text
  For each cell: render decoration

Optimized (1 pass):
  For each cell (once):
    - Batch background (same color consecutive cells)
    - Batch text run (same color consecutive chars)
    - Collect decoration
  Render backgrounds (one rect per batch)
  Render text (one text call per batch)
  Render decorations (one line per cell)
```

**Batching strategy:**
- **Backgrounds**: Group adjacent same-color cells → single rect_filled()
- **Text**: Accumulate consecutive chars of same color → single text() call
- **Decorations**: Collect (not batched, rare) → individual line() calls

**Performance characteristics:**
- **Best case**: Solid color → 1 draw call + decorations
- **Typical**: ~10-20 batches per 80-char row
- **Optimization**: Pre-calculated column X coordinates, cached FontId

**Reactive repaint:**
- **Focused window**: Repaint if PTY data, user input, or cursor blink time
- **Idle**: Schedule repaint only for next cursor blink (~530ms)
- **Unfocused**: Very low FPS (2 Hz) to save CPU

**CPU usage impact:**
- **Active**: ~30-50 FPS with PTY output
- **Idle with focus**: ~2 FPS (cursor blink only)
- **Unfocused**: ~2 FPS (low power mode)

**When to use:** Rendering performance optimization, frame rate analysis, CPU usage reduction.

---

## File Locations

All diagrams are generated in PlantUML format (.puml):

```
docs/uml/
  ├── 00-architecture-overview.puml     (2.5 KB - 110 lines)
  ├── 01-pty-module.puml                (3.2 KB - 140 lines)
  ├── 02-terminal-module.puml           (4.8 KB - 220 lines)
  ├── 03-state-machine.puml             (2.1 KB - 90 lines)
  ├── 04-data-flow.puml                 (4.5 KB - 200 lines)
  ├── 05-render-pipeline.puml           (5.2 KB - 240 lines)
  └── README.md                          (this file)
```

## Viewing Diagrams

### Online Viewers
- **PlantUML Editor**: http://www.plantuml.com/plantuml/uml/
- **Plant Text**: https://www.plantuml.com/plantuml/
- **GitHub**: Native rendering in .puml files

### Local Rendering
```bash
# Using plantuml CLI
plantuml docs/uml/00-architecture-overview.puml -tpng -o docs/uml/rendered/

# Using VS Code extension
# Install PlantUML extension, right-click .puml file → Preview
```

### IDE Support
- **VS Code**: PlantUML extension (Alt+D) for live preview
- **IntelliJ**: Built-in PlantUML support
- **GitHub**: Automatic rendering in markdown

## RigEngine Integration

**RigEngine** is the primary LLM backend for Infraware Terminal, providing:
- Native Rust agent using rig-rs framework
- Direct Anthropic Claude API integration
- HITL (Human-in-the-Loop) tool calling for command approval
- needs_continuation flag for intelligent command result handling
- Tighter integration with the terminal state machine via ExecutingCommand state

The state machine diagrams reflect the complete RigEngine flow, including the new ExecutingCommand state that captures shell output and determines whether to resume the agent loop or complete the interaction.

## Architecture Highlights

### Thread Safety
- **Main thread**: eframe event loop, UI rendering
- **PTY reader thread**: Dedicated thread with atomic stop flag
- **Tokio runtime**: Async resize and signal operations
- **Synchronization**: Arc/Mutex for shared state, channels for message passing

### Performance Optimizations
1. **Single-pass rendering**: Batch backgrounds/text, reduce draw calls
2. **Reactive repaint**: Only repaint when data/input/blink
3. **Blocking I/O**: Dedicated thread sleeps (no polling overhead)
4. **Backpressure**: sync_channel prevents runaway PTY reading
5. **Caching**: Pre-calculated coordinates, cached font IDs
6. **Rate limiting**: MAX_BYTES_PER_FRAME per frame

### Design Patterns
- **State Machine**: AppMode enum with type-safe transitions
- **Trait abstraction**: PtyWrite/PtyControl for DI and testing
- **Builder pattern**: PtySessionConfig for flexible PTY creation
- **Visitor pattern**: vte::Perform trait for escape sequence handling

### Rust-Specific Features
- **Ownership model**: Clear ownership of resources (master PTY, child process)
- **Trait objects**: Dynamic dispatch for PTY system (platform abstraction)
- **Error handling**: Result-based error propagation with context
- **Lifetime management**: Arc/Mutex for safe shared mutable state

## Development Notes

### Adding New Features
1. **State machine change**: Modify AppMode enum and transition rules
2. **PTY enhancement**: Extend PtyManager/PtySession APIs
3. **Terminal feature**: Add methods to TerminalGrid or update SGR handling
4. **Keyboard shortcut**: Extend KeyboardHandler.process_other_keys()
5. **Rendering optimization**: Update single-pass batching logic

### Common Patterns
```rust
// State transition (type-safe)
let new_mode = current_mode.transition(event)?;

// PTY I/O (async/sync dual-mode)
writer.write_sync(data)?;  // Sync context
writer.write(data).await?; // Async context

// Grid access (immutable + mutable)
let grid = handler.grid();           // Immutable borrow
let grid_mut = handler.grid_mut();   // Mutable borrow

// Rendering (single-pass batching)
for cell in row.iter() {
    // Batch backgrounds and text
    // Collect decorations
}
// Render in z-order using collected data
```

## References

- **VTE Parser**: https://docs.rs/vte/latest/vte/
- **Portable PTY**: https://docs.rs/portable-pty/latest/portable_pty/
- **egui**: https://docs.rs/egui/latest/egui/
- **Tokio**: https://docs.rs/tokio/latest/tokio/

## Document History

- **2025-12-31**: Initial UML diagram generation
  - 6 comprehensive diagrams
  - 1 README index
  - Complete architecture documentation

---

**Last Updated**: 2025-12-31
**Format**: PlantUML 1.2024.3+
**Theme**: Plain with custom styling
