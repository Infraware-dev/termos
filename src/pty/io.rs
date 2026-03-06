//! PTY I/O operations.
//!
//! Provides wrappers around the synchronous PTY reader/writer,
//! using a dedicated reader thread with sync channel for non-blocking operation.

use std::fmt;
use std::io::{Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;

use anyhow::{Context, Result};

use super::traits::PtyWrite;

/// Reader for PTY output.
///
/// Uses a dedicated background thread for reading to avoid blocking.
/// Data is sent via a sync channel for direct consumption without async overhead.
pub struct PtyReader {
    /// Flag to signal the reader thread to stop
    stop_flag: Arc<AtomicBool>,
}

impl fmt::Debug for PtyReader {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtyReader").finish_non_exhaustive()
    }
}

impl Drop for PtyReader {
    fn drop(&mut self) {
        // Signal the reader thread to stop
        // Release ordering: ensures all writes before this store are visible to the reader
        self.stop_flag.store(true, Ordering::Release);
    }
}

#[allow(dead_code)]
impl PtyReader {
    /// Creates a `PtyReader` with an externally managed stop flag.
    ///
    /// Use this when reading is handled by an external task (e.g., a tokio
    /// task draining an async stream) rather than by the internal blocking
    /// reader thread spawned by [`PtyReader::new`].
    pub fn with_stop_flag(stop_flag: Arc<AtomicBool>) -> Self {
        Self { stop_flag }
    }

    /// Create a new PTY reader that sends data to the provided channel.
    ///
    /// Spawns a background thread that reads from the PTY and sends data
    /// directly to the sync channel. This avoids double-channel overhead.
    ///
    /// # Arguments
    /// * `reader` - The raw PTY reader (file descriptor)
    /// * `sender` - Sync channel sender for PTY output data
    pub fn new(mut reader: Box<dyn Read + Send>, sender: SyncSender<Vec<u8>>) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Spawn a dedicated thread for reading from PTY
        std::thread::spawn(move || {
            let mut buf = vec![0u8; crate::config::pty::READER_BUFFER_SIZE];

            loop {
                // Check if we should stop
                // Acquire ordering: sees all writes before the Release store
                if stop_flag_clone.load(Ordering::Acquire) {
                    break;
                }

                // Read from PTY (this blocks)
                match reader.read(&mut buf) {
                    Ok(0) => {
                        // EOF
                        break;
                    }
                    Ok(n) => {
                        let data = buf[..n].to_vec();
                        // PERFORMANCE: Send directly to sync channel (no async overhead)
                        // This will BLOCK if channel is full - creating backpressure
                        if sender.send(data).is_err() {
                            // Channel closed, stop reading
                            break;
                        }
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::Interrupted {
                            tracing::debug!("PTY reader error: {}", e);
                            break;
                        }
                        // Interrupted, continue reading
                    }
                }
            }

            tracing::debug!("PTY reader thread exiting");
        });

        Self { stop_flag }
    }

    /// Check if the reader thread is still running.
    pub fn is_alive(&self) -> bool {
        // Acquire ordering: consistent with the reader thread's load
        !self.stop_flag.load(Ordering::Acquire)
    }
}

/// Async writer for PTY input.
///
/// Wraps the synchronous PTY writer. Uses std::sync::Mutex for compatibility
/// with both async and sync contexts (crossterm events run in block_in_place).
pub struct PtyWriter {
    inner: Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
}

impl fmt::Debug for PtyWriter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PtyWriter").finish_non_exhaustive()
    }
}

#[allow(dead_code)]
impl PtyWriter {
    /// Create a new PTY writer.
    pub fn new(writer: Box<dyn Write + Send>) -> Self {
        Self {
            inner: Arc::new(std::sync::Mutex::new(writer)),
        }
    }

    /// Write data to the PTY.
    ///
    /// This writes the entire buffer to the PTY.
    /// Returns the number of bytes written.
    pub async fn write(&self, data: &[u8]) -> Result<usize> {
        self.write_sync(data)
    }

    /// Synchronous write - works in both async and sync contexts.
    ///
    /// This is a blocking write that can be safely called from inside
    /// `tokio::task::block_in_place` or other synchronous contexts.
    ///
    /// # Panics
    /// Panics if the writer lock is poisoned (indicates unrecoverable corruption).
    pub fn write_sync(&self, data: &[u8]) -> Result<usize> {
        let mut writer = self
            .inner
            .lock()
            .expect("PTY writer lock poisoned - unrecoverable state corruption");
        writer.write_all(data).context("Failed to write to PTY")?;
        writer.flush().context("Failed to flush PTY")?;
        Ok(data.len())
    }

    /// Write a string to the PTY.
    ///
    /// Convenience method for writing UTF-8 strings.
    pub async fn write_str(&self, s: &str) -> Result<usize> {
        self.write(s.as_bytes()).await
    }

    /// Send a single byte (useful for control characters).
    ///
    /// # Common control characters:
    /// - `0x03` (Ctrl+C) - SIGINT
    /// - `0x04` (Ctrl+D) - EOF
    /// - `0x1A` (Ctrl+Z) - SIGTSTP (suspend)
    /// - `0x1B` - Escape
    pub async fn send_byte(&self, byte: u8) -> Result<()> {
        self.write(&[byte]).await?;
        Ok(())
    }

    /// Send Ctrl+C (SIGINT) to the PTY.
    pub async fn send_interrupt(&self) -> Result<()> {
        self.send_byte(0x03).await
    }

    /// Send Ctrl+D (EOF) to the PTY.
    pub async fn send_eof(&self) -> Result<()> {
        self.send_byte(0x04).await
    }

    /// Send Ctrl+Z (SIGTSTP - suspend) to the PTY.
    pub async fn send_suspend(&self) -> Result<()> {
        self.send_byte(0x1A).await
    }
}

/// Implement PtyWrite trait for dependency injection support.
impl PtyWrite for PtyWriter {
    fn write_bytes(&self, data: &[u8]) -> Result<usize> {
        self.write_sync(data)
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn test_pty_reader_creation() {
        let data = b"Hello, PTY!";
        let cursor = Cursor::new(data.to_vec());
        let (tx, rx) = mpsc::sync_channel(4);
        let _reader = PtyReader::new(Box::new(cursor), tx);

        // Wait a bit for the reader thread to read the data
        std::thread::sleep(std::time::Duration::from_millis(50));

        let result = rx.try_recv().unwrap();
        assert_eq!(&result, data);
    }

    #[tokio::test]
    async fn test_pty_writer_creation() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));

        let n = writer.write_str("test").await.unwrap();
        assert_eq!(n, 4);
    }

    #[test]
    fn test_pty_writer_trait_object() {
        // Test that PtyWriter works via trait object (for DI)
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));

        // Use via trait object to ensure dynamic dispatch works
        let trait_obj: &dyn PtyWrite = &writer;
        let result = trait_obj.write_bytes(b"hello");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    #[test]
    fn test_pty_reader_is_alive() {
        let (tx, _rx) = mpsc::sync_channel(4);
        // Use a reader that blocks indefinitely (pipe-like)
        let (read_end, _write_end) = std::os::unix::net::UnixStream::pair().unwrap();
        let reader = PtyReader::new(Box::new(read_end), tx);
        assert!(reader.is_alive());
    }

    #[test]
    fn test_pty_reader_drop_stops() {
        let (tx, _rx) = mpsc::sync_channel(4);
        let (read_end, _write_end) = std::os::unix::net::UnixStream::pair().unwrap();
        let reader = PtyReader::new(Box::new(read_end), tx);
        let stop_flag = reader.stop_flag.clone();

        assert!(!stop_flag.load(std::sync::atomic::Ordering::Acquire));
        drop(reader);
        assert!(stop_flag.load(std::sync::atomic::Ordering::Acquire));
    }

    #[test]
    fn test_pty_reader_debug() {
        let (tx, _rx) = mpsc::sync_channel(4);
        let cursor = Cursor::new(Vec::new());
        let reader = PtyReader::new(Box::new(cursor), tx);
        let debug = format!("{:?}", reader);
        assert!(debug.contains("PtyReader"));
    }

    #[test]
    fn test_pty_writer_debug() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        let debug = format!("{:?}", writer);
        assert!(debug.contains("PtyWriter"));
    }

    #[test]
    fn test_pty_writer_write_sync() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        let n = writer.write_sync(b"sync write").unwrap();
        assert_eq!(n, 10);
    }

    #[tokio::test]
    async fn test_pty_writer_send_byte() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        writer.send_byte(0x03).await.unwrap(); // Ctrl+C
    }

    #[tokio::test]
    async fn test_pty_writer_send_interrupt() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        writer.send_interrupt().await.unwrap();
    }

    #[tokio::test]
    async fn test_pty_writer_send_eof() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        writer.send_eof().await.unwrap();
    }

    #[tokio::test]
    async fn test_pty_writer_send_suspend() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        writer.send_suspend().await.unwrap();
    }

    #[tokio::test]
    async fn test_pty_writer_write_str() {
        let cursor = Cursor::new(Vec::new());
        let writer = PtyWriter::new(Box::new(cursor));
        let n = writer.write_str("hello pty").await.unwrap();
        assert_eq!(n, 9);
    }

    #[test]
    fn test_pty_reader_receives_multiple_chunks() {
        // Reader should handle EOF after reading data
        let data = vec![0u8; 1024];
        let cursor = Cursor::new(data.clone());
        let (tx, rx) = mpsc::sync_channel(16);
        let _reader = PtyReader::new(Box::new(cursor), tx);

        std::thread::sleep(std::time::Duration::from_millis(50));

        let received = rx.try_recv().unwrap();
        assert_eq!(received.len(), 1024);
    }
}
