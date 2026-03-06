//! PTY Manager for spawning and managing terminal sessions.
//!
//! Holds a `Box<dyn PtySession>` for runtime polymorphism — the concrete
//! session type (local, SSH, K8s, mock) is chosen at construction time.

use std::sync::Arc;

use anyhow::Result;
use portable_pty::PtySize;

use super::adapters::LocalPtySession;
use super::io::{PtyReader, PtyWriter};
use super::traits::PtySession;

/// Enum of supported PTY session providers. Used for selecting the session type at runtime.
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum PtyProvider {
    Local,
    #[cfg(feature = "pty-test_container")]
    TestContainer,
}

/// Manager for a PTY session. Wraps any [`PtySession`] implementation.
#[derive(Debug)]
pub struct PtyManager {
    session: Box<dyn PtySession>,
    current_size: PtySize,
    label: String,
}

impl PtyManager {
    /// Create a `PtyManager` from any [`PtySession`] implementation.
    pub async fn new(provider: PtyProvider, size: PtySize) -> Result<Self> {
        let (session, shell_name) = match provider {
            PtyProvider::Local => {
                let (session, shell_name) = LocalPtySession::spawn_shell(size)?;

                (Box::new(session) as Box<dyn PtySession>, shell_name)
            }
            #[cfg(feature = "pty-test_container")]
            PtyProvider::TestContainer => (
                Box::new(super::adapters::TestContainerPtySession::new().await?)
                    as Box<dyn PtySession>,
                "test-container-shell".to_string(),
            ),
        };

        Ok(Self {
            session,
            current_size: size,
            label: shell_name,
        })
    }

    /// Label describing the session (e.g. shell name for local sessions).
    pub fn label(&self) -> &str {
        &self.label
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

    /// Send SIGINT to the session (non-blocking).
    pub fn send_sigint(&self) -> Result<()> {
        self.session.send_sigint()
    }
}
