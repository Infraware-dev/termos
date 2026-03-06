//! Adapters for a test container running on docker; interaction with the container is achieved via [bollard](https://docs.rs/bollard/).
//!
//! This adapter is useful for testing purposes.
//! The idea is to use it in a sandboxed Debian container to allow the agent to execute commands without affecting the host system.

mod container;

use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;

use anyhow::{Context, Result};
use async_trait::async_trait;
use bollard::container::LogOutput;
use futures::{Stream, StreamExt as _};
use portable_pty::PtySize;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

use self::container::Container;
use crate::pty::io::{PtyReader, PtyWriter};
use crate::pty::traits::PtySession;

/// Boxed async output stream from the container's TTY.
type OutputStream = Pin<Box<dyn Stream<Item = Result<LogOutput, bollard::errors::Error>> + Send>>;

/// PTY session adapter that runs a Debian container using bollard.
///
/// Bridges bollard's async I/O streams to the sync [`PtyReader`]/[`PtyWriter`]
/// types expected by [`PtySession`] using Unix socket pairs and tokio tasks.
///
/// On drop, the container is stopped and removed asynchronously.
pub struct TestContainerPtySession {
    /// The container instance (used for resize, inspect, stop).
    /// Wrapped in `Option` so `Drop` can take ownership for async cleanup.
    container: Option<Container>,
    /// Tokio runtime handle captured at construction time.
    /// Stored explicitly because `Drop` may run outside a tokio context
    /// (e.g., when eframe drops the app struct).
    runtime_handle: tokio::runtime::Handle,
    /// Async output stream, consumed once by [`PtySession::take_reader`].
    /// Wrapped in `Mutex` to satisfy the `Sync` bound on [`PtySession`].
    output: std::sync::Mutex<Option<OutputStream>>,
    /// Sync write end of the writer bridge, consumed once by [`PtySession::take_writer`].
    writer_handle: Option<std::os::unix::net::UnixStream>,
    /// Cloned write handle for sending SIGINT (Ctrl+C) synchronously.
    sigint_handle: Arc<std::sync::Mutex<std::os::unix::net::UnixStream>>,
}

impl Drop for TestContainerPtySession {
    fn drop(&mut self) {
        let Some(container) = self.container.take() else {
            return;
        };
        let handle = self.runtime_handle.clone();
        // Run cleanup on a dedicated thread so `block_on` doesn't panic
        // if we happen to be inside an async context. The join ensures
        // cleanup finishes before the runtime (which drops after us) is
        // torn down — without it, hyper's IO driver may be gone by the
        // time `remove_container` fires.
        let join_handle = std::thread::spawn(move || {
            if let Err(e) = handle.block_on(container.stop()) {
                tracing::error!("Failed to stop container on drop: {e}");
            }
        });
        if let Err(e) = join_handle.join() {
            tracing::error!("Container cleanup thread panicked: {e:?}");
        }
    }
}

impl std::fmt::Debug for TestContainerPtySession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let has_output = self.output.lock().map(|o| o.is_some()).unwrap_or(false);
        f.debug_struct("TestContainerPtySession")
            .field("has_container", &self.container.is_some())
            .field("has_output", &has_output)
            .field("has_writer", &self.writer_handle.is_some())
            .finish_non_exhaustive()
    }
}

impl TestContainerPtySession {
    /// Creates a new PTY session by starting a Debian container.
    ///
    /// Sets up the container, attaches to its TTY, and creates the async→sync
    /// bridge for the writer side. The reader bridge is deferred to
    /// [`PtySession::take_reader`] because it needs the caller's channel.
    pub async fn new() -> Result<Self> {
        let (container, handles) = Container::setup().await?;

        // Create a Unix socket pair to bridge sync PtyWriter writes to the
        // container's async stdin. The sync end (`unix_write`) is handed to
        // PtyWriter; a background tokio task drains the async end into bollard.
        let (unix_read, unix_write) = std::os::unix::net::UnixStream::pair()
            .context("Failed to create Unix socket pair for writer bridge")?;

        // Keep a clone for synchronous SIGINT delivery (writes 0x03).
        let sigint_writer = unix_write
            .try_clone()
            .context("Failed to clone Unix socket for sigint")?;

        spawn_writer_bridge(unix_read, handles.input)?;

        // Capture the runtime handle now (we're guaranteed to be inside a
        // tokio context during construction) so `Drop` can use it later.
        let runtime_handle = tokio::runtime::Handle::current();

        Ok(Self {
            container: Some(container),
            runtime_handle,
            output: std::sync::Mutex::new(Some(handles.output)),
            writer_handle: Some(unix_write),
            sigint_handle: Arc::new(std::sync::Mutex::new(sigint_writer)),
        })
    }
}

/// Spawns a tokio task that forwards bytes from the sync Unix socket to the
/// container's async stdin.
///
/// ```text
/// PtyWriter → unix_write ──→ unix_read (this task) ──→ bollard input
/// ```
fn spawn_writer_bridge(
    unix_read: std::os::unix::net::UnixStream,
    mut input: Pin<Box<dyn tokio::io::AsyncWrite + Send>>,
) -> Result<()> {
    unix_read
        .set_nonblocking(true)
        .context("Failed to set Unix socket to non-blocking")?;
    let mut async_read = tokio::net::UnixStream::from_std(unix_read)
        .context("Failed to convert Unix socket to tokio")?;

    tokio::spawn(async move {
        let mut buf = vec![0u8; 4096];
        loop {
            match async_read.read(&mut buf).await {
                Ok(0) => break,
                Ok(n) => {
                    if input.write_all(&buf[..n]).await.is_err() {
                        break;
                    }
                    if input.flush().await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Writer bridge error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("Docker writer bridge task exiting");
    });

    Ok(())
}

/// Spawns a tokio task that drains the container's async output stream into
/// a sync channel, stopping when the stop flag is set or the stream ends.
///
/// ```text
/// bollard output ──→ this task ──→ SyncSender ──→ consumer
/// ```
fn spawn_reader_bridge(
    mut output: OutputStream,
    sender: SyncSender<Vec<u8>>,
    stop_flag: Arc<AtomicBool>,
) {
    tokio::spawn(async move {
        while let Some(chunk) = output.next().await {
            if stop_flag.load(Ordering::Acquire) {
                break;
            }
            match chunk {
                Ok(log_output) => {
                    let bytes = match log_output {
                        LogOutput::Console { message }
                        | LogOutput::StdOut { message }
                        | LogOutput::StdErr { message } => message,
                        _ => continue,
                    };
                    if sender.send(bytes.to_vec()).is_err() {
                        break;
                    }
                }
                Err(e) => {
                    tracing::debug!("Docker output stream error: {e}");
                    break;
                }
            }
        }
        tracing::debug!("Docker output reader task exiting");
    });
}

#[async_trait]
impl PtySession for TestContainerPtySession {
    async fn take_reader(&mut self, sender: SyncSender<Vec<u8>>) -> Result<PtyReader> {
        let output = self
            .output
            .lock()
            .expect("output lock poisoned")
            .take()
            .context("Output stream already taken")?;

        let stop_flag = Arc::new(AtomicBool::new(false));
        spawn_reader_bridge(output, sender, stop_flag.clone());

        Ok(PtyReader::with_stop_flag(stop_flag))
    }

    async fn take_writer(&mut self) -> Result<Arc<PtyWriter>> {
        let unix_write = self
            .writer_handle
            .take()
            .context("Writer already taken - can only be called once")?;

        Ok(Arc::new(PtyWriter::new(Box::new(unix_write))))
    }

    async fn resize(&self, size: PtySize) -> Result<()> {
        self.container
            .as_ref()
            .context("Container already stopped")?
            .resize(size.cols, size.rows)
            .await
            .context("Failed to resize container TTY")
    }

    fn send_sigint(&self) -> Result<()> {
        use std::io::Write as _;
        let mut writer = self
            .sigint_handle
            .lock()
            .expect("sigint handle lock poisoned");
        writer
            .write_all(&[0x03])
            .context("Failed to send Ctrl+C to container")
    }
}
