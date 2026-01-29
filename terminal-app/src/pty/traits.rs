//! Trait definitions for PTY abstraction.
//!
//! These traits enable dependency injection and testing without a real PTY.
//! Implemented by `PtyWriter` (PtyWrite).

use anyhow::Result;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

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
}
