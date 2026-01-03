//! PTY Manager for spawning and managing the shell session.
//!
//! This module provides a high-level API for managing a persistent shell (bash/zsh)
//! that runs throughout the application's lifetime.

use super::io::{PtyReader, PtyWriter};
use super::{Pty, PtySession, DEFAULT_PTY_SIZE};
use anyhow::{Context, Result};
use portable_pty::PtySize;
use std::sync::Arc;

/// Shell preference - tries zsh first, then bash.
const SHELL_PRIORITY: &[&str] = &["zsh", "bash", "sh"];

/// Manager for the persistent PTY shell session.
#[derive(Debug)]
pub struct PtyManager {
    /// The PTY session running the shell
    session: PtySession,
    /// Current terminal size
    current_size: PtySize,
    /// The shell being used
    shell: String,
}

#[allow(dead_code)]
impl PtyManager {
    /// Create a new PTY manager by spawning a shell.
    ///
    /// Tries zsh first, then falls back to bash, then sh.
    pub async fn new() -> Result<Self> {
        Self::with_size(DEFAULT_PTY_SIZE).await
    }

    /// Create a new PTY manager with a specific terminal size.
    pub async fn with_size(size: PtySize) -> Result<Self> {
        let shell = Self::detect_shell()?;
        let pty = Pty::new();

        log::info!("Spawning PTY with shell: {}", shell);

        // Spawn the shell with -i for interactive mode
        let session = pty
            .spawn(&shell, &["-i"], size)
            .context("Failed to spawn shell")?;

        Ok(Self {
            session,
            current_size: size,
            shell,
        })
    }

    /// Detect available shell (prefers zsh > bash > sh).
    fn detect_shell() -> Result<String> {
        for shell in SHELL_PRIORITY {
            if which::which(shell).is_ok() {
                return Ok((*shell).to_string());
            }
        }
        anyhow::bail!("No supported shell found (tried: {:?})", SHELL_PRIORITY)
    }

    /// Get the shell being used.
    pub fn shell(&self) -> &str {
        &self.shell
    }

    /// Get the current terminal size.
    pub fn size(&self) -> PtySize {
        self.current_size
    }

    /// Get a reader for PTY output that sends to the provided channel.
    ///
    /// Note: Can only be called once (takes ownership).
    ///
    /// # Arguments
    /// * `sender` - Sync channel sender where PTY output will be sent
    pub async fn take_reader(
        &mut self,
        sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    ) -> Result<PtyReader> {
        self.session.reader(sender).await
    }

    /// Get a writer for PTY input.
    ///
    /// Note: Can only be called once (takes ownership).
    pub async fn take_writer(&mut self) -> Result<Arc<PtyWriter>> {
        Ok(Arc::new(self.session.writer().await?))
    }

    /// Resize the terminal.
    pub async fn resize(&mut self, rows: u16, cols: u16) -> Result<()> {
        if self.current_size.rows != rows || self.current_size.cols != cols {
            self.session.resize(rows, cols).await?;
            self.current_size = PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            };
            log::debug!("PTY resized to {}x{}", cols, rows);
        }
        Ok(())
    }

    /// Check if the shell is still running.
    pub async fn is_running(&self) -> bool {
        self.session.is_running().await
    }

    /// Kill the shell process.
    pub async fn kill(&self) -> Result<()> {
        self.session.kill().await
    }

    /// Send SIGINT to the shell's process group (non-blocking).
    /// This interrupts the foreground process without waiting for PTY buffers.
    pub fn send_sigint(&self) -> Result<()> {
        self.session.send_sigint()
    }
}
