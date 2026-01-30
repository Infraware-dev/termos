//! Engine adapter implementations

mod mock;

pub use mock::MockEngine;

#[cfg(feature = "http")]
mod http;
#[cfg(feature = "http")]
pub use http::{HttpEngine, HttpEngineConfig};

#[cfg(feature = "process")]
mod process;
#[cfg(feature = "process")]
pub use process::{ProcessEngine, ProcessEngineConfig};

#[cfg(feature = "rig")]
mod rig;
#[cfg(feature = "rig")]
pub use rig::{RigEngine, RigEngineConfig};
