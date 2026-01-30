//! Mock engine for testing

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use async_trait::async_trait;
use futures::stream;
use tokio::sync::RwLock;

use crate::error::EngineError;
use crate::traits::{AgenticEngine, EventStream};
use crate::types::{HealthStatus, ResumeResponse};
use crate::{AgentEvent, Interrupt, Message, MessageRole, RunInput, ThreadId};

/// Mock engine that returns canned responses
///
/// Useful for testing the API layer without a real agent backend.
#[derive(Debug)]
pub struct MockEngine {
    /// Counter for generating unique thread IDs
    thread_counter: AtomicU64,
    /// Stored threads (thread_id -> messages)
    threads: Arc<RwLock<HashMap<String, Vec<Message>>>>,
    /// Pending interrupt for a thread (for testing HITL)
    pending_interrupts: Arc<RwLock<HashMap<String, Interrupt>>>,
}

impl Default for MockEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MockEngine {
    pub fn new() -> Self {
        Self {
            thread_counter: AtomicU64::new(1),
            threads: Arc::new(RwLock::new(HashMap::new())),
            pending_interrupts: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Queue an interrupt for the next run on a thread (for testing)
    pub async fn queue_interrupt(&self, thread_id: &ThreadId, interrupt: Interrupt) {
        self.pending_interrupts
            .write()
            .await
            .insert(thread_id.0.clone(), interrupt);
    }

    /// Generate a mock response based on input
    fn generate_response(input: &str) -> String {
        let input_lower = input.to_lowercase();

        if input_lower.contains("list files") || input_lower.contains("ls") {
            "To list files, use the `ls` command:\n\n```bash\nls -la\n```\n\nThis shows all files including hidden ones with details.".to_string()
        } else if input_lower.contains("docker") {
            "Here are some common Docker commands:\n\n```bash\ndocker ps        # List running containers\ndocker images    # List images\ndocker run       # Run a container\n```".to_string()
        } else if input_lower.contains("kubernetes") || input_lower.contains("k8s") {
            "Kubernetes commands use `kubectl`:\n\n```bash\nkubectl get pods\nkubectl get services\nkubectl describe pod <name>\n```".to_string()
        } else if input_lower.contains("git") {
            "Common Git commands:\n\n```bash\ngit status       # Check status\ngit add .        # Stage changes\ngit commit -m    # Commit\ngit push         # Push to remote\n```".to_string()
        } else {
            format!(
                "I understand you're asking about: \"{}\". In a production environment, I would provide detailed assistance.",
                input
            )
        }
    }
}

#[async_trait]
impl AgenticEngine for MockEngine {
    async fn create_thread(
        &self,
        _metadata: Option<serde_json::Value>,
    ) -> Result<ThreadId, EngineError> {
        let id = self.thread_counter.fetch_add(1, Ordering::SeqCst);
        let thread_id = format!("mock-thread-{}", id);

        self.threads
            .write()
            .await
            .insert(thread_id.clone(), Vec::new());

        tracing::info!(thread_id = %thread_id, "Created mock thread");
        Ok(ThreadId::new(thread_id))
    }

    async fn stream_run(
        &self,
        thread_id: &ThreadId,
        input: RunInput,
    ) -> Result<EventStream, EngineError> {
        // Check thread exists
        let mut threads = self.threads.write().await;
        let messages = threads
            .get_mut(&thread_id.0)
            .ok_or_else(|| EngineError::thread_not_found(&thread_id.0))?;

        // Store input messages
        messages.extend(input.messages.clone());

        // Get user message content
        let user_content = input
            .messages
            .iter()
            .filter(|m| m.role == MessageRole::User)
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join(" ");

        // Check for pending interrupt
        let pending = self.pending_interrupts.write().await.remove(&thread_id.0);

        let run_id = format!("mock-run-{}", uuid::Uuid::new_v4());
        tracing::info!(thread_id = %thread_id, run_id = %run_id, "Starting mock run");

        // Build event stream
        let events: Vec<Result<AgentEvent, EngineError>> = if let Some(interrupt) = pending {
            // Return interrupt
            vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::updates_with_interrupt(interrupt)),
            ]
        } else {
            // Generate response
            let response = Self::generate_response(&user_content);
            let assistant_msg = Message::assistant(&response);

            // Store assistant response while still holding the lock
            messages.push(assistant_msg.clone());

            vec![
                Ok(AgentEvent::metadata(&run_id)),
                Ok(AgentEvent::Values {
                    messages: vec![assistant_msg],
                }),
                Ok(AgentEvent::end()),
            ]
        };

        // Lock is released here when `threads` goes out of scope

        Ok(Box::pin(stream::iter(events)))
    }

    async fn resume_run(
        &self,
        thread_id: &ThreadId,
        response: ResumeResponse,
    ) -> Result<EventStream, EngineError> {
        // Check thread exists
        if !self.threads.read().await.contains_key(&thread_id.0) {
            return Err(EngineError::thread_not_found(&thread_id.0));
        }

        let run_id = format!("mock-run-{}", uuid::Uuid::new_v4());
        tracing::info!(thread_id = %thread_id, run_id = %run_id, ?response, "Resuming mock run");

        let response_text = match response {
            ResumeResponse::Approved => "Command approved and executed successfully.".to_string(),
            ResumeResponse::Rejected => "Command was rejected by user.".to_string(),
            ResumeResponse::Answer { text } => {
                format!("Received your answer: \"{}\". Processing...", text)
            }
            ResumeResponse::CommandOutput { command, output } => {
                format!(
                    "Command `{}` executed in terminal.\nOutput ({} chars): {}",
                    command,
                    output.len(),
                    if output.len() > 100 {
                        format!("{}...", &output[..100])
                    } else {
                        output
                    }
                )
            }
        };

        let events: Vec<Result<AgentEvent, EngineError>> = vec![
            Ok(AgentEvent::metadata(&run_id)),
            Ok(AgentEvent::Values {
                messages: vec![Message::assistant(response_text)],
            }),
            Ok(AgentEvent::end()),
        ];

        Ok(Box::pin(stream::iter(events)))
    }

    async fn health_check(&self) -> Result<HealthStatus, EngineError> {
        Ok(HealthStatus::healthy().with_details(serde_json::json!({
            "engine": "mock",
            "threads": self.threads.read().await.len()
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    #[tokio::test]
    async fn test_create_thread() {
        let engine = MockEngine::new();
        let thread_id = engine.create_thread(None).await.unwrap();
        assert!(thread_id.as_str().starts_with("mock-thread-"));
    }

    #[tokio::test]
    async fn test_stream_run() {
        let engine = MockEngine::new();
        let thread_id = engine.create_thread(None).await.unwrap();

        let input = RunInput::single_user_message("How do I list files?");
        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        assert!(events.len() >= 2);
        assert!(matches!(events[0], AgentEvent::Metadata { .. }));
    }

    #[tokio::test]
    async fn test_stream_run_with_interrupt() {
        let engine = MockEngine::new();
        let thread_id = engine.create_thread(None).await.unwrap();

        // Queue an interrupt
        engine
            .queue_interrupt(
                &thread_id,
                Interrupt::command_approval("rm -rf temp/", "Clean temp files", false),
            )
            .await;

        let input = RunInput::single_user_message("Clean up");
        let mut stream = engine.stream_run(&thread_id, input).await.unwrap();

        let mut found_interrupt = false;
        while let Some(event) = stream.next().await {
            if let AgentEvent::Updates { interrupts } = event.unwrap() {
                if interrupts.is_some() {
                    found_interrupt = true;
                }
            }
        }

        assert!(found_interrupt);
    }

    #[tokio::test]
    async fn test_resume_run() {
        let engine = MockEngine::new();
        let thread_id = engine.create_thread(None).await.unwrap();

        let mut stream = engine
            .resume_run(&thread_id, ResumeResponse::approved())
            .await
            .unwrap();

        let mut events = Vec::new();
        while let Some(event) = stream.next().await {
            events.push(event.unwrap());
        }

        assert!(!events.is_empty());
    }

    #[tokio::test]
    async fn test_health_check() {
        let engine = MockEngine::new();
        let status = engine.health_check().await.unwrap();
        assert!(status.healthy);
    }

    #[tokio::test]
    async fn test_thread_not_found() {
        let engine = MockEngine::new();
        let fake_thread = ThreadId::new("nonexistent");
        let input = RunInput::single_user_message("Hello");

        let result = engine.stream_run(&fake_thread, input).await;
        assert!(matches!(result, Err(EngineError::ThreadNotFound(_))));
    }
}
