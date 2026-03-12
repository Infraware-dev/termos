//! Session-scoped context memory for the Rig LLM engine.
//!
//! Unlike persistent [`super::persistent::MemoryStore`] memories, session context
//! entries are ephemeral and live only for the duration of a single terminal
//! session. The LLM uses [`SaveSessionContextTool`] to cache facts discovered
//! during command execution (e.g. OS type, running services) so it can avoid
//! re-running diagnostic commands.

use std::collections::VecDeque;
use std::fmt;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;

/// Maximum number of session context entries kept by default.
pub const DEFAULT_SESSION_CONTEXT_LIMIT: usize = 50;

/// Categorizes a session context entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionContextCategory {
    /// Host information (e.g. OS, kernel version, architecture).
    HostInfo,
    /// Environment details (e.g. shell, locale, PATH entries).
    Environment,
    /// Current or recently observed working directory.
    WorkingDirectory,
    /// State of a running service (e.g. "nginx is active").
    ServiceState,
    /// Discovered facts from exploratory commands.
    Discovery,
}

impl fmt::Display for SessionContextCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::HostInfo => write!(f, "host_info"),
            Self::Environment => write!(f, "environment"),
            Self::WorkingDirectory => write!(f, "working_directory"),
            Self::ServiceState => write!(f, "service_state"),
            Self::Discovery => write!(f, "discovery"),
        }
    }
}

/// A single session-scoped context entry.
#[derive(Debug, Clone)]
pub struct SessionContextEntry {
    /// Human-readable fact statement.
    pub fact: String,
    /// Classification of the context entry.
    pub category: SessionContextCategory,
    /// When the entry was created.
    pub created_at: DateTime<Utc>,
}

/// In-memory store for session-scoped context entries.
///
/// Entries are kept in insertion order and evicted FIFO when the store reaches
/// its configured [`limit`](SessionContextStore::new).
#[derive(Debug)]
pub struct SessionContextStore {
    /// The context entries.
    entries: VecDeque<SessionContextEntry>,
    /// Maximum entries to keep in the store.
    limit: usize,
}

impl SessionContextStore {
    /// Creates an empty store that retains at most `limit` entries.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            limit,
        }
    }

    /// Adds a new context entry to the store.
    ///
    /// The `fact` is sanitized before insertion. Empty facts (after
    /// sanitization) are silently discarded. When the store is at capacity
    /// the oldest entry is evicted first.
    pub fn add(&mut self, fact: String, category: SessionContextCategory) {
        let fact = sanitize_fact(&fact);
        if fact.is_empty() {
            return;
        }
        if self.entries.len() >= self.limit {
            self.entries.pop_front();
        }
        tracing::debug!("Inserting context memory for {category:?}: {fact}");
        self.entries.push_back(SessionContextEntry {
            fact,
            category,
            created_at: Utc::now(),
        });
    }

    /// Builds the session context portion of the system prompt.
    ///
    /// Returns an empty string when the store contains no entries.
    pub fn build_preamble(&self) -> String {
        if self.entries.is_empty() {
            return String::new();
        }

        let mut preamble = SESSION_CONTEXT_SYSTEM_PROMPT.to_string();
        preamble.push_str("\n\n## Current Session Context\n\n");
        preamble.push_str(
            "The following facts were discovered during this session. \
             Use them to avoid re-running commands unnecessarily.\n\n",
        );
        for entry in &self.entries {
            preamble.push_str(&format!("- [{}] {}\n", entry.category, entry.fact));
        }
        preamble
    }
}

/// Sanitizes user-supplied fact text before storage.
///
/// Collapses consecutive whitespace and newlines into a single space, trims
/// leading/trailing whitespace, and strips leading dashes to prevent markdown
/// list injection.
fn sanitize_fact(raw: &str) -> String {
    raw.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches('-')
        .trim()
        .to_string()
}

/// Base system prompt instructing the LLM about session context usage.
pub const SESSION_CONTEXT_SYSTEM_PROMPT: &str = "You have access to a `save_session_context` tool \
                                                 that stores facts discovered during this session. \
                                                 Use it to avoid re-running commands unnecessarily.";

/// Arguments the LLM supplies when invoking `save_session_context`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SaveSessionContextArgs {
    /// A clear, self-contained statement of the discovered fact.
    pub fact: String,
    /// Classification of the session context entry.
    pub category: SessionContextCategory,
}

/// Result returned to the LLM after a save attempt.
#[derive(Debug, Serialize)]
pub struct SaveSessionContextResult {
    /// Whether the context entry was saved successfully.
    pub saved: bool,
    /// Human-readable status message.
    pub message: String,
}

/// Error returned by [`SaveSessionContextTool::call`].
#[derive(Debug, thiserror::Error)]
pub enum SessionContextToolError {
    /// The store reference was not initialized.
    #[error("session context tool not initialized")]
    NotInitialized,
}

/// Rig [`Tool`] that lets the LLM persist session-scoped context facts.
///
/// On each invocation the tool sanitizes the fact text and stores it in the
/// in-memory [`SessionContextStore`]. Unlike [`super::persistent::SaveMemoryTool`],
/// these entries do **not** persist across sessions.
#[derive(Serialize, Deserialize)]
pub struct SaveSessionContextTool {
    #[serde(skip)]
    store: Option<Arc<RwLock<SessionContextStore>>>,
}

impl SaveSessionContextTool {
    /// Creates a new tool wired to the shared `store`.
    pub fn new(store: Arc<RwLock<SessionContextStore>>) -> Self {
        Self { store: Some(store) }
    }
}

impl Tool for SaveSessionContextTool {
    const NAME: &'static str = "save_session_context";

    type Error = SessionContextToolError;
    type Args = SaveSessionContextArgs;
    type Output = SaveSessionContextResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::from_value(json!({
            "name": "save_session_context",
            "description": concat!(
                "Saves a fact discovered during the current session so you can ",
                "avoid re-running the same diagnostic or exploratory command.\n\n",
                "### WHEN TO USE\n",
                "- After running a command that reveals host info (OS, kernel, arch)\n",
                "- After discovering environment details (shell, locale, PATH)\n",
                "- After checking service state (\"nginx is running on port 80\")\n",
                "- After exploring the filesystem or project structure\n\n",
                "### NEVER USE FOR\n",
                "- Long-lived user preferences (use `save_memory` instead)\n",
                "- Transient command output that changes frequently\n",
                "- Information already present in the session context"
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "fact": {
                        "type": "string",
                        "description": "A clear, self-contained statement of the discovered fact."
                    },
                    "category": {
                        "type": "string",
                        "enum": [
                            "host_info",
                            "environment",
                            "working_directory",
                            "service_state",
                            "discovery"
                        ],
                        "description": "Category of the session context entry."
                    }
                },
                "required": ["fact", "category"]
            }
        }))
        .expect("valid save_session_context tool definition")
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let fact = sanitize_fact(&args.fact);

        if fact.is_empty() {
            return Ok(SaveSessionContextResult {
                saved: false,
                message: "Empty fact provided — nothing saved.".into(),
            });
        }

        let store = self
            .store
            .as_ref()
            .ok_or(SessionContextToolError::NotInitialized)?;

        store.write().await.add(fact.clone(), args.category);

        Ok(SaveSessionContextResult {
            saved: true,
            message: format!("Session context saved: {fact}"),
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- sanitize_fact -------------------------------------------------------

    #[test]
    fn sanitize_fact_collapses_whitespace() {
        assert_eq!(sanitize_fact("  hello   world  "), "hello world");
    }

    #[test]
    fn sanitize_fact_strips_leading_dash() {
        assert_eq!(sanitize_fact("- remember this"), "remember this");
    }

    #[test]
    fn sanitize_fact_returns_empty_for_blank() {
        assert_eq!(sanitize_fact("   "), "");
    }

    // -- SessionContextCategory Display --------------------------------------

    #[test]
    fn category_display() {
        assert_eq!(SessionContextCategory::HostInfo.to_string(), "host_info");
        assert_eq!(
            SessionContextCategory::Environment.to_string(),
            "environment"
        );
        assert_eq!(
            SessionContextCategory::WorkingDirectory.to_string(),
            "working_directory"
        );
        assert_eq!(
            SessionContextCategory::ServiceState.to_string(),
            "service_state"
        );
        assert_eq!(SessionContextCategory::Discovery.to_string(), "discovery");
    }

    // -- SessionContextStore -------------------------------------------------

    #[test]
    fn new_store_is_empty() {
        let store = SessionContextStore::new(10);
        assert!(store.entries.is_empty());
    }

    #[test]
    fn add_inserts_entry() {
        let mut store = SessionContextStore::new(10);
        store.add("Linux x86_64".into(), SessionContextCategory::HostInfo);
        assert_eq!(store.entries.len(), 1);
        assert_eq!(store.entries[0].fact, "Linux x86_64");
        assert_eq!(store.entries[0].category, SessionContextCategory::HostInfo);
    }

    #[test]
    fn add_sanitizes_fact() {
        let mut store = SessionContextStore::new(10);
        store.add(
            "  multiple   spaces  ".into(),
            SessionContextCategory::Discovery,
        );
        assert_eq!(store.entries[0].fact, "multiple spaces");
    }

    #[test]
    fn add_evicts_oldest_at_limit() {
        let mut store = SessionContextStore::new(2);
        store.add("first".into(), SessionContextCategory::HostInfo);
        store.add("second".into(), SessionContextCategory::HostInfo);
        store.add("third".into(), SessionContextCategory::HostInfo);

        assert_eq!(store.entries.len(), 2);
        assert_eq!(store.entries[0].fact, "second");
        assert_eq!(store.entries[1].fact, "third");
    }

    #[test]
    fn add_skips_empty_fact() {
        let mut store = SessionContextStore::new(10);
        store.add("   ".into(), SessionContextCategory::Discovery);
        assert!(store.entries.is_empty());
    }

    #[test]
    fn build_preamble_empty_store() {
        let store = SessionContextStore::new(10);
        assert_eq!(store.build_preamble(), "");
    }

    #[test]
    fn build_preamble_with_entries() {
        let mut store = SessionContextStore::new(10);
        store.add("Linux x86_64".into(), SessionContextCategory::HostInfo);
        store.add(
            "/home/user".into(),
            SessionContextCategory::WorkingDirectory,
        );

        let preamble = store.build_preamble();
        assert!(preamble.contains("Current Session Context"));
        assert!(preamble.contains("[host_info] Linux x86_64"));
        assert!(preamble.contains("[working_directory] /home/user"));
    }

    // -- SaveSessionContextTool ----------------------------------------------

    #[tokio::test]
    async fn tool_saves_context() {
        let store = Arc::new(RwLock::new(SessionContextStore::new(10)));
        let tool = SaveSessionContextTool::new(Arc::clone(&store));

        let result = tool
            .call(SaveSessionContextArgs {
                fact: "Linux x86_64".into(),
                category: SessionContextCategory::HostInfo,
            })
            .await
            .unwrap();

        assert!(result.saved);
        assert!(result.message.contains("Linux x86_64"));

        let guard = store.read().await;
        assert_eq!(guard.entries.len(), 1);
        assert_eq!(guard.entries[0].fact, "Linux x86_64");
    }

    #[tokio::test]
    async fn tool_rejects_empty_fact() {
        let store = Arc::new(RwLock::new(SessionContextStore::new(10)));
        let tool = SaveSessionContextTool::new(Arc::clone(&store));

        let result = tool
            .call(SaveSessionContextArgs {
                fact: "   ".into(),
                category: SessionContextCategory::Discovery,
            })
            .await
            .unwrap();

        assert!(!result.saved);

        let guard = store.read().await;
        assert!(guard.entries.is_empty());
    }

    #[tokio::test]
    async fn tool_definition_has_correct_name() {
        let store = Arc::new(RwLock::new(SessionContextStore::new(10)));
        let tool = SaveSessionContextTool::new(store);

        let def = tool.definition(String::new()).await;
        assert_eq!(def.name, "save_session_context");
    }
}
