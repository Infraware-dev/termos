//! Engine module — re-exports from infraware-engine for terminal use.

pub use infraware_engine::adapters::MockEngine;
#[cfg(feature = "rig")]
pub use infraware_engine::adapters::{RigEngine, RigEngineConfig};
pub use infraware_engine::{
    AgentEvent, AgenticEngine, EngineError, EventStream, HealthStatus, IncidentPhase, Interrupt,
    Message, MessageRole, ResumeResponse, RunInput, ThreadId,
};
