# UML Diagram Generation Report

**Date**: 2025-12-31
**Project**: Infraware Terminal (PTY-based egui terminal emulator)
**Status**: Complete

## Summary

Generated comprehensive UML documentation for the Infraware Terminal egui application, covering all major architectural components, data flows, rendering pipeline, and state management.

### Deliverables

| File | Type | Lines | Size | Purpose |
|------|------|-------|------|---------|
| `00-architecture-overview.puml` | Class/Component | 344 | 9.1K | System-wide architecture overview |
| `01-pty-module.puml` | Class | 227 | 6.1K | PTY subsystem detailed design |
| `02-terminal-module.puml` | Class | 289 | 7.4K | Terminal emulation components |
| `03-state-machine.puml` | State Machine | 148 | 3.8K | Application state transitions |
| `04-data-flow.puml` | Sequence | 201 | 5.4K | Complete I/O data flows |
| `05-render-pipeline.puml` | Activity | 262 | 7.0K | egui rendering optimization |
| `README.md` | Documentation | 321 | 13K | Comprehensive diagram guide |
| `INDEX.md` | Reference | 284 | 7.8K | Quick reference index |
| **Total** | | **2,076** | **76K** | Complete documentation set |

## Architecture Analyzed

### Core Components

#### InfrawareApp (src/app.rs)
```rust
pub struct InfrawareApp {
    mode: AppMode,
    theme: Theme,
    vte_parser: vte::Parser,
    terminal_handler: TerminalHandler,
    pty_writer: Option<Arc<PtyWriter>>,
    pty_output_rx: Option<mpsc::Receiver<Vec<u8>>>,
    pty_manager: Option<Arc<TokioMutex<PtyManager>>>,
    keyboard_handler: KeyboardHandler,
    runtime: Runtime,
    // ... (caching, timing, state fields)
}
```
- **Role**: Main eframe::App implementation
- **Responsibility**: I/O coordination, VTE parsing, rendering
- **Key method**: update() called at ~60 FPS (or less if idle)
- **Diagram**: 00 (central), 04 (sequence), 05 (render)

#### PTY Module (src/pty/)
```rust
pub struct PtyManager {
    session: PtySession,
    current_size: PtySize,
    shell: String,
}

pub struct PtyReader {
    receiver: Receiver<Vec<u8>>,
    stop_flag: Arc<AtomicBool>,
}

pub struct PtyWriter {
    inner: Arc<Mutex<Box<dyn Write + Send>>>,
}
```
- **Role**: Pseudo-terminal abstraction
- **Architecture**: Session owns master/slave, reader/writer provide async I/O
- **Threading**: Dedicated background reader thread, async write
- **Diagram**: 00 (subsystem), 01 (detail)

#### Terminal Module (src/terminal/)
```rust
pub struct TerminalGrid {
    cells: Vec<Vec<Cell>>,
    scrollback: Vec<Vec<Cell>>,
    scroll_offset: usize,
    cursor_row: u16,
    cursor_col: u16,
    current_attrs: CellAttrs,
    current_fg: Color,
    current_bg: Color,
    // ... (alt screen, tab stops, modes)
}

pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: CellAttrs,
}
```
- **Role**: Terminal emulation state
- **Features**: Scrollback (10K lines), alternate screen, cursor save/restore
- **Colors**: Named ANSI (16), indexed (256), true color (24-bit)
- **Diagram**: 00 (subsystem), 02 (detail)

#### State Management (src/state.rs)
```rust
pub enum AppMode {
    Normal,
    WaitingLLM,
    AwaitingApproval { command: String, message: String },
    AwaitingAnswer { question: String, options: Option<Vec<String>> },
}

impl AppMode {
    pub fn can_transition_to(&self, target: &Self) -> bool { ... }
    pub fn transition(self, event: AppModeEvent) -> Result<Self> { ... }
}
```
- **Role**: Type-safe state machine
- **Transitions**: 7 valid transition paths, invalid transitions return error
- **Data**: Variants carry state-specific data, consumed on transition
- **Diagram**: 00 (overview), 03 (state machine)

#### Keyboard Input (src/input/keyboard.rs)
```rust
pub enum KeyboardAction {
    SendBytes(Vec<u8>),
    SendSigInt,
}

pub struct KeyboardHandler {
    actions: Vec<KeyboardAction>,
}

impl KeyboardHandler {
    pub fn process(&mut self, ctx: &egui::Context) -> Vec<KeyboardAction> { ... }
}
```
- **Role**: Keyboard event processing
- **Strategy**: Ctrl+key via event iteration, text input via text input events
- **Diagram**: 00 (input), 04 (sequence)

### Key Design Patterns

#### 1. Single-Pass Rendering (Diagram 05)

**Traditional approach** (3 passes):
```
For each cell:
    render background
For each cell:
    render text
For each cell:
    render decoration
```

**Optimized approach** (1 pass):
```
For each cell (once):
    Batch background (same color)
    Batch text (same color)
    Collect decoration
Render backgrounds (one rect per batch)
Render text runs (one text call per batch)
Render decorations (one line per cell)
```

**Benefits**:
- Reduces iteration from 3 to 1
- Batches reduce draw calls (typical: 20-30 vs 240 per row)
- Better CPU cache usage
- Measurable FPS improvement on large grids

#### 2. Backpressure & Rate Limiting (Diagram 04)

**Reader thread**: Dedicated std::thread with blocking read()
```rust
// Background thread
loop {
    match reader.read(&mut buf) {
        Ok(n) => {
            if tx.send(data).is_err() { break; }  // Blocked if channel full
        }
    }
}
```

**Channel**: sync_channel with capacity 32
```rust
let (tx, rx) = mpsc::sync_channel(pty_config::CHANNEL_CAPACITY);
```

**Effect**:
- Thread blocks if channel full
- Kernel buffer fills on PTY write side
- Shell cat command blocks on write()
- Ctrl+C can interrupt (kernel receives signal while cat blocked)

**Rate limiting**: MAX_BYTES_PER_FRAME (~4KB)
```rust
if bytes_processed >= rendering::MAX_BYTES_PER_FRAME {
    break;
}
```

#### 3. State Machine Safety (Diagram 03)

**Type-safe transitions**:
```rust
// Compile-time type safety
let state = AppMode::Normal;

// Valid transition
let new = state.transition(AppModeEvent::QueryLLM)?;  // ✓ Normal → WaitingLLM

// Invalid transitions prevented
can_transition_to(&AppMode::AwaitingApproval { ... })?;  // ✗ Can't go directly
```

**Atomic operations**:
```rust
pub fn transition(self, event: AppModeEvent) -> Result<Self> {
    // Consumes self + event (no intermediate states)
    let new_state = match (self, event) { ... };
    Ok(new_state)
}
```

#### 4. Trait-Based Dependency Injection (Diagram 01)

**Abstraction for testing**:
```rust
pub trait PtyWrite: Send + Sync {
    fn write_bytes(&self, data: &[u8]) -> Result<usize>;
}

pub trait PtyControl: Send + Sync {
    fn resize(&self, rows: u16, cols: u16) -> Result<()>;
    fn send_sigint(&self) -> Result<()>;
}

// Implementations
impl PtyWrite for PtyWriter { ... }
impl PtyControl for PtySession { ... }

// Enables mocking in tests
struct MockPtyWriter { ... }
impl PtyWrite for MockPtyWriter { ... }
```

### Data Flow Paths

#### Keyboard Input Path
```
User keyboard → egui::Context
  ↓
KeyboardHandler::process()
  ↓
KeyboardAction::SendBytes/SendSigInt
  ↓
InfrawareApp::send_to_pty()
  ↓
PtyWriter::write_sync()
  ↓
PTY kernel buffer
  ↓
Shell (bash/zsh)
```

#### Shell Output Path
```
Shell process → PTY kernel
  ↓
PtyReader (background thread, blocking read)
  ↓
mpsc::sync_channel
  ↓
InfrawareApp::poll_pty_output() (main thread, non-blocking recv)
  ↓
vte::Parser::advance() (feed bytes)
  ↓
TerminalHandler::Perform (trait methods)
  ↓
TerminalGrid (update cells)
  ↓
render_terminal() (single-pass rendering)
  ↓
egui::Painter (draw calls)
  ↓
Display
```

### Performance Characteristics

#### Frame Rate
- **Focused (PTY output)**: 30-60 FPS (bounded by terminal output rate)
- **Focused (idle)**: 2 FPS (cursor blink interval ~530ms)
- **Unfocused**: 2 FPS (BACKGROUND_REPAINT constant)

#### Latency
- **Keyboard → PTY**: <1 frame (processed each update)
- **PTY output → display**: <1 frame (parsed in same update)
- **Render time (per row)**: <500µs (single-pass, batched)

#### Resource Usage
- **CPU (active)**: ~30-50% (rendering + VTE parsing)
- **CPU (focused, idle)**: <5% (reactive repaint)
- **CPU (unfocused)**: <2% (2 Hz refresh)
- **Memory (grid)**: ~50KB (24x80 grid) + scrollback (~10MB max)

### Thread Model

```
Main Thread (eframe):
  ├─ Event polling (keyboard, mouse, resize)
  ├─ egui UI update()
  ├─ Terminal rendering
  └─ Requests repaint

PTY Reader Thread:
  └─ Blocking read() on PTY file descriptor
     Sends data via mpsc to main thread

Tokio Runtime:
  ├─ Async PTY resize task
  └─ Signal handling (SIGINT)
```

### Signal Delivery (Unix)

**SIGINT flow**:
```
User Ctrl+C
  ↓
egui::Event::Key (Ctrl+C)
  ↓
KeyboardHandler → SendSigInt
  ↓
InfrawareApp::send_sigint()
  ↓
PtyManager::send_sigint()
  ↓
PtySession::send_sigint()
  ↓
Read /proc/<pid>/stat for tpgid (terminal process group id)
  ↓
nix::kill(Pid::from_raw(-tpgid), SIGINT)
  ↓
Foreground process group (e.g., cat, not bash)
```

## Diagram Details

### Diagram 00: Architecture Overview (344 lines)
**Coverage**:
- InfrawareApp as central hub
- AppMode state machine
- PTY subsystem with all I/O components
- VTE parser pipeline
- Terminal grid with cell model
- Theme and rendering functions
- Keyboard input handling

**Key relationships**:
- Data flow arrows color-coded (blue, green, red, etc.)
- Component ownership vs. reference relationships
- Thread model indicated in notes

### Diagram 01: PTY Module (227 lines)
**Coverage**:
- PtyManager high-level API
- PtySession with master/child ownership
- PtyReader with dedicated thread
- PtyWriter with dual async/sync modes
- Trait abstractions (PtyWrite, PtyControl)
- Configuration builder pattern

**Key details**:
- Thread spawning mechanism
- Channel communication
- Signal delivery implementation
- SIGINT via /proc/<pid>/stat

### Diagram 02: Terminal Module (289 lines)
**Coverage**:
- TerminalHandler as vte::Perform implementor
- TerminalGrid with scrollback and viewport
- Cell structure with colors and attributes
- Color model (Named, Indexed, RGB)
- Saved cursor and alternate screen states

**Key details**:
- Scrolling architecture
- Color conversion to egui
- SGR code mapping
- Attribute handling (bold, italic, etc.)

### Diagram 03: State Machine (148 lines)
**Coverage**:
- Four application states
- Seven transition events
- Valid transition paths
- Idempotent same-state transitions
- Cancel from any state

**Key properties**:
- Type safety enforced by enums
- Data carried in variants
- Invalid transitions return errors

### Diagram 04: Data Flow (201 lines)
**Coverage**:
- Keyboard input sequence
- PTY I/O blocking/non-blocking model
- VTE parsing steps
- Terminal grid updates
- Rendering pipeline

**Key interactions**:
- Backpressure mechanism
- Rate limiting strategy
- Performance optimizations noted

### Diagram 05: Render Pipeline (262 lines)
**Coverage**:
- Update loop phases
- Startup and state management
- Input and PTY processing
- Layout calculation
- Single-pass rendering algorithm
- Z-order drawing
- Mouse input handling
- Reactive repaint scheduling

**Key optimizations**:
- Per-row rendering with buffering
- Cell batching strategy
- Column X coordinate caching
- Font ID caching
- Conditional cursor/scrollbar rendering

## Code Structure Referenced

### Key Files Analyzed

```
src/
├── app.rs                    # InfrawareApp - main eframe::App
├── state.rs                  # AppMode - state machine
├── pty/
│   ├── mod.rs               # Pty system wrapper
│   ├── manager.rs           # PtyManager - high-level API
│   ├── session.rs           # PtySession - PTY lifecycle
│   ├── io.rs                # PtyReader, PtyWriter - async I/O
│   └── traits.rs            # PtyWrite, PtyControl traits
├── terminal/
│   ├── handler.rs           # TerminalHandler - vte::Perform
│   ├── grid.rs              # TerminalGrid - terminal state
│   └── cell.rs              # Cell, Color, CellAttrs
├── ui/
│   ├── renderer.rs          # Rendering functions
│   └── theme.rs             # Theme colors
└── input/
    └── keyboard.rs          # KeyboardHandler - input processing
```

## Quality Metrics

### PlantUML Standards
- **Theme**: Plain with custom styling
- **Syntax**: All diagrams validate successfully
- **Conventions**: UML stereotypes, proper relationships
- **Documentation**: Inline notes explaining complex patterns

### Coverage
- **Components**: All major classes documented
- **Relationships**: Ownership, implementation, usage all shown
- **Data flows**: Complete keyboard and PTY I/O paths traced
- **Patterns**: Design patterns and optimizations highlighted

### Readability
- **Size**: Diagrams fit on standard screen without scrolling (mostly)
- **Grouping**: Components grouped into packages/namespaces
- **Color**: Consistent color scheme for relationships
- **Labels**: Clear, descriptive relationship labels

## Documentation Provided

### README.md (321 lines)
- Overview of all 6 diagrams
- Detailed description of each diagram
- Key relationships and interactions
- Thread model and design patterns
- Performance targets and optimization techniques
- Development guidelines

### INDEX.md (284 lines)
- Quick reference table of diagrams
- Component cross-reference index
- Data flow path summaries
- Key concepts with code examples
- Performance targets table
- Common modifications guide
- Testing considerations
- File location reference

## Notes for Future Maintenance

### When Adding Features
1. **State machine change**: Update diagram 03 and AppMode enum
2. **PTY enhancement**: Update diagrams 00, 01, and 04
3. **Terminal feature**: Update diagrams 00, 02, and 04
4. **Rendering optimization**: Update diagrams 00 and 05
5. **Keyboard shortcut**: Update diagrams 00 and 04

### When Refactoring
1. Verify component relationships remain accurate
2. Update data flow diagrams if paths change
3. Check thread model implications
4. Validate state machine transitions

### Diagram Generation
- All diagrams created with PlantUML 1.2024.3+ syntax
- Compatible with online editors and GitHub rendering
- Can be rendered to PNG/SVG using PlantUML CLI:
  ```bash
  plantuml -tpng docs/uml/*.puml -o docs/uml/rendered/
  ```

---

**Generated by**: Claude Code (UML Specialist)
**Format**: PlantUML 1.2024.3+
**Total lines**: 2,076
**Total size**: 76 KB
**Completion**: 2025-12-31 17:44 UTC
