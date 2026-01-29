//! Inter-Process Communication module
//!
//! Provides JSON-RPC based communication with subprocess engines.

pub mod protocol;
pub mod stdio;

pub use protocol::{JsonRpcEvent, JsonRpcRequest, JsonRpcResponse};
pub use stdio::StdioTransport;
