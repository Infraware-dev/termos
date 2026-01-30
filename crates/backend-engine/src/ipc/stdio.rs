//! Stdio transport for subprocess communication
//!
//! Handles spawning a subprocess and communicating via stdin/stdout.

use std::process::Stdio;
use std::sync::Arc;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, mpsc};

use crate::error::EngineError;
use crate::ipc::protocol::{JsonRpcRequest, JsonRpcResponse};

/// Configuration for stdio transport
#[derive(Debug, Clone)]
pub struct StdioConfig {
    /// Command to run (e.g., "python3")
    pub command: String,
    /// Arguments (e.g., ["bridge.py"])
    pub args: Vec<String>,
    /// Working directory (optional)
    pub working_dir: Option<String>,
    /// Environment variables to set
    pub env: Vec<(String, String)>,
}

impl StdioConfig {
    pub fn new(command: impl Into<String>) -> Self {
        Self {
            command: command.into(),
            args: Vec::new(),
            working_dir: None,
            env: Vec::new(),
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
}

/// Stdio transport for subprocess communication
#[derive(Debug)]
pub struct StdioTransport {
    config: StdioConfig,
    child: Option<Child>,
    stdin: Option<Arc<Mutex<ChildStdin>>>,
    response_rx: Option<mpsc::Receiver<Result<JsonRpcResponse, EngineError>>>,
}

impl StdioTransport {
    /// Create a new stdio transport with the given configuration
    pub fn new(config: StdioConfig) -> Self {
        Self {
            config,
            child: None,
            stdin: None,
            response_rx: None,
        }
    }

    /// Spawn the subprocess and start communication
    pub async fn spawn(&mut self) -> Result<(), EngineError> {
        let mut cmd = Command::new(&self.config.command);
        cmd.args(&self.config.args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit()); // Let stderr pass through for debugging

        if let Some(ref dir) = self.config.working_dir {
            cmd.current_dir(dir);
        }

        for (key, value) in &self.config.env {
            cmd.env(key, value);
        }

        tracing::info!(
            command = %self.config.command,
            args = ?self.config.args,
            "Spawning subprocess"
        );

        let mut child = cmd.spawn().map_err(|e| {
            EngineError::Connection(format!("Failed to spawn {}: {}", self.config.command, e))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            EngineError::Connection("Failed to capture subprocess stdin".to_string())
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            EngineError::Connection("Failed to capture subprocess stdout".to_string())
        })?;

        // Create channel for responses
        let (tx, rx) = mpsc::channel(100);

        // Spawn reader task
        tokio::spawn(async move {
            Self::read_responses(stdout, tx).await;
        });

        self.child = Some(child);
        self.stdin = Some(Arc::new(Mutex::new(stdin)));
        self.response_rx = Some(rx);

        tracing::info!("Subprocess started successfully");
        Ok(())
    }

    /// Read responses from stdout and send to channel
    async fn read_responses(
        stdout: ChildStdout,
        tx: mpsc::Sender<Result<JsonRpcResponse, EngineError>>,
    ) {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();

        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    if line.trim().is_empty() {
                        continue;
                    }

                    tracing::trace!(line = %line, "Received line from subprocess");

                    let result = JsonRpcResponse::from_json_line(&line)
                        .map_err(|e| EngineError::Other(anyhow::anyhow!("Parse error: {}", e)));

                    if tx.send(result).await.is_err() {
                        tracing::debug!("Response channel closed");
                        break;
                    }
                }
                Ok(None) => {
                    tracing::info!("Subprocess stdout closed");
                    break;
                }
                Err(e) => {
                    tracing::error!(error = %e, "Error reading from subprocess");
                    let _ = tx
                        .send(Err(EngineError::Connection(format!("Read error: {}", e))))
                        .await;
                    break;
                }
            }
        }
    }

    /// Send a request to the subprocess
    pub async fn send(&self, request: &JsonRpcRequest) -> Result<(), EngineError> {
        let stdin = self
            .stdin
            .as_ref()
            .ok_or_else(|| EngineError::Connection("Subprocess not started".to_string()))?;

        let json = request
            .to_json_line()
            .map_err(|e| EngineError::Other(anyhow::anyhow!("Serialization error: {}", e)))?;

        tracing::trace!(json = %json.trim(), "Sending to subprocess");

        let mut stdin = stdin.lock().await;
        stdin
            .write_all(json.as_bytes())
            .await
            .map_err(|e| EngineError::Connection(format!("Write error: {}", e)))?;
        stdin
            .flush()
            .await
            .map_err(|e| EngineError::Connection(format!("Flush error: {}", e)))?;

        Ok(())
    }

    /// Receive the next response from the subprocess
    pub async fn recv(&mut self) -> Option<Result<JsonRpcResponse, EngineError>> {
        self.response_rx.as_mut()?.recv().await
    }

    /// Receive responses for a specific request ID until final response
    pub async fn recv_until_final(
        &mut self,
        request_id: &str,
    ) -> Result<Vec<JsonRpcResponse>, EngineError> {
        let mut responses = Vec::new();

        while let Some(result) = self.recv().await {
            let response = result?;

            if response.id != request_id {
                tracing::warn!(
                    expected = %request_id,
                    got = %response.id,
                    "Received response for different request"
                );
                continue;
            }

            let is_final = response.is_final();
            responses.push(response);

            if is_final {
                break;
            }
        }

        Ok(responses)
    }

    /// Get stdin for direct writing (used for streaming)
    pub fn stdin(&self) -> Option<Arc<Mutex<ChildStdin>>> {
        self.stdin.clone()
    }

    /// Take the response receiver (for streaming)
    pub fn take_response_rx(
        &mut self,
    ) -> Option<mpsc::Receiver<Result<JsonRpcResponse, EngineError>>> {
        self.response_rx.take()
    }

    /// Check if subprocess is running
    pub fn is_running(&mut self) -> bool {
        if let Some(ref mut child) = self.child {
            match child.try_wait() {
                Ok(None) => true, // Still running
                Ok(Some(status)) => {
                    tracing::info!(status = ?status, "Subprocess exited");
                    false
                }
                Err(e) => {
                    tracing::error!(error = %e, "Error checking subprocess status");
                    false
                }
            }
        } else {
            false
        }
    }

    /// Kill the subprocess
    pub async fn kill(&mut self) -> Result<(), EngineError> {
        if let Some(ref mut child) = self.child {
            child.kill().await.map_err(|e| {
                EngineError::Connection(format!("Failed to kill subprocess: {}", e))
            })?;
            tracing::info!("Subprocess killed");
        }
        Ok(())
    }
}

impl Drop for StdioTransport {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.child {
            // Try to kill the subprocess when transport is dropped
            let _ = child.start_kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = StdioConfig::new("python3")
            .with_arg("bridge.py")
            .with_working_dir("/tmp")
            .with_env("DEBUG", "1");

        assert_eq!(config.command, "python3");
        assert_eq!(config.args, vec!["bridge.py"]);
        assert_eq!(config.working_dir, Some("/tmp".to_string()));
        assert_eq!(config.env, vec![("DEBUG".to_string(), "1".to_string())]);
    }
}
