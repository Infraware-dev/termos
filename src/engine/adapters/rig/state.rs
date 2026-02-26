//! State management for the Rig engine adapter

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;

use super::incident::context::{IncidentContext, RiskLevel};
use crate::engine::shared::{Message, ThreadId};

/// In-memory state store for threads and runs
#[derive(Debug)]
pub struct StateStore {
    /// Thread data keyed by thread ID
    threads: RwLock<HashMap<String, ThreadState>>,
    /// Counter for generating unique thread IDs
    thread_counter: AtomicU64,
}

impl Default for StateStore {
    fn default() -> Self {
        Self::new()
    }
}

impl StateStore {
    /// Create a new empty state store
    pub fn new() -> Self {
        Self {
            threads: RwLock::new(HashMap::new()),
            thread_counter: AtomicU64::new(1),
        }
    }

    /// Create a new thread and return its ID
    pub async fn create_thread(&self) -> ThreadId {
        let id = self.thread_counter.fetch_add(1, Ordering::SeqCst);
        let thread_id = format!("rig-thread-{}", id);

        let state = ThreadState {
            messages: Vec::new(),
            active_run: None,
        };

        self.threads.write().await.insert(thread_id.clone(), state);
        ThreadId::new(thread_id)
    }

    /// Check if a thread exists
    pub async fn thread_exists(&self, thread_id: &ThreadId) -> bool {
        self.threads.read().await.contains_key(&thread_id.0)
    }

    /// Add messages to a thread's history
    pub async fn add_messages(&self, thread_id: &ThreadId, messages: Vec<Message>) -> bool {
        let mut threads = self.threads.write().await;
        if let Some(state) = threads.get_mut(&thread_id.0) {
            state.messages.extend(messages);
            true
        } else {
            false
        }
    }

    /// Get conversation history for a thread
    pub async fn get_messages(&self, thread_id: &ThreadId) -> Option<Vec<Message>> {
        self.threads
            .read()
            .await
            .get(&thread_id.0)
            .map(|s| s.messages.clone())
    }

    /// Store a pending interrupt for a thread
    pub async fn store_interrupt(&self, thread_id: &ThreadId, interrupt: PendingInterrupt) -> bool {
        let mut threads = self.threads.write().await;
        if let Some(state) = threads.get_mut(&thread_id.0) {
            state.active_run = Some(RunState {
                pending_interrupt: Some(interrupt),
            });
            return true;
        }
        false
    }

    /// Take (remove and return) a pending interrupt for a thread
    pub async fn take_interrupt(&self, thread_id: &ThreadId) -> Option<PendingInterrupt> {
        let mut threads = self.threads.write().await;
        if let Some(state) = threads.get_mut(&thread_id.0)
            && let Some(ref mut run) = state.active_run
        {
            return run.pending_interrupt.take();
        }
        None
    }

    /// Get the number of threads in the store
    pub async fn thread_count(&self) -> usize {
        self.threads.read().await.len()
    }
}

/// State for a single conversation thread
#[derive(Debug, Clone)]
struct ThreadState {
    /// Conversation history
    messages: Vec<Message>,
    /// Currently active run (if any)
    active_run: Option<RunState>,
}

/// State for an active run
#[derive(Debug, Clone)]
struct RunState {
    /// Pending interrupt waiting for user response
    pending_interrupt: Option<PendingInterrupt>,
}

/// A pending interrupt awaiting user response
#[derive(Debug, Clone)]
pub struct PendingInterrupt {
    /// Context needed to resume after the interrupt
    pub resume_context: ResumeContext,
    /// Tool call ID from rig-rs (for tool result message)
    pub tool_call_id: Option<String>,
    /// Original tool arguments (for retry/debug)
    pub tool_args: Option<serde_json::Value>,
}

impl PendingInterrupt {
    /// Create a new pending command approval interrupt with tool call metadata
    pub fn command_approval_with_tool(
        command: String,
        _message: String,
        needs_continuation: bool,
        tool_call_id: Option<String>,
        tool_args: Option<serde_json::Value>,
    ) -> Self {
        Self {
            resume_context: ResumeContext::CommandApproval {
                command,
                needs_continuation,
            },
            tool_call_id,
            tool_args,
        }
    }

    /// Create a new pending question interrupt with tool call metadata
    pub fn question_with_tool(
        question: String,
        _options: Option<Vec<String>>,
        tool_call_id: Option<String>,
        tool_args: Option<serde_json::Value>,
    ) -> Self {
        Self {
            resume_context: ResumeContext::Question { question },
            tool_call_id,
            tool_args,
        }
    }

    /// Create a pending sudo password interrupt
    pub fn sudo_password(command: String) -> Self {
        Self {
            resume_context: ResumeContext::SudoPassword { command },
            tool_call_id: None,
            tool_args: None,
        }
    }

    /// Create an incident confirmation interrupt (y/n to start pipeline)
    pub fn incident_confirmation(incident_description: String) -> Self {
        Self {
            resume_context: ResumeContext::IncidentConfirmation {
                incident_description,
            },
            tool_call_id: None,
            tool_args: None,
        }
    }

    /// Create an incident command interrupt (operator approves diagnostic command)
    #[expect(
        clippy::too_many_arguments,
        reason = "All fields are required for incident context fidelity"
    )]
    pub fn incident_command(
        command: String,
        motivation: String,
        needs_continuation: bool,
        risk_level: RiskLevel,
        expected_diagnostic_value: String,
        context: IncidentContext,
        tool_call_id: Option<String>,
        tool_args: Option<serde_json::Value>,
    ) -> Self {
        Self {
            resume_context: ResumeContext::IncidentCommand {
                command,
                motivation,
                needs_continuation,
                risk_level,
                expected_diagnostic_value,
                context,
            },
            tool_call_id,
            tool_args,
        }
    }
}

/// Context for resuming after an interrupt
#[derive(Debug, Clone)]
pub enum ResumeContext {
    /// Resuming after command approval/rejection
    CommandApproval {
        /// The command that was proposed
        command: String,
        /// Whether agent needs to process output after execution
        needs_continuation: bool,
    },
    /// Resuming after a question was answered
    Question {
        /// The question that was asked
        question: String,
    },
    /// Waiting for sudo password to execute a command
    SudoPassword {
        /// The command that requires sudo with password
        command: String,
    },
    /// Waiting for operator to confirm starting the incident investigation pipeline
    IncidentConfirmation {
        /// Description of the incident provided by the NormalAgent
        incident_description: String,
    },
    /// Incident investigation in progress — waiting for a diagnostic command to be approved
    IncidentCommand {
        /// The diagnostic command to execute after approval
        command: String,
        /// Why this command is needed (from DiagnosticCommandTool)
        motivation: String,
        /// Whether the InvestigatorAgent needs to process the output to continue
        needs_continuation: bool,
        /// Risk level declared by the agent at call time
        risk_level: RiskLevel,
        /// What diagnostic value was expected from this command
        expected_diagnostic_value: String,
        /// Accumulated investigation context so far
        context: IncidentContext,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_thread() {
        let store = StateStore::new();
        let thread_id = store.create_thread().await;
        assert!(thread_id.as_str().starts_with("rig-thread-"));
        assert!(store.thread_exists(&thread_id).await);
    }

    #[tokio::test]
    async fn test_add_messages() {
        let store = StateStore::new();
        let thread_id = store.create_thread().await;

        let messages = vec![Message::user("Hello"), Message::assistant("Hi there!")];
        assert!(store.add_messages(&thread_id, messages).await);

        let stored = store.get_messages(&thread_id).await.unwrap();
        assert_eq!(stored.len(), 2);
    }

    #[tokio::test]
    async fn test_interrupt_flow() {
        let store = StateStore::new();
        let thread_id = store.create_thread().await;

        // Store an interrupt (needs_continuation=false for simple command)
        let interrupt = PendingInterrupt::command_approval_with_tool(
            "ls -la".to_string(),
            "List files".to_string(),
            false, // needs_continuation
            None,
            None,
        );
        assert!(store.store_interrupt(&thread_id, interrupt).await);

        // Take the interrupt
        let taken = store.take_interrupt(&thread_id).await;
        assert!(taken.is_some());

        // Interrupt should be gone
        assert!(store.take_interrupt(&thread_id).await.is_none());
    }

    #[tokio::test]
    async fn test_nonexistent_thread() {
        let store = StateStore::new();
        let fake_id = ThreadId::new("nonexistent");

        assert!(!store.thread_exists(&fake_id).await);
        assert!(store.get_messages(&fake_id).await.is_none());
        assert!(!store.add_messages(&fake_id, vec![]).await);
    }

    #[tokio::test]
    async fn test_thread_count() {
        let store = StateStore::new();
        assert_eq!(store.thread_count().await, 0);

        store.create_thread().await;
        assert_eq!(store.thread_count().await, 1);

        store.create_thread().await;
        assert_eq!(store.thread_count().await, 2);
    }

    #[tokio::test]
    async fn test_store_interrupt_nonexistent_thread() {
        let store = StateStore::new();
        let fake_id = ThreadId::new("nonexistent");
        let interrupt = PendingInterrupt::command_approval_with_tool(
            "ls".to_string(),
            "list".to_string(),
            false,
            None,
            None,
        );
        assert!(!store.store_interrupt(&fake_id, interrupt).await);
    }

    #[tokio::test]
    async fn test_take_interrupt_nonexistent_thread() {
        let store = StateStore::new();
        let fake_id = ThreadId::new("nonexistent");
        assert!(store.take_interrupt(&fake_id).await.is_none());
    }

    #[tokio::test]
    async fn test_get_messages_returns_clone() {
        let store = StateStore::new();
        let thread_id = store.create_thread().await;

        let messages = vec![Message::user("Hello")];
        store.add_messages(&thread_id, messages).await;

        let retrieved1 = store.get_messages(&thread_id).await.unwrap();
        let retrieved2 = store.get_messages(&thread_id).await.unwrap();
        assert_eq!(retrieved1.len(), retrieved2.len());
    }

    #[test]
    fn test_pending_interrupt_command_approval_with_tool() {
        let interrupt = PendingInterrupt::command_approval_with_tool(
            "ls -la".to_string(),
            "List files".to_string(),
            true,
            Some("tool-123".to_string()),
            Some(serde_json::json!({"command": "ls -la"})),
        );

        match interrupt.resume_context {
            ResumeContext::CommandApproval {
                command,
                needs_continuation,
            } => {
                assert_eq!(command, "ls -la");
                assert!(needs_continuation);
            }
            _ => panic!("Expected CommandApproval context"),
        }
    }

    #[test]
    fn test_pending_interrupt_question_with_tool() {
        let interrupt = PendingInterrupt::question_with_tool(
            "Which option?".to_string(),
            Some(vec!["A".to_string(), "B".to_string()]),
            Some("tool-456".to_string()),
            None,
        );

        match interrupt.resume_context {
            ResumeContext::Question { question } => {
                assert_eq!(question, "Which option?");
            }
            _ => panic!("Expected Question context"),
        }
    }

    #[test]
    fn test_pending_interrupt_sudo_password() {
        let interrupt = PendingInterrupt::sudo_password("apt update".to_string());

        match interrupt.resume_context {
            ResumeContext::SudoPassword { command } => {
                assert_eq!(command, "apt update");
            }
            _ => panic!("Expected SudoPassword context"),
        }
        assert!(interrupt.tool_call_id.is_none());
        assert!(interrupt.tool_args.is_none());
    }

    #[test]
    fn test_resume_context_debug() {
        let ctx = ResumeContext::CommandApproval {
            command: "ls".to_string(),
            needs_continuation: false,
        };
        let debug_str = format!("{:?}", ctx);
        assert!(debug_str.contains("CommandApproval"));
    }

    #[test]
    fn test_state_store_default() {
        let store1 = StateStore::new();
        let store2 = StateStore::default();
        // Both should be empty initially (can't compare directly)
        assert!(format!("{:?}", store1).contains("StateStore"));
        assert!(format!("{:?}", store2).contains("StateStore"));
    }
}
