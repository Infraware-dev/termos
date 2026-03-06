//! Local PTY session adapter.
//!
//! Wraps the native platform PTY (`portable_pty`) to provide interactive
//! terminal sessions for local shell and command execution.

use std::ffi::OsStr;
use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use anyhow::{Context, Result};
use async_trait::async_trait;
use portable_pty::{CommandBuilder, MasterPty, PtyPair, PtySize, PtySystem, native_pty_system};
use tokio::sync::Mutex;

use crate::pty::io::{PtyReader, PtyWriter};
use crate::pty::traits::PtySession;

/// Shell preference — tries zsh first, then bash, then sh.
const SHELL_PRIORITY: &[&str] = &["zsh", "bash", "sh"];

/// A local PTY session backed by the native platform PTY.
///
/// Owns the master side of the PTY pair and the child process.
/// Use the [`PtySession`] trait methods for I/O and lifecycle.
pub struct LocalPtySession {
    /// Master PTY for I/O operations
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Reader (taken once via `take_reader`)
    reader: Option<PtyReader>,
    /// Writer (taken once via `take_writer`)
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
    /// Create a `LocalPtySession` from a PTY pair.
    fn new(pair: PtyPair) -> Self {
        Self {
            master: Arc::new(Mutex::new(pair.master)),
            reader: None,
            writer: None,
        }
    }

    /// Spawn an interactive shell session.
    ///
    /// Detects the best available shell (zsh > bash > sh) and spawns it
    /// in interactive mode.
    ///
    /// # Returns
    /// `(session, shell_name)` where `shell_name` is e.g. `"zsh"`, `"bash"`.
    pub fn spawn_shell(size: PtySize) -> Result<(Self, String)> {
        let shell = detect_shell()?;
        tracing::info!("Spawning local PTY with shell: {shell}");
        let session = spawn_command(&shell, &["-i"], size)?;
        Ok((session, shell))
    }
}

#[async_trait]
impl PtySession for LocalPtySession {
    async fn take_reader(&mut self, sender: SyncSender<Vec<u8>>) -> Result<PtyReader> {
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
        master.resize(size).context("Failed to resize PTY")?;
        Ok(())
    }

    fn send_sigint(&self) -> Result<()> {
        #[cfg(unix)]
        {
            match self.master.try_lock() {
                Ok(master) => {
                    if let Some(raw_fd) = master.as_raw_fd() {
                        // SAFETY: raw_fd is valid while master lock is held.
                        // We write a single byte (Ctrl+C) to trigger SIGINT.
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
        {
            tracing::warn!("SIGINT not supported on this platform, using kill");
            Ok(())
        }
    }
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Detect the best available shell.
fn detect_shell() -> Result<String> {
    for shell in SHELL_PRIORITY {
        if which::which(shell).is_ok() {
            return Ok((*shell).to_string());
        }
    }
    anyhow::bail!("No supported shell found (tried: {SHELL_PRIORITY:?})")
}

/// Spawn a command in a new native PTY.
fn spawn_command<S: AsRef<OsStr>>(cmd: &str, args: &[S], size: PtySize) -> Result<LocalPtySession> {
    let system: Box<dyn PtySystem + Send> = native_pty_system();
    let pair = system.openpty(size)?;

    let mut builder = CommandBuilder::new(cmd);
    for arg in args {
        builder.arg(arg);
    }

    // Set working directory to current directory
    if let Ok(cwd) = std::env::current_dir() {
        builder.cwd(cwd);
    }

    // Inherit environment from parent process
    for (key, value) in std::env::vars() {
        builder.env(key, value);
    }

    // Set TERM for proper terminal emulation
    builder.env("TERM", "xterm-256color");

    // Set terminal size environment variables
    builder.env("COLUMNS", size.cols.to_string());
    builder.env("LINES", size.rows.to_string());

    // NOTE: Do NOT set PS1/PROMPT here — it interferes with sub-shells (sudo su).
    // The custom prompt is set by initialize_shell() after startup instead.

    let _child = pair.slave.spawn_command(builder)?;
    Ok(LocalPtySession::new(pair))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_pty_spawn_shell_and_read() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let (mut session, shell) = LocalPtySession::spawn_shell(crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn shell");

        assert!(!shell.is_empty());

        // Create sync channel for PTY output
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let _reader = session.take_reader(tx).await.expect("Failed to get reader");

        let writer = session.take_writer().await.expect("Failed to get writer");

        // Send a command and read output
        writer
            .write_str("echo hello PTY\n")
            .await
            .expect("Failed to write");

        // Wait for output
        let mut all_output = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(2) {
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(bytes) if !bytes.is_empty() => {
                    all_output.extend_from_slice(&bytes);
                    let output = String::from_utf8_lossy(&all_output);
                    if output.contains("hello PTY") {
                        break;
                    }
                }
                _ => {}
            }
        }

        let output = String::from_utf8_lossy(&all_output);
        assert!(
            output.contains("hello PTY"),
            "Expected 'hello PTY' in output: {output:?}",
        );

        // Clean up
        writer
            .write_str("exit\n")
            .await
            .expect("Failed to write exit");
    }

    /// Test SIGINT with interactive shell — the realistic scenario used by the app
    #[tokio::test]
    async fn test_sigint_interactive_shell() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let (mut session, _) = LocalPtySession::spawn_shell(crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn shell");

        let writer = session.take_writer().await.expect("Failed to get writer");

        // Wait for shell to start
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Send command to run infinite output
        writer.write_str("yes\n").await.expect("Failed to write");

        // Let yes run for a bit
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Send SIGINT — should not panic or fail
        session.send_sigint().expect("Failed to send SIGINT");

        // Wait a bit then clean up
        std::thread::sleep(std::time::Duration::from_millis(300));
        writer
            .write_str("exit\n")
            .await
            .expect("Failed to write exit");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    #[test]
    fn test_local_pty_session_debug() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let (session, shell) = LocalPtySession::spawn_shell(crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn shell");

        assert!(!shell.is_empty());
        let debug = format!("{session:?}");
        assert!(debug.contains("LocalPtySession"));
    }

    #[test]
    fn test_detect_shell() {
        // At least one of zsh/bash/sh should be available
        let shell = detect_shell().expect("Should find a shell");
        assert!(
            SHELL_PRIORITY.contains(&shell.as_str()),
            "Unexpected shell: {shell}"
        );
    }
}
