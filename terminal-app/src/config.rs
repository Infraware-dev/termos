//! Terminal configuration constants.
//!
//! Centralizes all magic numbers and configuration values.

use std::time::Duration;

/// Terminal timing configuration.
pub mod timing {
    use super::*;

    /// Cursor blink interval (530ms matches typical terminal behavior).
    pub const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(530);

    /// Shell initialization delay after startup.
    pub const SHELL_INIT_DELAY: Duration = Duration::from_millis(500);

    /// Delay after init commands before enabling rendering.
    /// Allows shell's `clear` to execute before we start showing output.
    pub const INIT_COMMANDS_DELAY: Duration = Duration::from_millis(100);

    /// Background window repaint interval (low CPU mode).
    pub const BACKGROUND_REPAINT: Duration = Duration::from_millis(500);
}

/// Terminal rendering configuration.
pub mod rendering {
    /// Maximum bytes to process from PTY per frame during keyboard activity.
    /// Must be large enough to drain a full screen of escape sequences (~60KB)
    /// in one frame, while still allowing keyboard checks every ~5ms.
    pub const MAX_BYTES_PER_FRAME_ACTIVE: usize = 64 * 1024;

    /// Maximum bytes to process from PTY per frame when idle.
    /// Must be large enough to drain a full screen of escape sequences (~60KB for
    /// 80x24 with 256-color) in a single frame, otherwise throughput is bottlenecked
    /// by multi-frame pipeline latency.
    pub const MAX_BYTES_PER_FRAME_IDLE: usize = 1024 * 1024;

    /// Default font size in points.
    pub const FONT_SIZE: f32 = 14.0;

    /// Default character width in pixels.
    pub const CHAR_WIDTH: f32 = 8.4;

    /// Default character height in pixels.
    pub const CHAR_HEIGHT: f32 = 16.0;
}

/// Terminal size defaults.
pub mod size {
    /// Default terminal rows.
    pub const DEFAULT_ROWS: u16 = 24;

    /// Default terminal columns.
    pub const DEFAULT_COLS: u16 = 80;
}

/// PTY channel configuration.
pub mod pty {
    /// Sync channel capacity for backpressure.
    /// macOS PTY kernel buffer is ~1KB, so each read yields ~1KB.
    /// At 120 FPS with ~87KB/frame, the reader produces ~86 messages per frame.
    /// Capacity must exceed this to prevent reader stalls between consumer drains.
    /// 512 slots = ~6 frames of headroom at ~1KB/message.
    pub const CHANNEL_CAPACITY: usize = 512;

    /// PTY reader buffer size. Larger buffers reduce system call overhead
    /// during heavy output (e.g., cat large_file, colorful TUI apps).
    pub const READER_BUFFER_SIZE: usize = 64 * 1024;
}
