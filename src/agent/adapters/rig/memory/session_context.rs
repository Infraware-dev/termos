//! Session-scoped context memory for the Rig LLM engine.
//!
//! Unlike persistent [`super::session::MemoryStore`] memories, session context
//! entries are ephemeral and live only for the duration of a single terminal
//! session. The LLM uses [`SaveSessionContextTool`] to cache facts discovered
//! during command execution (e.g. OS type, running services) so it can avoid
//! re-running diagnostic commands.

use std::collections::VecDeque;
use std::fmt;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Maximum number of session context entries kept by default.
pub const DEFAULT_SESSION_CONTEXT_LIMIT: usize = 50;

/// Categorizes a session context entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
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
}
