//! Engine adapter implementations

mod mock;

pub use mock::{MockEngine, Workflow};

#[cfg(feature = "rig")]
mod rig;
#[cfg(feature = "rig")]
pub use rig::{RigEngine, RigEngineConfig};
