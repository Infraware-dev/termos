//! PTY (Pseudo-Terminal) module for interactive command support.

pub(crate) mod adapters;
mod io;
mod manager;
mod traits;

pub use io::{PtyReader, PtyWriter};
pub use manager::PtyManager;
use portable_pty::PtySize;
#[expect(
    unused_imports,
    reason = "Public API — trait re-exported for consumers that accept dyn PtySession"
)]
pub use traits::PtySession;
pub use traits::PtyWrite;

/// Default PTY size matching typical terminal dimensions.
pub const DEFAULT_PTY_SIZE: PtySize = PtySize {
    rows: 24,
    cols: 80,
    pixel_width: 0,
    pixel_height: 0,
};
