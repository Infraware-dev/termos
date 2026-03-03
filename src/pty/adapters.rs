//! PTY session adapters for different transport layers.

mod local;

pub use self::local::LocalPtySession;
