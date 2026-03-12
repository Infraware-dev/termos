//! Context and persistent memory for the Rig LLM engine.

pub mod persistent;
pub mod session_context;

use std::sync::Arc;

use tokio::sync::RwLock;

use self::persistent::MemoryStore;
use self::session_context::SessionContextStore;

/// Pre-rendered system prompt fragments from the memory stores.
///
/// Returned by [`MemoryContext::build_preambles`] so callers can inject
/// these strings into agent preambles without holding read locks.
#[derive(Debug, Clone)]
pub struct Preambles {
    /// Session-scoped context facts (ephemeral, per-thread).
    pub session: String,
    /// Persistent cross-session memory.
    pub memory: String,
}

/// Bundles every memory-related dependency that an agent builder needs.
///
/// Pass this single value to any agent builder to guarantee that both the
/// system-prompt preambles and the save-tools are always wired in.
#[derive(Debug, Clone)]
pub struct MemoryContext {
    /// Shared persistent memory store (cross-session).
    pub memory_store: Arc<RwLock<MemoryStore>>,
    /// Shared session-scoped context store (ephemeral, per-thread).
    pub session_context_store: Arc<RwLock<SessionContextStore>>,
}

impl MemoryContext {
    /// Creates a new context referencing the given stores.
    pub fn new(
        memory_store: Arc<RwLock<MemoryStore>>,
        session_context_store: Arc<RwLock<SessionContextStore>>,
    ) -> Self {
        Self {
            memory_store,
            session_context_store,
        }
    }

    /// Reads both stores and renders the system-prompt preamble fragments.
    ///
    /// Acquires read locks on both stores, so the caller does **not** need
    /// to hold any locks beforehand.
    pub async fn build_preambles(&self) -> Preambles {
        let memory = self.memory_store.read().await;
        let session = self.session_context_store.read().await;

        Preambles {
            session: session.build_preamble(),
            memory: memory.build_preamble(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapters::rig::memory::session_context::DEFAULT_SESSION_CONTEXT_LIMIT;

    #[tokio::test]
    async fn build_preambles_empty_stores() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("memory.json");
        let memory = MemoryStore::load_or_create(&path, 10).unwrap();
        let ctx = MemoryContext::new(
            Arc::new(RwLock::new(memory)),
            Arc::new(RwLock::new(SessionContextStore::new(
                DEFAULT_SESSION_CONTEXT_LIMIT,
            ))),
        );
        let preambles = ctx.build_preambles().await;
        // Session context is empty when no facts have been discovered
        assert!(preambles.session.is_empty());
        // Persistent memory always includes the system prompt header
        assert!(!preambles.memory.is_empty());
        assert!(!preambles.memory.contains("Known Context"));
    }
}
