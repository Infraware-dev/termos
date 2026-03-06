//! Trait definitions for PTY abstraction.
//!
//! These traits enable dependency injection and testing without a real PTY.
//! - `PtyWrite` — byte-level write trait implemented by `PtyWriter`.
//! - `PtySession` — async trait for interactive terminal sessions (local, SSH, K8s, mock).

use std::sync::Arc;
use std::sync::mpsc::SyncSender;

use anyhow::Result;
use async_trait::async_trait;
use portable_pty::PtySize;

use super::io::{PtyReader, PtyWriter};

/// Trait for writing to a PTY.
///
/// This abstraction allows for mock implementations in tests.
/// Implementations must be thread-safe (Send + Sync).
pub trait PtyWrite: Send + Sync {
    /// Write bytes to the PTY.
    ///
    /// # Arguments
    /// * `data` - The bytes to write to the PTY
    ///
    /// # Returns
    /// The number of bytes successfully written.
    ///
    /// # Errors
    /// Returns error if the PTY is closed or the write operation fails.
    fn write_bytes(&self, data: &[u8]) -> Result<usize>;
}

/// An interactive terminal session on a host (local, SSH, K8s, mock).
///
/// Implementations wrap the transport layer (local PTY, SSH channel, K8s exec)
/// and provide byte-level I/O via [`PtyReader`]/[`PtyWriter`].
#[async_trait]
pub trait PtySession: Send + Sync + std::fmt::Debug {
    /// Take a reader that streams output bytes to the given channel.
    async fn take_reader(&mut self, sender: SyncSender<Vec<u8>>) -> Result<PtyReader>;

    /// Take a writer handle for sending input bytes.
    async fn take_writer(&mut self) -> Result<Arc<PtyWriter>>;

    /// Resize the terminal dimensions.
    async fn resize(&self, size: PtySize) -> Result<()>;

    /// Send an interrupt signal (SIGINT / Ctrl+C equivalent).
    fn send_sigint(&self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    /// Mock PTY writer for testing.
    struct MockPtyWriter {
        bytes_written: AtomicUsize,
    }

    impl MockPtyWriter {
        fn new() -> Self {
            Self {
                bytes_written: AtomicUsize::new(0),
            }
        }

        fn bytes_written(&self) -> usize {
            self.bytes_written.load(Ordering::SeqCst)
        }
    }

    impl PtyWrite for MockPtyWriter {
        fn write_bytes(&self, data: &[u8]) -> Result<usize> {
            self.bytes_written.fetch_add(data.len(), Ordering::SeqCst);
            Ok(data.len())
        }
    }

    #[test]
    fn test_mock_pty_writer() {
        let writer = Arc::new(MockPtyWriter::new());
        assert_eq!(writer.bytes_written(), 0);

        writer.write_bytes(b"hello").unwrap();
        assert_eq!(writer.bytes_written(), 5);

        writer.write_bytes(b" world").unwrap();
        assert_eq!(writer.bytes_written(), 11);
    }

    #[test]
    fn test_pty_session_is_object_safe() {
        // Compile-time guard: PtySession must remain usable as Box<dyn PtySession>
        fn _assert_object_safe(_: &dyn PtySession) {}
    }
}
