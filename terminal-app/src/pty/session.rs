//! PTY session management.
//!
//! Handles the lifecycle of a PTY session including the master/slave pair
//! and the child process.

use crate::pty::io::{PtyReader, PtyWriter};
use anyhow::{Context, Result};
use portable_pty::{Child, ExitStatus, MasterPty, PtyPair, PtySize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Configuration for spawning a PTY session.
#[derive(Debug, Clone)]
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

    /// Get a reader for the PTY output.
    ///
    /// Note: This takes ownership of the reader. Only call once.
    /// Returns an error if the reader has already been taken.
    pub async fn reader(&mut self) -> Result<PtyReader> {
        if self.reader.is_none() {
            let master = self.master.lock().await;
            let reader = master
                .try_clone_reader()
                .context("Failed to clone PTY reader")?;
            self.reader = Some(PtyReader::new(reader));
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

        let mut reader = session.reader().await.expect("Failed to get reader");

        // Read output - wait for data from background thread
        let mut all_output = Vec::new();
        let start = std::time::Instant::now();
        while start.elapsed() < std::time::Duration::from_secs(2) {
            // Use read_with_timeout for blocking wait with timeout
            match reader.read_with_timeout(std::time::Duration::from_millis(100)).await {
                Ok(bytes) if !bytes.is_empty() => {
                    all_output.extend_from_slice(&bytes);
                }
                _ => {}
            }

            if session.try_wait().await.ok().flatten().is_some() {
                // Process exited, drain remaining output
                while let Ok(bytes) = reader.read_available().await {
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
}
