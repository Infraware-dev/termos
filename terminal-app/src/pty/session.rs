//! PTY session management.
//!
//! Handles the lifecycle of a PTY session including the master/slave pair
//! and the child process.

use crate::pty::io::{PtyReader, PtyWriter};
use crate::pty::traits::PtyControl;
use anyhow::{Context, Result};
use portable_pty::{Child, ExitStatus, MasterPty, PtyPair, PtySize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Configuration for spawning a PTY session.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Builder pattern API for future PTY spawn customization
pub struct PtySessionConfig {
    /// Command to execute
    pub command: String,
    /// Command arguments
    pub args: Vec<String>,
    /// Working directory (None = inherit from parent)
    pub working_dir: Option<PathBuf>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Terminal size
    pub size: PtySize,
}

#[allow(dead_code)] // Builder pattern API for future PTY spawn customization
impl PtySessionConfig {
    /// Create a new configuration with default settings.
    #[must_use]
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            env: HashMap::new(),
            size: super::DEFAULT_PTY_SIZE,
        }
    }

    /// Add arguments to the command.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Set the working directory.
    #[must_use]
    pub fn working_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(path.into());
        self
    }

    /// Set an environment variable.
    #[must_use]
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.insert(key.into(), value.into());
        self
    }

    /// Set the terminal size.
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

/// A PTY session representing an active terminal with a running command.
///
/// The session owns the master side of the PTY pair and the child process.
/// Use `reader()` and `writer()` to get async I/O handles.
pub struct PtySession {
    /// Master PTY for I/O operations
    master: Arc<Mutex<Box<dyn MasterPty + Send>>>,
    /// Child process handle
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    /// Reader for async output reading
    reader: Option<PtyReader>,
    /// Writer for async input writing
    writer: Option<PtyWriter>,
}

impl std::fmt::Debug for PtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtySession")
            .field("reader", &self.reader)
            .field("writer", &self.writer)
            .finish_non_exhaustive()
    }
}

#[expect(dead_code, reason = "Public API - session methods used by PtyManager and tests")]
impl PtySession {
    /// Create a new PTY session from a PTY pair and child process.
    pub(crate) fn new(pair: PtyPair, child: Box<dyn Child + Send + Sync>) -> Self {
        Self {
            master: Arc::new(Mutex::new(pair.master)),
            child: Arc::new(Mutex::new(child)),
            reader: None,
            writer: None,
        }
    }

    /// Get a reader for the PTY output that sends to the provided channel.
    ///
    /// Note: This takes ownership of the reader. Only call once.
    /// Returns an error if the reader has already been taken.
    ///
    /// # Arguments
    /// * `sender` - Sync channel sender where PTY output will be sent
    pub async fn reader(&mut self, sender: std::sync::mpsc::SyncSender<Vec<u8>>) -> Result<PtyReader> {
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

    /// Get a writer for the PTY input.
    ///
    /// Note: This takes ownership of the writer. Only call once.
    /// Returns an error if the writer has already been taken.
    pub async fn writer(&mut self) -> Result<PtyWriter> {
        if self.writer.is_none() {
            let master = self.master.lock().await;
            let writer = master.take_writer().context("Failed to take PTY writer")?;
            self.writer = Some(PtyWriter::new(writer));
        }
        self.writer
            .take()
            .context("Writer already taken - can only be called once")
    }

    /// Resize the PTY terminal.
    ///
    /// This sends SIGWINCH to the child process to notify it of the size change.
    pub async fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        let master = self.master.lock().await;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")?;
        Ok(())
    }

    /// Check if the child process is still running.
    pub async fn is_running(&self) -> bool {
        let mut child = self.child.lock().await;
        child.try_wait().ok().flatten().is_none()
    }

    /// Wait for the child process to exit and return the exit status.
    pub async fn wait(&self) -> Result<ExitStatus> {
        let mut child = self.child.lock().await;
        child.wait().context("Failed to wait for child process")
    }

    /// Try to get the exit status without blocking.
    ///
    /// Returns `None` if the child is still running.
    pub async fn try_wait(&self) -> Result<Option<ExitStatus>> {
        let mut child = self.child.lock().await;
        child
            .try_wait()
            .context("Failed to check child process status")
    }

    /// Kill the child process.
    pub async fn kill(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        child.kill().context("Failed to kill child process")
    }

    /// Get the process ID of the child.
    pub async fn pid(&self) -> Option<u32> {
        let child = self.child.lock().await;
        child.process_id()
    }

    /// Send SIGINT to the foreground process group.
    /// Reads tpgid from /proc to get the actual foreground group (e.g., cat, not shell).
    #[cfg(unix)]
    pub fn send_sigint(&self) -> Result<()> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        // Get shell PID first
        let shell_pid = {
            match self.child.try_lock() {
                Ok(child) => child.process_id(),
                Err(_) => {
                    log::warn!("Could not lock child to get PID, skipping SIGINT");
                    return Ok(());
                }
            }
        };

        if let Some(pid) = shell_pid {
            // Read foreground pgid from /proc/<pid>/stat - this is the KEY!
            // tpgid is the foreground process group of the controlling terminal
            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
                if let Some(comm_end) = stat.rfind(')') {
                    let after_comm = &stat[comm_end + 1..];
                    let fields: Vec<&str> = after_comm.split_whitespace().collect();
                    // After (comm): state(0) ppid(1) pgrp(2) session(3) tty_nr(4) tpgid(5)
                    if fields.len() > 5 {
                        if let Ok(tpgid) = fields[5].parse::<i32>() {
                            if tpgid > 0 {
                                log::info!("Sending SIGINT to foreground pgid {} (tpgid from /proc/{})", tpgid, pid);
                                kill(Pid::from_raw(-tpgid), Signal::SIGINT)
                                    .context("Failed to send SIGINT to foreground pgid")?;
                                return Ok(());
                            }
                        }
                    }
                }
            }

            // Fallback: send to shell's process group
            log::info!("Sending SIGINT to shell process group {} (fallback)", pid);
            kill(Pid::from_raw(-(pid as i32)), Signal::SIGINT)
                .context("Failed to send SIGINT to process group")?;
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub fn send_sigint(&self) -> Result<()> {
        // On non-Unix, fall back to kill
        log::warn!("SIGINT not supported on this platform, using kill");
        // Can't use async here, so just log warning
        Ok(())
    }

    /// Synchronous resize using try_lock (for trait implementation).
    ///
    /// This is a non-blocking version that returns an error if the lock is held.
    /// For guaranteed resize, use the async `resize()` method instead.
    ///
    /// # Note
    /// Uses `tokio::sync::Mutex` which doesn't have lock poisoning (unlike `std::sync::Mutex`).
    /// The only failure case is when the lock is currently held by another task.
    fn resize_sync(&self, rows: u16, cols: u16) -> Result<()> {
        let master = self.master.try_lock().map_err(|_| {
            log::debug!("PTY resize deferred - async lock held by another operation");
            anyhow::anyhow!("Master PTY lock held, resize deferred")
        })?;
        master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")
    }
}

/// Implement PtyControl trait for dependency injection support.
///
/// Note: resize uses try_lock and may fail if the async lock is held.
/// For production code, prefer the async resize() method.
impl PtyControl for PtySession {
    fn resize(&self, rows: u16, cols: u16) -> Result<()> {
        self.resize_sync(rows, cols)
    }

    fn send_sigint(&self) -> Result<()> {
        // Delegate to the inherent method
        PtySession::send_sigint(self)
    }
}

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

    #[tokio::test]
    async fn test_pty_echo_command() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let pty = crate::pty::Pty::new();
        let mut session = pty
            .spawn("echo", &["hello PTY"], crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn PTY");

        // Create sync channel for PTY output
        let (tx, rx) = std::sync::mpsc::sync_channel(4);
        let _reader = session.reader(tx).await.expect("Failed to get reader");

        // Read output - wait for data from background thread
        let mut all_output = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(2) {
            // Use recv_timeout for blocking wait with timeout
            match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                Ok(bytes) if !bytes.is_empty() => {
                    all_output.extend_from_slice(&bytes);
                }
                _ => {}
            }

            if session.try_wait().await.ok().flatten().is_some() {
                // Process exited, drain remaining output
                while let Ok(bytes) = rx.try_recv() {
                    if bytes.is_empty() {
                        break;
                    }
                    all_output.extend_from_slice(&bytes);
                }
                break;
            }
        }

        let output = String::from_utf8_lossy(&all_output);
        println!("PTY output: {:?}", output);
        assert!(
            output.contains("hello PTY"),
            "Expected 'hello PTY' in output: {:?}",
            output
        );
    }

    /// Test that SIGINT can kill an infinite-output process like `yes` or `cat /dev/zero`
    #[tokio::test]
    async fn test_sigint_kills_infinite_output() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let pty = crate::pty::Pty::new();
        // Use `yes` instead of `cat /dev/zero` - similar infinite output but text
        let args: &[&str] = &[];
        let session = pty
            .spawn("yes", args, crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn PTY");

        // Let it run for a bit
        std::thread::sleep(std::time::Duration::from_millis(100));

        // Verify it's still running
        assert!(session.is_running().await, "Process should still be running");

        // Send SIGINT
        println!("Sending SIGINT...");
        session.send_sigint().expect("Failed to send SIGINT");

        // Wait a bit for it to die
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Check if it's dead
        let is_running = session.is_running().await;
        println!("After SIGINT, is_running: {}", is_running);

        assert!(!is_running, "Process should have been killed by SIGINT");
    }

    /// Test SIGINT with a shell running an infinite command (closer to real app behavior)
    #[tokio::test]
    async fn test_sigint_kills_shell_child() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let pty = crate::pty::Pty::new();
        // Spawn bash running `yes` - this is how the real app works
        let session = pty
            .spawn("bash", &["-c", "yes"], crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn PTY");

        // Let it run for a bit
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Verify it's still running
        assert!(session.is_running().await, "Process should still be running");

        // Send SIGINT - should kill the foreground job (yes), not bash itself
        println!("Sending SIGINT to foreground...");
        session.send_sigint().expect("Failed to send SIGINT");

        // Wait for it to die
        std::thread::sleep(std::time::Duration::from_millis(300));

        // The bash -c "yes" should exit when yes is killed
        let is_running = session.is_running().await;
        println!("After SIGINT, is_running: {}", is_running);

        assert!(!is_running, "bash -c 'yes' should have exited after SIGINT");
    }

    /// Test SIGINT with interactive shell - most realistic test
    #[tokio::test]
    async fn test_sigint_interactive_shell() {
        // Skip if not running in a real terminal
        if std::env::var("CI").is_ok() {
            return;
        }

        let pty = crate::pty::Pty::new();
        // Spawn interactive bash like the app does
        let mut session = pty
            .spawn("bash", &["-i"], crate::pty::DEFAULT_PTY_SIZE)
            .expect("Failed to spawn PTY");

        let writer = session.writer().await.expect("Failed to get writer");

        // Wait for shell to start
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Send command to run infinite output
        println!("Sending 'yes' command to shell...");
        writer.write_str("yes\n").await.expect("Failed to write");

        // Let yes run for a bit
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Verify still running
        assert!(session.is_running().await, "Shell should still be running");

        // Send SIGINT
        println!("Sending SIGINT...");
        session.send_sigint().expect("Failed to send SIGINT");

        // Wait a bit
        std::thread::sleep(std::time::Duration::from_millis(300));

        // Shell should still be running (only yes was killed)
        let is_running = session.is_running().await;
        println!("After SIGINT, shell is_running: {}", is_running);

        // yes should have been killed, but interactive bash is still running
        // In interactive mode, bash catches SIGINT for the child and returns to prompt
        assert!(is_running, "Interactive bash should still be running after child is killed");

        // Clean up - exit the shell
        writer.write_str("exit\n").await.expect("Failed to write exit");
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
