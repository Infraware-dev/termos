//! Context and persistent memory for Rig LLM engine.
//!
//! Memories are persisted to a JSON file and loaded into the system prompt
//! on each request. The LLM decides **when** to invoke [`SaveMemoryTool`]
//! based on the tool description and system-prompt guidelines — no manual
//! pattern matching is required on the application side.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::{fmt, fs};

use anyhow::Context;
use chrono::{DateTime, Utc};
use rig::completion::ToolDefinition;
use rig::tool::Tool;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tokio::sync::RwLock;

/// Categorizes a persisted memory entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// User preference (e.g. "tabs over spaces").
    Preference,
    /// Personal fact (e.g. "my name is Alice").
    PersonalFact,
    /// Workflow convention (e.g. "always run clippy before commit").
    Workflow,
    /// Explicit restriction (e.g. "never push to main").
    Restriction,
    /// Team or project convention (e.g. "we use conventional commits").
    Convention,
}

impl fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Preference => write!(f, "preference"),
            Self::PersonalFact => write!(f, "personal_fact"),
            Self::Workflow => write!(f, "workflow"),
            Self::Restriction => write!(f, "restriction"),
            Self::Convention => write!(f, "convention"),
        }
    }
}

/// A single persisted memory entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MemoryEntry {
    /// Human-readable fact statement.
    pub fact: String,
    /// Classification of the memory.
    pub category: MemoryCategory,
    /// When the memory was created.
    pub created_at: DateTime<Utc>,
}

/// JSON-file-backed persistent store for [`MemoryEntry`] items.
///
/// All memories are loaded into the system prompt on each request.
#[derive(Debug)]
pub struct MemoryStore {
    /// Path where to store the memory entries
    path: PathBuf,
    /// The memory entries
    entries: VecDeque<MemoryEntry>,
    /// Maximum entries to keep in the store
    limit: usize,
}

impl MemoryStore {
    /// Opens an existing memory file or creates an empty one.
    ///
    /// # Errors
    ///
    /// Returns an error when the file exists but cannot be read or parsed.
    pub fn load_or_create(path: impl AsRef<Path>, limit: usize) -> anyhow::Result<Self> {
        let path = path.as_ref().to_path_buf();
        tracing::debug!("Loading memory store from path: {}", path.display());

        let entries = if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str(&data)?
        } else {
            tracing::debug!("Memory store is empty or unexisting");
            Vec::new()
        };
        let entries = VecDeque::from(entries);
        tracing::debug!("Loaded memory store with {} entries", entries.len());

        Ok(Self {
            path,
            entries,
            limit,
        })
    }

    /// Adds a new memory and persists to disk.
    ///
    /// If the count of `entries` is equal (or bigger) than `limit`, the oldest item is popped,
    /// before inserting a new one.
    ///
    /// # Errors
    ///
    /// Returns an error when the file cannot be written.
    pub fn add(&mut self, fact: String, category: MemoryCategory) -> anyhow::Result<&MemoryEntry> {
        let entry = MemoryEntry {
            fact,
            category,
            created_at: Utc::now(),
        };
        if self.entries.len() >= self.limit {
            self.entries.pop_front();
        }
        tracing::debug!(
            "Adding memory entry: {:?} (count: {count})",
            entry,
            count = self.entries.len() + 1
        );
        self.entries.push_back(entry);
        self.flush()?;
        Ok(self.entries.back().expect("cannot be empty after add"))
    }

    /// Serializes the current entries to the backing JSON file.
    fn flush(&self) -> anyhow::Result<()> {
        // create parent directory if it doesn't exist
        if let Some(parent) = self.path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory {}", parent.display()))?;
        }

        let json = serde_json::to_string_pretty(&self.entries)?;
        fs::write(&self.path, json)?;
        tracing::debug!("Written memory entry to disk");
        Ok(())
    }

    /// Builds the full system prompt, including all persisted memories.
    pub fn build_preamble(&self) -> String {
        let mut preamble = MEMORY_SYSTEM_PROMPT.to_string();

        if !self.entries.is_empty() {
            preamble.push_str("\n\n## Known Context From Memory\n\n");
            for entry in &self.entries {
                preamble.push_str(&format!("- [{}] {}\n", entry.category, entry.fact));
            }
        }

        preamble
    }
}

/// Arguments the LLM supplies when invoking `save_memory`.
#[derive(Debug, Deserialize, Serialize, JsonSchema)]
pub struct SaveMemoryArgs {
    /// A clear, self-contained statement of the fact to remember.
    pub fact: String,
    /// Classification of the memory.
    pub category: MemoryCategory,
}

/// Result returned to the LLM after a save attempt.
#[derive(Debug, Serialize)]
pub struct SaveMemoryResult {
    /// Whether the memory was persisted successfully.
    pub saved: bool,
    /// Human-readable status message.
    pub message: String,
}

/// Error returned by [`SaveMemoryTool::call`].
#[derive(Debug, thiserror::Error)]
pub enum MemoryToolError {
    /// The store reference was not initialized.
    #[error("memory tool not initialized")]
    NotInitialized,
    /// Persistence failed.
    #[error("storage error: {0}")]
    Storage(String),
}

/// Rig [`Tool`] that lets the LLM persist cross-session user context.
///
/// On each invocation the tool sanitizes the fact text and persists it to
/// the [`MemoryStore`] JSON file.
#[derive(Serialize, Deserialize)]
pub struct SaveMemoryTool {
    #[serde(skip)]
    store: Option<Arc<RwLock<MemoryStore>>>,
}

impl SaveMemoryTool {
    /// Creates a new tool wired to the shared `store`.
    pub fn new(store: Arc<RwLock<MemoryStore>>) -> Self {
        Self { store: Some(store) }
    }
}

/// Sanitizes user-supplied fact text before persistence.
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

impl Tool for SaveMemoryTool {
    const NAME: &'static str = "save_memory";

    type Error = MemoryToolError;
    type Args = SaveMemoryArgs;
    type Output = SaveMemoryResult;

    async fn definition(&self, _prompt: String) -> ToolDefinition {
        serde_json::from_value(json!({
            "name": "save_memory",
            "description": concat!(
                "Saves important user context (preferences, facts, conventions) ",
                "for use across ALL sessions.\n\n",
                "### WHEN TO USE\n",
                "- User explicitly says \"remember X\" or \"always do X\"\n",
                "- User states a personal fact or team information\n",
                "- User establishes a workflow convention ",
                "(\"we use pnpm\", \"always run tests before commit\")\n",
                "- User sets a restriction (\"never push to main directly\")\n",
                "- User defines a coding style preference\n\n",
                "### NEVER USE FOR\n",
                "- Workspace-specific file paths or project structure\n",
                "- Transient conversation (\"I'll be back in 5 min\")\n",
                "- Summaries of code changes, bug fixes, or task progress\n",
                "- Information that only applies to the current session\n\n",
                "If unsure, ask the user: \"Should I remember that for future sessions?\""
            ),
            "parameters": {
                "type": "object",
                "properties": {
                    "fact": {
                        "type": "string",
                        "description": "A clear, self-contained statement of the fact to remember."
                    },
                    "category": {
                        "type": "string",
                        "enum": [
                            "preference",
                            "personal_fact",
                            "workflow",
                            "restriction",
                            "convention"
                        ],
                        "description": "Category of the memory."
                    }
                },
                "required": ["fact", "category"]
            }
        }))
        .expect("valid save_memory tool definition")
    }

    async fn call(&self, args: Self::Args) -> Result<Self::Output, Self::Error> {
        let fact = sanitize_fact(&args.fact);

        if fact.is_empty() {
            return Ok(SaveMemoryResult {
                saved: false,
                message: "Empty fact provided — nothing saved.".into(),
            });
        }

        let store = self.store.as_ref().ok_or(MemoryToolError::NotInitialized)?;

        store
            .write()
            .await
            .add(fact.clone(), args.category)
            .map_err(|e| MemoryToolError::Storage(e.to_string()))?;

        Ok(SaveMemoryResult {
            saved: true,
            message: format!("Remembered: {fact}"),
        })
    }
}

/// Base system prompt that instructs the LLM about memory usage.
pub const MEMORY_SYSTEM_PROMPT: &str = "\
You are a DevOps AI assistant.

## Memory Guidelines

You have access to a `save_memory` tool that persists important user context
across sessions.

**Save when:**
- User explicitly says \"remember X\" or \"always do X\"
- User states a personal fact (name, preferences, team info)
- User establishes a workflow convention (\"we use pnpm\", \"always run tests\")
- User sets a restriction (\"never push to main directly\")
- User defines a coding style preference (\"use 4-space indentation\")

**Never save:**
- Workspace-specific file paths or project structure details
- Transient conversation (\"I'll be back in 5 minutes\")
- Summaries of code changes, bug fixes, or task progress
- Information that only applies to the current session
- Anything you are unsure about — ask the user first

When uncertain, ask: \"Should I remember that for future sessions?\"
";

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::io::Write as _;

    use tempfile::NamedTempFile;

    use super::*;

    const LIMIT: usize = 200;

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
    fn sanitize_fact_returns_empty_for_blank_input() {
        assert_eq!(sanitize_fact("   "), "");
    }

    // -- MemoryStore ---------------------------------------------------------

    #[test]
    fn store_creates_empty_when_file_missing() {
        let path = "/tmp/infraware_test_nonexistent.json";
        let _ = fs::remove_file(path);

        let store = MemoryStore::load_or_create(path, LIMIT).unwrap();
        assert!(store.entries.is_empty());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn store_add_persists_and_reloads() {
        let mut tmp = NamedTempFile::new().unwrap();
        write!(tmp, "[]").unwrap();
        let path = tmp.path().to_path_buf();

        let mut store = MemoryStore::load_or_create(&path, LIMIT).unwrap();
        store
            .add("we use pnpm".into(), MemoryCategory::Convention)
            .unwrap();
        assert_eq!(store.entries.len(), 1);

        // Reload from disk.
        let store2 = MemoryStore::load_or_create(&path, LIMIT).unwrap();
        assert_eq!(store2.entries.len(), 1);
        assert_eq!(store2.entries[0].fact, "we use pnpm");
    }

    // -- build_preamble ------------------------------------------------------

    #[test]
    fn build_preamble_empty_memories() {
        let preamble = test_store().build_preamble();
        assert!(preamble.contains("Memory Guidelines"));
        assert!(!preamble.contains("Known Context"));
    }

    #[test]
    fn build_preamble_with_memories() {
        let entries = vec![
            MemoryEntry {
                fact: "we use pnpm".into(),
                category: MemoryCategory::Convention,
                created_at: Utc::now(),
            },
            MemoryEntry {
                fact: "tabs over spaces".into(),
                category: MemoryCategory::Preference,
                created_at: Utc::now(),
            },
        ];
        let mut store = test_store();
        store.entries = entries.into();

        let preamble = store.build_preamble();
        assert!(preamble.contains("Known Context From Memory"));
        assert!(preamble.contains("[convention] we use pnpm"));
        assert!(preamble.contains("[preference] tabs over spaces"));
    }

    #[test]
    fn test_should_not_insert_more_entries_than_limit() {
        let tmp = NamedTempFile::new().unwrap();
        let mut store = MemoryStore {
            path: tmp.path().to_path_buf(),
            entries: VecDeque::new(),
            limit: 2,
        };

        store
            .add("abc".to_string(), MemoryCategory::Convention)
            .expect("failed to add");
        store
            .add("def".to_string(), MemoryCategory::Convention)
            .expect("failed to add");
        assert_eq!(store.entries.len(), 2);

        store
            .add("xyz".to_string(), MemoryCategory::Convention)
            .expect("failed to add");
        assert_eq!(store.entries.len(), 2);

        assert!(store.entries.iter().any(|x| x.fact == "def"));
        assert!(store.entries.iter().any(|x| x.fact == "xyz"));
    }

    /// Creates a test [`MemoryStore`] with no entries backed by no file.
    ///
    /// Only suitable for tests that do NOT call [`MemoryStore::add`] (which flushes to disk).
    fn test_store() -> MemoryStore {
        MemoryStore {
            path: PathBuf::default(),
            entries: VecDeque::new(),
            limit: usize::MAX,
        }
    }
}
