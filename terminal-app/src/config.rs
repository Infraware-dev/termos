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
    /// Lower value ensures Ctrl+C responsiveness during fast output.
    pub const MAX_BYTES_PER_FRAME_ACTIVE: usize = 4096;

    /// Maximum bytes to process from PTY per frame when idle.
    /// Higher value improves throughput for burst output (e.g., cat large_file).
    pub const MAX_BYTES_PER_FRAME_IDLE: usize = 16384;

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
    /// Small value ensures Ctrl+C can interrupt heavy output.
    pub const CHANNEL_CAPACITY: usize = 4;
}
