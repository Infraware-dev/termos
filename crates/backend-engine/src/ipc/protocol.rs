//! JSON-RPC protocol types for subprocess communication
//!
//! Protocol is line-delimited JSON-RPC 2.0 over stdio.
//!
//! ## Request (Rust → Engine)
//! ```json
//! {"jsonrpc":"2.0","id":"uuid","method":"create_thread","params":{"metadata":{}}}
//! {"jsonrpc":"2.0","id":"uuid","method":"stream_run","params":{"thread_id":"...","input":{...}}}
//! {"jsonrpc":"2.0","id":"uuid","method":"resume_run","params":{"thread_id":"...","response":{...}}}
//! {"jsonrpc":"2.0","id":"uuid","method":"health_check","params":{}}
//! ```
//!
//! ## Response (Engine → Rust)
//! ```json
//! {"jsonrpc":"2.0","id":"uuid","result":{"thread_id":"..."}}
//! {"jsonrpc":"2.0","id":"uuid","event":{"type":"metadata","run_id":"..."}}
//! {"jsonrpc":"2.0","id":"uuid","event":{"type":"values","messages":[...]}}
//! {"jsonrpc":"2.0","id":"uuid","event":{"type":"end"}}
//! {"jsonrpc":"2.0","id":"uuid","error":{"code":-32000,"message":"..."}}
//! ```

use serde::{Deserialize, Serialize};

/// JSON-RPC version string
pub const JSONRPC_VERSION: &str = "2.0";

/// JSON-RPC Request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: impl Into<String>, method: impl Into<String>) -> Self {
        Self {
            jsonrpc: JSONRPC_VERSION.to_string(),
            id: id.into(),
            method: method.into(),
            params: None,
        }
    }

    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        self.params = Some(params);
        self
    }

    /// Create a create_thread request
    pub fn create_thread(id: impl Into<String>, metadata: Option<serde_json::Value>) -> Self {
        let params = serde_json::json!({ "metadata": metadata });
        Self::new(id, "create_thread").with_params(params)
    }

    /// Create a stream_run request
    pub fn stream_run(id: impl Into<String>, thread_id: &str, input: serde_json::Value) -> Self {
        let params = serde_json::json!({
            "thread_id": thread_id,
            "input": input
        });
        Self::new(id, "stream_run").with_params(params)
    }

    /// Create a resume_run request
    pub fn resume_run(id: impl Into<String>, thread_id: &str, response: serde_json::Value) -> Self {
        let params = serde_json::json!({
            "thread_id": thread_id,
            "response": response
        });
        Self::new(id, "resume_run").with_params(params)
    }

    /// Create a health_check request
    pub fn health_check(id: impl Into<String>) -> Self {
        Self::new(id, "health_check")
    }

    /// Serialize to JSON line (with newline)
    pub fn to_json_line(&self) -> Result<String, serde_json::Error> {
        let mut json = serde_json::to_string(self)?;
        json.push('\n');
        Ok(json)
    }
}

/// JSON-RPC Response (can be result, event, or error)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event: Option<JsonRpcEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    /// Check if this is a final response (result or error, not an event)
    pub fn is_final(&self) -> bool {
        self.result.is_some() || self.error.is_some()
    }

    /// Check if this is an event (streaming response)
    pub fn is_event(&self) -> bool {
        self.event.is_some()
    }

    /// Check if this is an error
    pub fn is_error(&self) -> bool {
        self.error.is_some()
    }

    /// Parse from JSON line
    pub fn from_json_line(line: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(line.trim())
    }
}

/// JSON-RPC Event (streaming response)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum JsonRpcEvent {
    /// Run metadata
    Metadata { run_id: String },
    /// Message content
    Message { role: String, content: String },
    /// State values (messages array)
    Values { messages: Vec<serde_json::Value> },
    /// State updates (may include interrupts)
    Updates {
        #[serde(skip_serializing_if = "Option::is_none")]
        interrupts: Option<Vec<serde_json::Value>>,
    },
    /// Error occurred
    Error { message: String },
    /// Stream ended
    End,
}

/// JSON-RPC Error
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl JsonRpcError {
    /// Standard error codes
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
    /// Custom error codes (application-specific)
    pub const ENGINE_ERROR: i32 = -32000;
    pub const THREAD_NOT_FOUND: i32 = -32001;
    pub const CONNECTION_ERROR: i32 = -32002;

    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub fn engine_error(message: impl Into<String>) -> Self {
        Self::new(Self::ENGINE_ERROR, message)
    }

    pub fn thread_not_found(thread_id: &str) -> Self {
        Self::new(
            Self::THREAD_NOT_FOUND,
            format!("Thread not found: {}", thread_id),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_serialization() {
        let req = JsonRpcRequest::create_thread("req-1", Some(serde_json::json!({"key": "value"})));
        let json = req.to_json_line().unwrap();
        assert!(json.contains("\"jsonrpc\":\"2.0\""));
        assert!(json.contains("\"id\":\"req-1\""));
        assert!(json.contains("\"method\":\"create_thread\""));
        assert!(json.ends_with('\n'));
    }

    #[test]
    fn test_response_parsing() {
        let json = r#"{"jsonrpc":"2.0","id":"req-1","result":{"thread_id":"t-123"}}"#;
        let resp = JsonRpcResponse::from_json_line(json).unwrap();
        assert_eq!(resp.id, "req-1");
        assert!(resp.is_final());
        assert!(!resp.is_event());
    }

    #[test]
    fn test_event_parsing() {
        let json =
            r#"{"jsonrpc":"2.0","id":"req-1","event":{"type":"metadata","run_id":"run-123"}}"#;
        let resp = JsonRpcResponse::from_json_line(json).unwrap();
        assert!(resp.is_event());
        assert!(!resp.is_final());

        match resp.event.unwrap() {
            JsonRpcEvent::Metadata { run_id } => assert_eq!(run_id, "run-123"),
            _ => panic!("Expected Metadata event"),
        }
    }

    #[test]
    fn test_error_parsing() {
        let json =
            r#"{"jsonrpc":"2.0","id":"req-1","error":{"code":-32000,"message":"Engine error"}}"#;
        let resp = JsonRpcResponse::from_json_line(json).unwrap();
        assert!(resp.is_error());
        assert!(resp.is_final());

        let err = resp.error.unwrap();
        assert_eq!(err.code, JsonRpcError::ENGINE_ERROR);
        assert_eq!(err.message, "Engine error");
    }

    #[test]
    fn test_values_event() {
        let json = r#"{"jsonrpc":"2.0","id":"req-1","event":{"type":"values","messages":[{"role":"user","content":"hello"}]}}"#;
        let resp = JsonRpcResponse::from_json_line(json).unwrap();

        match resp.event.unwrap() {
            JsonRpcEvent::Values { messages } => {
                assert_eq!(messages.len(), 1);
            }
            _ => panic!("Expected Values event"),
        }
    }

    #[test]
    fn test_end_event() {
        let json = r#"{"jsonrpc":"2.0","id":"req-1","event":{"type":"end"}}"#;
        let resp = JsonRpcResponse::from_json_line(json).unwrap();

        match resp.event.unwrap() {
            JsonRpcEvent::End => {}
            _ => panic!("Expected End event"),
        }
    }
}
