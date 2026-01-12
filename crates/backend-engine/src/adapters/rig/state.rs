//! State management for the Rig engine adapter

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::sync::RwLock;

use infraware_shared::{Message, ThreadId};

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
}

impl PendingInterrupt {
    /// Create a new pending command approval interrupt
    pub fn command_approval(command: String, _message: String) -> Self {
        Self {
            resume_context: ResumeContext::CommandApproval { command },
        }
    }

    /// Create a new pending question interrupt
    pub fn question(question: String, _options: Option<Vec<String>>) -> Self {
        Self {
            resume_context: ResumeContext::Question { question },
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
    },
    /// Resuming after a question was answered
    Question {
        /// The question that was asked
        question: String,
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

        // Store an interrupt
        let interrupt =
            PendingInterrupt::command_approval("ls -la".to_string(), "List files".to_string());
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
}
