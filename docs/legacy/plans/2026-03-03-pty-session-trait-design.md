# PtySession Trait Design

## Problem

Command execution is split across two paths:

1. PTY path (terminal UI): HITL-approved commands sent as raw bytes to a local PTY, output captured via prompt detection.
2. Engine-internal path (ShellCommandTool): `tokio::process::Command` spawns `sh -c <cmd>` directly when `require_approval=false`.

This duplication blocks three goals:

- Running commands on remote hosts (SSH, Kubernetes) transparently.
- Easy testability of the AI integration (mock sessions without real PTY).
- A single, consistent execution path for all commands.

## Decision

Introduce a `PtySession` trait that abstracts interactive terminal sessions. All command execution flows through this single abstraction.

## Design

### Core Trait

```rust
#[async_trait]
pub trait PtySession: Send + Sync + Debug {
    /// Take a reader that streams output bytes to the given channel.
    /// Can only be called once per session.
    async fn take_reader(&mut self, sender: SyncSender<Vec<u8>>) -> Result<PtyReader>;

    /// Take a writer handle for sending input bytes.
    /// Can only be called once per session.
    async fn take_writer(&mut self) -> Result<Arc<PtyWriter>>;

    /// Resize the terminal dimensions.
    async fn resize(&self, rows: u16, cols: u16) -> Result<()>;

    /// Send an interrupt signal (SIGINT / Ctrl+C equivalent).
    fn send_sigint(&self) -> Result<()>;

    /// Check if the remote process is still alive.
    async fn is_running(&self) -> bool;

    /// Kill the session.
    async fn kill(&self) -> Result<()>;
}
```

`PtyReader` and `PtyWriter` stay as concrete types. They already accept `Box<dyn Read + Send>` / `Box<dyn Write + Send>`, so any backend provides its own streams.

### PtyManager Changes

`PtyManager` holds `Box<dyn PtySession>` instead of a concrete session:

```rust
pub struct PtyManager {
    session: Box<dyn PtySession>,
    current_size: PtySize,
    label: String,
}

impl PtyManager {
    pub fn new(session: Box<dyn PtySession>, label: String, size: PtySize) -> Self { ... }
    pub async fn local() -> Result<Self> { ... }
    pub async fn local_with_size(size: PtySize) -> Result<Self> { ... }
}
```

All delegate methods (`take_reader`, `take_writer`, `resize`, `send_sigint`, `kill`) forward to `self.session`.

### LocalPtySession

Current `PtySession` struct renamed to `LocalPtySession`, moved to `src/pty/adapters/local/`. The `Pty` native system wrapper becomes an internal detail.

```rust
pub struct LocalPtySession {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    reader: Option<PtyReader>,
    writer: Option<PtyWriter>,
}

impl LocalPtySession {
    pub fn spawn(size: PtySize) -> Result<Self> { ... }
    pub fn spawn_command(command: &str, args: &[&str], size: PtySize) -> Result<Self> { ... }
}

impl PtySession for LocalPtySession { ... }
```

### File Layout

```
src/pty/
├── mod.rs              # re-exports, module declarations
├── traits.rs           # PtySession trait + PtyWrite trait
├── manager.rs          # PtyManager (holds Box<dyn PtySession>)
├── io.rs               # PtyReader, PtyWriter (unchanged)
└── adapters/
    ├── mod.rs           # re-exports adapters
    └── local/
        ├── mod.rs       # LocalPtySession + Pty spawn logic
        └── ...
```

Future adapters follow the same pattern:

```
src/pty/adapters/
├── local/
├── ssh/        # future
└── k8s/        # future
```

### Impact on Existing Code

| Component | Change |
|-----------|--------|
| `TerminalSession` | None. Still holds `Option<Arc<PtyWriter>>` and `Option<PtyReader>`. |
| `SessionManager` | `PtyManager::new()` → `PtyManager::local()` (rename). |
| `ShellCommandTool` | Remove `require_approval=false` / `tokio::process::Command` path. Tool becomes pure schema + validation, always returns `PendingApproval`. |
| `PtyWrite` trait | Unchanged. Coexists with `PtySession` (different purpose). |
| `PtyReader` / `PtyWriter` | Unchanged. |
| `AppMode` state machine | Unchanged. |
| `AgenticEngine` trait | Unchanged. |
| HITL flow | Unchanged (only internal ShellCommandTool path removed). |

### Testing

`MockPtySession` can be added for integration tests: scripted byte sequences in/out, no real PTY needed. Allows testing the full HITL loop including output capture and prompt detection.
