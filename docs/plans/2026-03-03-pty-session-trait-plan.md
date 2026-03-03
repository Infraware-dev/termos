# PtySession Trait Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Extract a `PtySession` trait from the current concrete `PtySession` struct, move the local implementation to `src/pty/adapters/local/`, and make `PtyManager` hold `Box<dyn PtySession>` for runtime polymorphism.

**Architecture:** The current `PtySession` struct becomes `LocalPtySession` inside `src/pty/adapters/local/`. A new `PtySession` trait is defined in `src/pty/traits.rs`. `PtyManager` accepts any `Box<dyn PtySession>` and delegates all session operations. The `Pty` spawn helper becomes an internal detail of `LocalPtySession`. The `ShellCommandTool`'s internal `tokio::process::Command` execution path is removed — the tool always returns `PendingApproval`.

**Tech Stack:** Rust 2024, async-trait, portable-pty, tokio

**Conventions:** @rust-conventions, @cargo-toml-conventions

---

### Task 1: Define PtySession trait in traits.rs

**Files:**
- Modify: `src/pty/traits.rs`

**Step 1: Write the PtySession trait**

Add the trait definition below the existing `PtyWrite` trait:

```rust
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use async_trait::async_trait;
use portable_pty::PtySize;

use super::io::{PtyReader, PtyWriter};

/// An interactive terminal session on a host (local, SSH, K8s, mock).
///
/// Implementations wrap the transport layer (local PTY, SSH channel, K8s exec)
/// and provide byte-level I/O via `PtyReader`/`PtyWriter`.
#[async_trait]
pub trait PtySession: Send + Sync + std::fmt::Debug {
    /// Take a reader that streams output bytes to the given channel.
    ///
    /// Can only be called once per session — returns error on second call.
    async fn take_reader(&mut self, sender: SyncSender<Vec<u8>>) -> anyhow::Result<PtyReader>;

    /// Take a writer handle for sending input bytes.
    ///
    /// Can only be called once per session — returns error on second call.
    async fn take_writer(&mut self) -> anyhow::Result<Arc<PtyWriter>>;

    /// Resize the terminal dimensions.
    async fn resize(&self, size: PtySize) -> anyhow::Result<()>;

    /// Send an interrupt signal (SIGINT / Ctrl+C equivalent).
    fn send_sigint(&self) -> anyhow::Result<()>;

    /// Check if the remote process is still alive.
    async fn is_running(&self) -> bool;

    /// Kill the session.
    async fn kill(&self) -> anyhow::Result<()>;
}
```

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | head -30`
Expected: compiles (trait is defined but not yet used)

**Step 3: Commit**

```
feat(pty): define PtySession trait for pluggable session backends
```

---

### Task 2: Create LocalPtySession adapter

**Files:**
- Create: `src/pty/adapters/mod.rs`
- Create: `src/pty/adapters/local/mod.rs`
- Modify: `src/pty.rs` (add `mod adapters`, move `Pty` struct, update re-exports)

**Step 1: Create adapters module root**

Create `src/pty/adapters/mod.rs`:

```rust
//! PTY session adapters.

pub mod local;
```

**Step 2: Create LocalPtySession**

Create `src/pty/adapters/local/mod.rs`. Move the contents of `src/pty/session.rs` (the struct + impl, NOT `PtySessionConfig`) and the `Pty` helper struct from `src/pty.rs` into this file. Rename the struct from `PtySession` to `LocalPtySession`.

The `Pty` struct becomes a private helper inside this module. `PtySessionConfig` moves here too (it's only used in tests currently and is specific to the spawn API).

```rust
//! Local PTY session using the native platform PTY system.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use portable_pty::{
    Child, CommandBuilder, MasterPty, PtyPair, PtySize, PtySystem, native_pty_system,
};
use tokio::sync::Mutex;

use crate::pty::io::{PtyReader, PtyWriter};
use crate::pty::traits::PtySession;

/// Local PTY session backed by a native platform pseudo-terminal.
pub struct LocalPtySession {
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    reader: Option<PtyReader>,
    writer: Option<PtyWriter>,
}

impl std::fmt::Debug for LocalPtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalPtySession")
            .field("reader", &self.reader)
            .field("writer", &self.writer)
            .finish_non_exhaustive()
    }
}

impl LocalPtySession {
    /// Create from a PTY pair and child process.
    pub(crate) fn new(pair: PtyPair, child: Box<dyn Child + Send + Sync>) -> Self {
        Self {
            master: Arc::new(Mutex::new(pair.master)),
            child: Arc::new(Mutex::new(child)),
            reader: None,
            writer: None,
        }
    }

    /// Spawn an interactive shell, auto-detecting zsh > bash > sh.
    pub fn spawn_shell(size: PtySize) -> Result<Self> {
        let shell = detect_shell()?;
        tracing::info!("Spawning local PTY with shell: {shell}");
        spawn_command(&shell, &["-i"], size)
    }

    /// Spawn a specific command in a local PTY.
    pub fn spawn_command<S: AsRef<OsStr>>(
        cmd: &str,
        args: &[S],
        size: PtySize,
    ) -> Result<Self> {
        spawn_command(cmd, args, size)
    }

    /// Send SIGINT via raw fd (platform-specific implementation).
    #[cfg(unix)]
    fn send_sigint_impl(&self) -> Result<()> {
        match self.master.try_lock() {
            Ok(master) => {
                if let Some(raw_fd) = master.as_raw_fd() {
                    let result = unsafe { libc::write(raw_fd, [0x03].as_ptr().cast(), 1) };
                    if result == 1 {
                        tracing::debug!("Sent Ctrl+C (0x03) to PTY fd {raw_fd}");
                        Ok(())
                    } else {
                        let err = std::io::Error::last_os_error();
                        tracing::warn!("Failed to write Ctrl+C to PTY: {err}");
                        Err(anyhow::anyhow!("Failed to write Ctrl+C to PTY: {err}"))
                    }
                } else {
                    tracing::warn!("No raw fd available from master PTY");
                    Ok(())
                }
            }
            Err(_) => {
                tracing::warn!("Could not lock master PTY for SIGINT");
                Ok(())
            }
        }
    }

    #[cfg(not(unix))]
    fn send_sigint_impl(&self) -> Result<()> {
        tracing::warn!("SIGINT not supported on this platform");
        Ok(())
    }
}

#[async_trait]
impl PtySession for LocalPtySession {
    async fn take_reader(
        &mut self,
        sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    ) -> Result<PtyReader> {
        if self.reader.is_none() {
            let master = self.master.lock().await;
            let reader = master
                .try_clone_reader()
                .context("Failed to clone PTY reader")?;
            self.reader = Some(PtyReader::new(reader, sender));
        }
        self.reader
            .take()
            .context("Reader already taken - can only be called once")
    }

    async fn take_writer(&mut self) -> Result<Arc<PtyWriter>> {
        if self.writer.is_none() {
            let master = self.master.lock().await;
            let writer = master.take_writer().context("Failed to take PTY writer")?;
            self.writer = Some(PtyWriter::new(writer));
        }
        self.writer
            .take()
            .map(Arc::new)
            .context("Writer already taken - can only be called once")
    }

    async fn resize(&self, size: PtySize) -> Result<()> {
        let master = self.master.lock().await;
        master
            .resize(size)
            .context("Failed to resize PTY")
    }

    fn send_sigint(&self) -> Result<()> {
        self.send_sigint_impl()
    }

    async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        child.try_wait().ok().flatten().is_none()
    }

    async fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().context("Failed to kill child process")
    }
}

// --- Private helpers ---

const SHELL_PRIORITY: &[&str] = &["zsh", "bash", "sh"];

fn detect_shell() -> Result<String> {
    for shell in SHELL_PRIORITY {
        if which::which(shell).is_ok() {
            return Ok((*shell).to_string());
        }
    }
    anyhow::bail!("No supported shell found (tried: {SHELL_PRIORITY:?})")
}

fn spawn_command<S: AsRef<OsStr>>(cmd: &str, args: &[S], size: PtySize) -> Result<LocalPtySession> {
    let system = native_pty_system();
    let pair = system.openpty(size)?;

    let mut builder = CommandBuilder::new(cmd);
    for arg in args {
        builder.arg(arg);
    }
    if let Ok(cwd) = std::env::current_dir() {
        builder.cwd(cwd);
    }
    for (key, value) in std::env::vars() {
        builder.env(key, value);
    }
    builder.env("TERM", "xterm-256color");
    builder.env("COLUMNS", size.cols.to_string());
    builder.env("LINES", size.rows.to_string());

    let child = pair.slave.spawn_command(builder)?;
    Ok(LocalPtySession::new(pair, child))
}

/// Configuration for spawning a PTY session (builder pattern).
#[derive(Debug, Clone)]
#[expect(dead_code, reason = "Builder pattern API for future PTY spawn customization")]
pub struct PtySessionConfig {
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: Option<PathBuf>,
    pub env: HashMap<String, String>,
    pub size: PtySize,
}

#[expect(dead_code, reason = "Builder pattern API for future PTY spawn customization")]
impl PtySessionConfig {
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            env: HashMap::new(),
            size: crate::pty::DEFAULT_PTY_SIZE,
        }
    }

    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    #[must_use]
    pub fn working_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(path.into());
        self
    }

    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    #[must_use]
    pub fn size(mut self, rows: u16, cols: u16) -> Self {
        self.size = PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        };
        self
    }
}
```

**Step 3: Update src/pty.rs module root**

Replace the `Pty` struct and update re-exports. The `Pty` struct, `PtySession` struct, and `PtySessionConfig` all moved to `adapters/local/`. Re-export `LocalPtySession` and `PtySession` trait:

```rust
//! PTY (Pseudo-Terminal) module for interactive command support.

mod adapters;
mod io;
mod manager;
mod traits;

pub use adapters::local::LocalPtySession;
pub use io::{PtyReader, PtyWriter};
pub use manager::PtyManager;
use portable_pty::PtySize;
pub use traits::{PtySession, PtyWrite};

/// Default PTY size matching typical terminal dimensions.
pub const DEFAULT_PTY_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
};
```

**Step 4: Delete old session.rs**

Delete `src/pty/session.rs` — its content has moved to `src/pty/adapters/local/mod.rs`.

**Step 5: Verify it compiles**

Run: `cargo check 2>&1 | head -40`
Expected: May fail due to downstream references to old `PtySession` struct. That's expected — we fix those in Task 3.

**Step 6: Commit**

```
refactor(pty): move PtySession struct to adapters/local as LocalPtySession
```

---

### Task 3: Refactor PtyManager to hold Box<dyn PtySession>

**Files:**
- Modify: `src/pty/manager.rs`

**Step 1: Rewrite PtyManager**

Replace the current implementation. Shell detection logic moved to `LocalPtySession::spawn_shell()`, so `PtyManager` becomes a thin wrapper around `Box<dyn PtySession>`:

```rust
//! PTY Manager for managing terminal sessions.
//!
//! Holds a `Box<dyn PtySession>` for runtime polymorphism.
//! Use `PtyManager::local()` for local shell sessions.

use std::sync::Arc;

use anyhow::Result;
use portable_pty::PtySize;

use super::adapters::local::LocalPtySession;
use super::io::{PtyReader, PtyWriter};
use super::traits::PtySession;
use super::DEFAULT_PTY_SIZE;

/// Manager for a PTY session. Wraps any `PtySession` implementation.
pub struct PtyManager {
    session: Box<dyn PtySession>,
    current_size: PtySize,
    label: String,
}

impl std::fmt::Debug for PtyManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyManager")
            .field("label", &self.label)
            .field("current_size", &self.current_size)
            .field("session", &self.session)
            .finish()
    }
}

impl PtyManager {
    /// Create a PtyManager from any PtySession implementation.
    pub fn new(session: Box<dyn PtySession>, label: impl Into<String>, size: PtySize) -> Self {
        Self {
            session,
            current_size: size,
            label: label.into(),
        }
    }

    /// Spawn a local interactive shell (convenience constructor).
    pub fn local() -> Result<Self> {
        Self::local_with_size(DEFAULT_PTY_SIZE)
    }

    /// Spawn a local interactive shell with a specific size.
    pub fn local_with_size(size: PtySize) -> Result<Self> {
        let session = LocalPtySession::spawn_shell(size)?;
        Ok(Self::new(Box::new(session), "local", size))
    }

    /// Get the session label (e.g., "local", "ssh://user@host").
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Get the current terminal size.
    pub fn size(&self) -> PtySize {
        self.current_size
    }

    /// Get a reader for PTY output that sends to the provided channel.
    ///
    /// Can only be called once (takes ownership).
    pub async fn take_reader(
        &mut self,
        sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    ) -> Result<PtyReader> {
        self.session.take_reader(sender).await
    }

    /// Get a writer for PTY input.
    ///
    /// Can only be called once (takes ownership).
    pub async fn take_writer(&mut self) -> Result<Arc<PtyWriter>> {
        self.session.take_writer().await
    }

    /// Resize the terminal.
    pub async fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        if self.current_size.rows != rows || self.current_size.cols != cols {
            self.session
                .resize(PtySize {
                    rows,
                    cols,
                    pixel_width: 0,
                    pixel_height: 0,
                })
                .await?;
            self.current_size = PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            };
            tracing::debug!("PTY resized to {cols}x{rows}");
        }
        Ok(())
    }

    /// Check if the session is still running.
    pub async fn is_running(&self) -> bool {
        self.session.is_running().await
    }

    /// Kill the session.
    pub async fn kill(&self) -> Result<()> {
        self.session.kill().await
    }

    /// Send SIGINT to the session.
    pub fn send_sigint(&self) -> Result<()> {
        self.session.send_sigint()
    }
}
```

Note: `PtyManager::new()` was previously `async` because it spawned a shell. Now `local()` is sync because `LocalPtySession::spawn_shell()` is sync (uses `portable_pty` which is sync). The `async` was only needed because the old constructor called `with_size` which was async for no real reason — the pty spawn is synchronous.

**Step 2: Verify it compiles**

Run: `cargo check 2>&1 | head -40`
Expected: May fail at call sites that use `PtyManager::new().await` — fix in next task.

**Step 3: Commit**

```
refactor(pty): make PtyManager hold Box<dyn PtySession>
```

---

### Task 4: Update TerminalSession and call sites

**Files:**
- Modify: `src/session.rs` (update `PtyManager` construction from `PtyManager::new().await` to `PtyManager::local()`)

**Step 1: Update TerminalSession::new()**

In `src/session.rs`, find the `PtyManager::new().await` call (around line 128) and change it to `PtyManager::local()`. Since `local()` is now sync, it doesn't need `.await`. But it's called inside a `runtime_handle.block_on(async { ... })` block, so it can stay in the async block — just remove the `.await`.

The key change in the block_on closure:

```rust
// Before:
match PtyManager::new().await {
// After:
match PtyManager::local() {
```

Also update the `shell` extraction. The old `PtyManager` had `.shell()` returning the shell name. The new one has `.label()`. Adjust accordingly — the `label` for local sessions is `"local"`. If the session init code uses `.shell()` for display or the PS1 prompt, we may need to keep shell info. Check if `shell` is used beyond display.

Look at `src/session.rs` to see how `shell` is used after extraction:

The `shell` field in `TerminalSession` is used to display the shell name. For local sessions `LocalPtySession::spawn_shell()` prints the shell name via tracing but doesn't expose it. Options:
- Store the detected shell name in `PtyManager::label` (e.g., `"zsh"` instead of `"local"`)
- Or have `LocalPtySession` expose it

Simplest: have `LocalPtySession::spawn_shell()` return `(Self, String)` where the second element is the shell name. Then `PtyManager::local()` uses that as the label.

**Step 2: Also update any import that references old `PtySession` struct**

The import in `session.rs` line 19 currently is:
```rust
use crate::pty::{PtyManager, PtyReader, PtyWrite, PtyWriter};
```
This doesn't import `PtySession` directly, so no change needed here (it was only used by `PtyManager` internally).

**Step 3: Verify it compiles**

Run: `cargo check 2>&1 | head -40`
Expected: PASS

**Step 4: Run tests**

Run: `cargo test 2>&1 | tail -30`
Expected: All existing tests pass

**Step 5: Commit**

```
refactor(pty): update TerminalSession to use PtyManager::local()
```

---

### Task 5: Simplify ShellCommandTool (remove internal execution path)

**Files:**
- Modify: `src/engine/adapters/rig/tools/shell.rs`

**Step 1: Remove the execution path from ShellCommandTool**

Remove:
- The `execute()` method (lines 191-233)
- The `require_approval` field and builder method
- The `else` branch in `call()` that does direct execution
- The `tokio::process::Command` import
- The `Timeout` and `Duration` imports
- The `Stdio` import
- Tests that test direct execution (`test_execute_simple_command`, `test_execute_failing_command`)
- Tests for `require_approval` builder (`test_shell_tool_builder`)
- The `Executed` and `Failed` variants from `ShellCommandResult` (only `PendingApproval` remains)
- The `ShellError::Timeout` and `ShellError::ExecutionFailed` variants (only `DangerousCommand` and `NotApproved` remain)

The tool becomes pure schema + validation. `call()` always validates then returns `PendingApproval`:

```rust
fn call(
    &self,
    args: Self::Args,
) -> impl Future<Output = Result<Self::Output, Self::Error>> + Send {
    let command = args.command.clone();
    let explanation = args.explanation.clone();
    let needs_continuation = args.needs_continuation;

    async move {
        Self::validate_command(&command)?;
        Ok(ShellCommandResult::PendingApproval {
            command,
            explanation,
            needs_continuation,
        })
    }
}
```

Simplified struct:

```rust
#[derive(Debug, Clone)]
pub struct ShellCommandTool {
    /// Timeout hint in seconds (metadata for the frontend, not enforced here)
    pub timeout_secs: u64,
}
```

Keep `validate_command()` — it's still useful as a pre-check before even asking the user.

**Step 2: Check if ShellCommandResult::Executed or Failed are used elsewhere**

Search for `ShellCommandResult::Executed` and `ShellCommandResult::Failed` outside of `shell.rs`. If the orchestrator or other code deserializes these variants, they need updating too.

Based on the exploration, the orchestrator only checks for `PendingApproval` (via `handle_tool_intercept`). The `Executed`/`Failed` variants were only used in the tool's own `execute()` path. Safe to remove.

**Step 3: Verify it compiles**

Run: `cargo check 2>&1 | head -30`
Expected: PASS

**Step 4: Run tests**

Run: `cargo test -- shell 2>&1`
Expected: Remaining shell tests pass

**Step 5: Commit**

```
refactor(rig): simplify ShellCommandTool to pure schema + validation
```

---

### Task 6: Move tests and add PtySession trait tests

**Files:**
- Modify: `src/pty/adapters/local/mod.rs` (move tests from old session.rs)
- Modify: `src/pty/traits.rs` (add trait object test)

**Step 1: Add tests to LocalPtySession**

Move the existing `PtySession` tests from the old `session.rs` into `src/pty/adapters/local/mod.rs`, adjusting for the renamed type:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pty_session_config_builder() {
        let config = PtySessionConfig::new("ssh")
            .args(["user@host", "-p", "22"])
            .working_dir("/tmp")
            .env("MY_VAR", "value")
            .size(40, 120);

        assert_eq!(config.command, "ssh");
        assert_eq!(config.args, vec!["user@host", "-p", "22"]);
        assert_eq!(config.working_dir, Some(PathBuf::from("/tmp")));
        assert_eq!(config.env.get("MY_VAR"), Some(&"value".to_string()));
        assert_eq!(config.size.rows, 40);
        assert_eq!(config.size.cols, 120);
    }

    #[test]
    fn test_pty_session_config_default() {
        let config = PtySessionConfig::new("bash");
        assert_eq!(config.command, "bash");
        assert!(config.args.is_empty());
        assert!(config.working_dir.is_none());
        assert!(config.env.is_empty());
        assert_eq!(config.size.rows, 24);
        assert_eq!(config.size.cols, 80);
    }

    #[test]
    fn test_local_pty_session_debug() {
        // Skip on CI
        if std::env::var("CI").is_ok() {
            return;
        }
        let session = LocalPtySession::spawn_shell(crate::pty::DEFAULT_PTY_SIZE);
        assert!(session.is_ok());
        let session = session.unwrap();
        let debug = format!("{:?}", session);
        assert!(debug.contains("LocalPtySession"));
    }
}
```

**Step 2: Add trait object test in traits.rs**

```rust
#[cfg(test)]
mod tests {
    // ... existing PtyWrite tests ...

    #[test]
    fn test_pty_session_is_object_safe() {
        // Verify PtySession can be used as a trait object
        fn _assert_object_safe(_: &dyn super::PtySession) {}
    }
}
```

**Step 3: Run all tests**

Run: `cargo test 2>&1 | tail -20`
Expected: All tests pass

**Step 4: Commit**

```
test(pty): add tests for LocalPtySession and PtySession trait
```

---

### Task 7: Final verification

**Step 1: Run full CI checks**

Run: `cargo +nightly fmt --all && cargo clippy --all-targets --all-features -- -D warnings`
Expected: No warnings

**Step 2: Run full test suite**

Run: `cargo test 2>&1`
Expected: All tests pass

**Step 3: Verify the app runs**

Run: `ENGINE_TYPE=mock cargo run`
Expected: Terminal app launches, PTY works normally

**Step 4: Commit any fmt/clippy fixes**

```
style: fix formatting and clippy warnings
```
