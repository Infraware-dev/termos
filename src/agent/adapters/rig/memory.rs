//! Context and persistent memory for the Rig LLM engine.

pub mod persistent;
pub mod session_context;

use std::sync::Arc;

use tokio::sync::RwLock;

use self::persistent::MemoryStore;
use self::session_context::SessionContextStore;

/// Sanitizes user-supplied fact text before storage.
///
/// Collapses consecutive whitespace and newlines into a single space, trims
/// leading/trailing whitespace, and strips leading dashes to prevent markdown
/// list injection.
pub(crate) fn sanitize_fact(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches('-')
        .trim()
        .to_string()
}

/// Pre-rendered system prompt fragments from the memory stores.
///
/// Returned by [`MemoryContext::build_preambles`] so callers can inject
/// these strings into agent preambles without holding read locks.
///
/// Injection order matters: `memory` is appended first, then `session`.
/// Content closer to the **end** of the system prompt receives more LLM
/// attention, so session-scoped facts (which override persistent memory)
/// are placed last.
#[derive(Debug, Clone)]
pub struct Preambles {
    /// Persistent cross-session memory (injected first).
    pub memory: String,
    /// Session-scoped context facts — ephemeral, per-thread (injected last
    /// for higher priority).
    pub session: String,
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
    /// Acquires locks on both stores, so the caller does **not** need to
    /// hold any locks beforehand. The session context store uses a write
    /// lock because [`SessionContextStore::build_preamble`] may update its
    /// internal cache.
    pub async fn build_preambles(&self) -> Preambles {
        let memory = self.memory_store.read().await;
        let mut session = self.session_context_store.write().await;

        Preambles {
            memory: memory.build_preamble(),
            session: session.build_preamble(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapters::rig::memory::session_context::DEFAULT_SESSION_CONTEXT_LIMIT;

    // -- sanitize_fact -------------------------------------------------------

    #[test]
    fn sanitize_fact_collapses_whitespace() {
        assert_eq!(sanitize_fact("  hello   world  "), "hello world");
    }

    #[test]
    fn sanitize_fact_collapses_newlines() {
        assert_eq!(sanitize_fact("hello\n\nworld"), "hello world");
    }

    #[test]
    fn sanitize_fact_strips_leading_dash() {
        assert_eq!(sanitize_fact("- remember this"), "remember this");
    }

    #[test]
    fn sanitize_fact_returns_empty_for_blank() {
        assert_eq!(sanitize_fact("   "), "");
    }

    // -- MemoryContext -------------------------------------------------------

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
