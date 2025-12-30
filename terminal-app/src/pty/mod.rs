//! PTY (Pseudo-Terminal) module for interactive command support.
//!
//! This module provides PTY functionality for running a persistent shell (bash/zsh)
//! that handles all command execution.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────┐     ┌─────────────┐     ┌─────────────┐
//! │   egui      │────▶│  PtyManager │────▶│   PTY       │
//! │   (UI)      │◀────│  (async)    │◀────│  (bash/zsh) │
//! └─────────────┘     └─────────────┘     └─────────────┘
//!      Input              I/O              Shell
//!      Render          Multiplex          Execution
//! ```

mod io;
mod manager;
mod session;

pub use io::PtyWriter;
pub use manager::PtyManager;
pub use session::PtySession;

use anyhow::Result;
use portable_pty::{native_pty_system, CommandBuilder, PtySize, PtySystem};
use std::ffi::OsStr;

/// Default PTY size matching typical terminal dimensions.
pub const DEFAULT_PTY_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
};

/// PTY system wrapper providing a clean API for spawning PTY sessions.
pub struct Pty {
    system: Box<dyn PtySystem + Send>,
}

impl std::fmt::Debug for Pty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pty").finish_non_exhaustive()
    }
}

impl Default for Pty {
    fn default() -> Self {
        Self::new()
    }
}

impl Pty {
    /// Create a new PTY system using the native platform implementation.
    #[must_use]
    pub fn new() -> Self {
        Self {
            system: native_pty_system(),
        }
    }

    /// Spawn a new PTY session for the given command.
    pub fn spawn<S: AsRef<OsStr>>(
        &self,
        cmd: &str,
        args: &[S],
        size: PtySize,
    ) -> Result<PtySession> {
        let pair = self.system.openpty(size)?;

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

        let child = pair.slave.spawn_command(builder)?;

        Ok(PtySession::new(pair, child))
    }
}
