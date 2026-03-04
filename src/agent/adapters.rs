//! Agent adapter implementations

mod mock;

pub use mock::{MockAgent, Workflow};

#[cfg(feature = "rig")]
mod rig;
#[cfg(feature = "rig")]
pub use rig::{RigAgent, RigAgentConfig};
