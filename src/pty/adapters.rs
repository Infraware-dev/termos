//! PTY session adapters for different transport layers.

mod local;
#[cfg(feature = "pty-test_container")]
mod test_container;

pub use self::local::LocalPtySession;
#[cfg(feature = "pty-test_container")]
pub use self::test_container::TestContainerPtySession;
