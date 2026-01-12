//! Process Engine - Subprocess communication via stdio
//!
//! This engine spawns a subprocess and communicates with it via JSON-RPC over stdin/stdout.
//! Useful for running Python-based LangGraph agents as subprocesses.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;

use crate::error::EngineError;
use crate::ipc::protocol::{JsonRpcEvent, JsonRpcRequest};
use crate::ipc::stdio::{StdioConfig, StdioTransport};
use crate::traits::{AgenticEngine, EventStream};
use crate::types::{HealthStatus, ResumeResponse};
use crate::{AgentEvent, Interrupt, Message, MessageRole, RunInput, ThreadId};
use infraware_shared::MessageEvent;

/// Default timeout for operations (5 minutes)
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(300);

/// Configuration for the process engine
#[derive(Debug, Clone)]
pub struct ProcessEngineConfig {
    /// Command to run (e.g., "python3")
    pub command: String,
    /// Arguments (e.g., ["bridge.py"])
    pub args: Vec<String>,
    /// Working directory for the subprocess
    pub working_dir: Option<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
    /// Timeout for operations
    pub timeout: Duration,
}

impl Default for ProcessEngineConfig {
    fn default() -> Self {
        Self {
            command: "python3".to_string(),
            args: vec!["bridge.py".to_string()],
            working_dir: None,
            env: Vec::new(),
            timeout: DEFAULT_TIMEOUT,
        }
    }
}

impl ProcessEngineConfig {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            ..Default::default()
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn with_working_dir(mut self, dir: impl Into<String>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Create a StdioConfig from this config
    fn to_stdio_config(&self) -> StdioConfig {
        let mut config = StdioConfig::new(&self.command).with_args(self.args.clone());

        if let Some(ref dir) = self.working_dir {
            config = config.with_working_dir(dir);
        }

        for (key, value) in &self.env {
            config = config.with_env(key, value);
        }

        config
    }
}

/// Process Engine that communicates with a subprocess via JSON-RPC
#[derive(Debug)]
pub struct ProcessEngine {
    config: ProcessEngineConfig,
    transport: Arc<Mutex<Option<StdioTransport>>>,
    request_counter: Arc<Mutex<u64>>,
}

impl ProcessEngine {
    /// Create a new process engine with the given configuration
    pub fn new(config: ProcessEngineConfig) -> Self {
        Self {
            config,
            transport: Arc::new(Mutex::new(None)),
            request_counter: Arc::new(Mutex::new(0)),
        }
    }

    /// Spawn the subprocess if not already running
    pub async fn ensure_started(&self) -> Result<(), EngineError> {
        let mut transport_guard = self.transport.lock().await;

        if transport_guard.is_none() {
            let stdio_config = self.config.to_stdio_config();
            let mut transport = StdioTransport::new(stdio_config);
            transport.spawn().await?;
            *transport_guard = Some(transport);
            tracing::info!("Process engine subprocess started");
        }

        Ok(())
    }

    /// Generate next request ID
    async fn next_request_id(&self) -> String {
        let mut counter = self.request_counter.lock().await;
        *counter += 1;
        format!("req-{}", *counter)
    }

    /// Convert JsonRpcEvent to AgentEvent
    fn convert_event(event: &JsonRpcEvent) -> Option<AgentEvent> {
        match event {
            JsonRpcEvent::Metadata { run_id } => Some(AgentEvent::metadata(run_id.clone())),
            JsonRpcEvent::Message { role, content } => {
                let msg_role = match role.as_str() {
                    "user" => MessageRole::User,
                    "assistant" => MessageRole::Assistant,
                    "system" => MessageRole::System,
                    _ => MessageRole::Assistant, // Default to assistant for unknown roles
                };
                Some(AgentEvent::Message(MessageEvent::new(
                    msg_role,
                    content.clone(),
                )))
            }
            JsonRpcEvent::Values { messages } => {
                let parsed_messages: Vec<Message> = messages
                    .iter()
                    .filter_map(|m| {
                        let role = match m.get("role").and_then(|v| v.as_str()) {
                            Some("user") => MessageRole::User,
                            Some("assistant") => MessageRole::Assistant,
                            Some("system") => MessageRole::System,
                            _ => return None,
                        };
                        let content = m.get("content").and_then(|v| v.as_str())?;
                        Some(Message::new(role, content))
                    })
                    .collect();

                if !parsed_messages.is_empty() {
                    Some(AgentEvent::Values {
                        messages: parsed_messages,
                    })
                } else {
                    None
                }
            }
            JsonRpcEvent::Updates { interrupts } => {
                if let Some(ints) = interrupts {
                    let parsed_interrupts: Vec<Interrupt> = ints
                        .iter()
                        .filter_map(|int| {
                            if int.get("command").is_some() {
                                let command = int
                                    .get("command")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let message = int
                                    .get("message")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Command requires approval")
                                    .to_string();
                                Some(Interrupt::command_approval(command, message))
                            } else {
                                let question = int
                                    .get("question")
                                    .or_else(|| int.get("message"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("Agent is asking for input")
                                    .to_string();
                                let options =
                                    int.get("options").and_then(|v| v.as_array()).map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(String::from))
                                            .collect()
                                    });
                                Some(Interrupt::question(question, options))
                            }
                        })
                        .collect();

                    if !parsed_interrupts.is_empty() {
                        return Some(AgentEvent::Updates {
                            interrupts: Some(parsed_interrupts),
                        });
                    }
                }
                None
            }
            JsonRpcEvent::Error { message } => Some(AgentEvent::error(message.clone())),
            JsonRpcEvent::End => Some(AgentEvent::end()),
        }
    }
}

#[async_trait]
impl AgenticEngine for ProcessEngine {
    async fn create_thread(
        &self,
        metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError> {
        let timeout = self.config.timeout;

        tokio::time::timeout(timeout, async {
            self.ensure_started().await?;

            let request_id = self.next_request_id().await;
            let request = JsonRpcRequest::create_thread(&request_id, metadata);

            let mut transport_guard = self.transport.lock().await;
            let transport = transport_guard
                .as_mut()
                .ok_or_else(|| EngineError::Connection("Transport not initialized".to_string()))?;

            transport.send(&request).await?;

            // Wait for the result response
            let responses = transport.recv_until_final(&request_id).await?;

            for response in responses {
                if let Some(result) = response.result {
                    if let Some(thread_id) = result.get("thread_id").and_then(|v| v.as_str()) {
                        tracing::info!(thread_id = %thread_id, "Thread created via subprocess");
                        return Ok(ThreadId::new(thread_id.to_string()));
                    }
                }
                if let Some(error) = response.error {
                    return Err(EngineError::Other(anyhow::anyhow!(
                        "Subprocess error: {}",
                        error.message
                    )));
                }
            }

            Err(EngineError::Other(anyhow::anyhow!(
                "No thread_id in response"
            )))
        })
        .await
        .map_err(|_| EngineError::timeout("create_thread timed out"))?
    }

    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError> {
        let timeout = self.config.timeout;
        let thread_id = thread_id.clone();
        let input = input.clone();

        // Timeout the setup phase
        let (response_rx, req_id) = tokio::time::timeout(timeout, async {
            self.ensure_started().await?;

            let request_id = self.next_request_id().await;
            let input_json = serde_json::json!({
                "messages": input.messages.iter().map(|m| {
                    serde_json::json!({
                        "role": match m.role {
                            MessageRole::User => "user",
                            MessageRole::Assistant => "assistant",
                            MessageRole::System => "system",
                        },
                        "content": m.content
                    })
                }).collect::<Vec<_>>()
            });
            let request = JsonRpcRequest::stream_run(&request_id, thread_id.as_str(), input_json);

            let mut transport_guard = self.transport.lock().await;
            let transport = transport_guard
                .as_mut()
                .ok_or_else(|| EngineError::Connection("Transport not initialized".to_string()))?;

            transport.send(&request).await?;

            // Take ownership of the response receiver for streaming
            let response_rx = transport.take_response_rx().ok_or_else(|| {
                EngineError::Connection("Response channel not available".to_string())
            })?;

            Ok::<_, EngineError>((response_rx, request_id))
        })
        .await
        .map_err(|_| EngineError::timeout("stream_run setup timed out"))??;

        // Create stream from responses
        let stream = async_stream::stream! {
            let mut rx = response_rx;

            while let Some(result) = rx.recv().await {
                match result {
                    Ok(response) => {
                        // Filter to only responses for this request
                        if response.id != req_id {
                            continue;
                        }

                        // Convert event to AgentEvent
                        if let Some(event) = &response.event {
                            if let Some(agent_event) = Self::convert_event(event) {
                                yield Ok(agent_event);

                                // If it's an End event, stop
                                if matches!(event, JsonRpcEvent::End) {
                                    break;
                                }
                            }
                        }

                        // If it's a final response (result or error), stop
                        if response.is_final() {
                            if let Some(error) = response.error {
                                yield Err(EngineError::Other(anyhow::anyhow!(
                                    "Subprocess error: {}",
                                    error.message
                                )));
                            }
                            break;
                        }
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError> {
        let timeout = self.config.timeout;
        let thread_id = thread_id.clone();

        // Timeout the setup phase
        let (response_rx, req_id) = tokio::time::timeout(timeout, async {
            self.ensure_started().await?;

            let request_id = self.next_request_id().await;
            let response_json = match response {
                ResumeResponse::Approved => serde_json::json!({ "approved": true }),
                ResumeResponse::Rejected => serde_json::json!({ "approved": false }),
                ResumeResponse::Answer { text } => serde_json::json!({ "answer": text }),
            };
            let request =
                JsonRpcRequest::resume_run(&request_id, thread_id.as_str(), response_json);

            let mut transport_guard = self.transport.lock().await;
            let transport = transport_guard
                .as_mut()
                .ok_or_else(|| EngineError::Connection("Transport not initialized".to_string()))?;

            transport.send(&request).await?;

            // Take ownership of the response receiver for streaming
            let response_rx = transport.take_response_rx().ok_or_else(|| {
                EngineError::Connection("Response channel not available".to_string())
            })?;

            Ok::<_, EngineError>((response_rx, request_id))
        })
        .await
        .map_err(|_| EngineError::timeout("resume_run setup timed out"))??;

        // Create stream from responses
        let stream = async_stream::stream! {
            let mut rx = response_rx;

            while let Some(result) = rx.recv().await {
                match result {
                    Ok(response) => {
                        if response.id != req_id {
                            continue;
                        }

                        if let Some(event) = &response.event {
                            if let Some(agent_event) = Self::convert_event(event) {
                                yield Ok(agent_event);

                                if matches!(event, JsonRpcEvent::End) {
                                    break;
                                }
                            }
                        }

                        if response.is_final() {
                            if let Some(error) = response.error {
                                yield Err(EngineError::Other(anyhow::anyhow!(
                                    "Subprocess error: {}",
                                    error.message
                                )));
                            }
                            break;
                        }
                    }
                    Err(e) => {
                        yield Err(e);
                        break;
                    }
                }
            }
        };

        Ok(Box::pin(stream))
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        // Try to ensure subprocess is started
        if let Err(e) = self.ensure_started().await {
            return Ok(HealthStatus::unhealthy(format!(
                "Cannot start subprocess: {}",
                e
            )));
        }

        let request_id = self.next_request_id().await;
        let request = JsonRpcRequest::health_check(&request_id);

        let mut transport_guard = self.transport.lock().await;
        let transport = transport_guard
            .as_mut()
            .ok_or_else(|| EngineError::Connection("Transport not initialized".to_string()))?;

        // Check if subprocess is still running
        if !transport.is_running() {
            return Ok(HealthStatus::unhealthy("Subprocess is not running"));
        }

        transport.send(&request).await?;

        // Wait for response with timeout
        match tokio::time::timeout(
            std::time::Duration::from_secs(5),
            transport.recv_until_final(&request_id),
        )
        .await
        {
            Ok(Ok(responses)) => {
                for response in responses {
                    if let Some(result) = response.result {
                        return Ok(HealthStatus::healthy().with_details(serde_json::json!({
                            "engine": "process",
                            "command": self.config.command,
                            "subprocess": result
                        })));
                    }
                    if let Some(error) = response.error {
                        return Ok(HealthStatus::unhealthy(format!(
                            "Health check failed: {}",
                            error.message
                        )));
                    }
                }
                Ok(HealthStatus::healthy().with_details(serde_json::json!({
                    "engine": "process",
                    "command": self.config.command,
                    "status": "running"
                })))
            }
            Ok(Err(e)) => Ok(HealthStatus::unhealthy(format!(
                "Health check error: {}",
                e
            ))),
            Err(_) => Ok(HealthStatus::unhealthy(
                "Health check timed out after 5 seconds",
            )),
        }
    }
}

impl Drop for ProcessEngine {
    fn drop(&mut self) {
        // Transport's drop will handle killing the subprocess
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = ProcessEngineConfig::default();
        assert_eq!(config.command, "python3");
        assert_eq!(config.args, vec!["bridge.py"]);
    }

    #[test]
    fn test_config_builder() {
        let config = ProcessEngineConfig::new("python3")
            .with_args(vec!["scripts/bridge.py".to_string()])
            .with_working_dir("/opt/agent")
            .with_env("DEBUG", "1");

        assert_eq!(config.command, "python3");
        assert_eq!(config.args, vec!["scripts/bridge.py"]);
        assert_eq!(config.working_dir, Some("/opt/agent".to_string()));
        assert_eq!(config.env, vec![("DEBUG".to_string(), "1".to_string())]);
    }

    #[test]
    fn test_convert_event_metadata() {
        let event = JsonRpcEvent::Metadata {
            run_id: "run-123".to_string(),
        };
        let agent_event = ProcessEngine::convert_event(&event).unwrap();
        match agent_event {
            AgentEvent::Metadata { run_id } => assert_eq!(run_id, "run-123"),
            _ => panic!("Expected Metadata event"),
        }
    }

    #[test]
    fn test_convert_event_end() {
        let event = JsonRpcEvent::End;
        let agent_event = ProcessEngine::convert_event(&event).unwrap();
        assert!(matches!(agent_event, AgentEvent::End));
    }

    #[test]
    fn test_convert_event_error() {
        let event = JsonRpcEvent::Error {
            message: "Something went wrong".to_string(),
        };
        let agent_event = ProcessEngine::convert_event(&event).unwrap();
        match agent_event {
            AgentEvent::Error { message } => assert_eq!(message, "Something went wrong"),
            _ => panic!("Expected Error event"),
        }
    }
}
