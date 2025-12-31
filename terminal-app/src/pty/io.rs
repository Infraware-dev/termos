//! Async I/O for PTY operations.
//!
//! Provides async wrappers around the synchronous PTY reader/writer,
//! using a dedicated reader thread with channels for non-blocking operation.

use super::traits::PtyWrite;
use anyhow::{Context, Result};
use std::fmt;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Async reader for PTY output.
///
/// Uses a dedicated background thread for reading to avoid blocking.
/// Data is sent via a channel for async consumption.
pub struct PtyReader {
    /// Receiver for data from the reader thread
    receiver: tokio::sync::mpsc::Receiver<Vec<u8>>,
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
        self.stop_flag.store(true, Ordering::SeqCst);
    }
}

#[expect(dead_code, reason = "Public API - reader methods used by PtyManager")]
impl PtyReader {
    /// Create a new async PTY reader with a background reading thread.
    pub fn new(mut reader: Box<dyn Read + Send>) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(32);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_flag_clone = stop_flag.clone();

        // Spawn a dedicated thread for reading from PTY
        std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];

            loop {
                // Check if we should stop
                if stop_flag_clone.load(Ordering::SeqCst) {
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
                        // Send data through channel
                        if tx.blocking_send(data).is_err() {
                            // Channel closed, stop reading
                            break;
                        }
                    }
                    Err(e) => {
                        if e.kind() != std::io::ErrorKind::Interrupted {
                            log::debug!("PTY reader error: {}", e);
                            break;
                        }
                        // Interrupted, continue reading
                    }
                }
            }

            log::debug!("PTY reader thread exiting");
        });

        Self {
            receiver: rx,
            stop_flag,
        }
    }

    /// Read available data from the PTY without blocking.
    ///
    /// Returns data if available, or empty Vec if no data ready.
    pub async fn read_available(&mut self) -> Result<Vec<u8>> {
        // Try to receive data from the channel without blocking
        match self.receiver.try_recv() {
            Ok(data) => Ok(data),
            Err(tokio::sync::mpsc::error::TryRecvError::Empty) => Ok(Vec::new()),
            Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                // Reader thread has stopped
                Ok(Vec::new())
            }
        }
    }

    /// Read with timeout - waits up to the specified duration for data.
    /// Returns error if the channel is closed (shell exited).
    pub async fn read_with_timeout(&mut self, timeout: std::time::Duration) -> Result<Vec<u8>> {
        match tokio::time::timeout(timeout, self.receiver.recv()).await {
            Ok(Some(data)) => Ok(data),
            Ok(None) => {
                // Channel closed - shell exited
                anyhow::bail!("PTY channel closed - shell exited")
            }
            Err(_) => Ok(Vec::new()), // Timeout - no data yet
        }
    }

    /// Blocking read - waits indefinitely for data from PTY.
    /// This is more efficient than polling with timeout as the thread sleeps
    /// until data is actually available.
    /// Returns error if the channel is closed (shell exited).
    pub async fn read(&mut self) -> Result<Vec<u8>> {
        match self.receiver.recv().await {
            Some(data) => Ok(data),
            None => {
                // Channel closed - shell exited
                anyhow::bail!("PTY channel closed - shell exited")
            }
        }
    }

    /// Check if the reader channel is still open.
    pub fn is_alive(&self) -> bool {
        !self.stop_flag.load(Ordering::SeqCst)
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

#[expect(dead_code, reason = "Public API - writer methods used by InfrawareApp")]
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
        writer
            .write_all(data)
            .context("Failed to write to PTY")?;
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
    use super::*;
    use std::io::Cursor;

    #[tokio::test]
    async fn test_pty_reader_creation() {
        let data = b"Hello, PTY!";
        let cursor = Cursor::new(data.to_vec());
        let mut reader = PtyReader::new(Box::new(cursor));

        // Wait a bit for the reader thread to read the data
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let result = reader.read_available().await.unwrap();
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
}
