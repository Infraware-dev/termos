//! PTY Manager for spawning and managing terminal sessions.
//!
//! Holds a `Box<dyn PtySession>` for runtime polymorphism — the concrete
//! session type (local, SSH, K8s, mock) is chosen at construction time.

use std::sync::Arc;

use anyhow::Result;
use portable_pty::PtySize;

use super::DEFAULT_PTY_SIZE;
use super::adapters::LocalPtySession;
use super::io::{PtyReader, PtyWriter};
use super::traits::PtySession;

/// Manager for a PTY session. Wraps any [`PtySession`] implementation.
#[derive(Debug)]
pub struct PtyManager {
    session: Box<dyn PtySession>,
    current_size: PtySize,
    label: String,
}

#[expect(
    dead_code,
    reason = "Public API — methods used by TerminalSession and tests"
)]
impl PtyManager {
    /// Create a `PtyManager` from any [`PtySession`] implementation.
    pub fn new(session: Box<dyn PtySession>, label: impl Into<String>, size: PtySize) -> Self {
        Self {
            session,
            current_size: size,
            label: label.into(),
        }
    }

    /// Convenience constructor: spawn a local interactive shell with default size.
    pub fn local() -> Result<Self> {
        Self::local_with_size(DEFAULT_PTY_SIZE)
    }

    /// Convenience constructor: spawn a local interactive shell with custom size.
    pub fn local_with_size(size: PtySize) -> Result<Self> {
        let (session, shell_name) = LocalPtySession::spawn_shell(size)?;
        Ok(Self {
            session: Box::new(session),
            current_size: size,
            label: shell_name,
        })
    }

    /// Label describing the session (e.g. shell name for local sessions).
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Current terminal size.
    pub fn size(&self) -> PtySize {
        self.current_size
    }

    /// Take a reader for PTY output that sends to the provided channel.
    ///
    /// Note: Can only be called once (takes ownership).
    pub async fn take_reader(
        &mut self,
        sender: std::sync::mpsc::SyncSender<Vec<u8>>,
    ) -> Result<PtyReader> {
        self.session.take_reader(sender).await
    }

    /// Take a writer for PTY input.
    ///
    /// Note: Can only be called once (takes ownership).
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

    /// Send SIGINT to the session (non-blocking).
    pub fn send_sigint(&self) -> Result<()> {
        self.session.send_sigint()
    }
}
