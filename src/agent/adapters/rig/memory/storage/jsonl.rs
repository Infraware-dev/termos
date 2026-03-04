//! JSONL-based storage backend for Phase 1
//!
//! Stores interaction records as line-delimited JSON. Search uses
//! word-overlap text similarity (no vector embeddings).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tokio::fs;
use tokio::io::AsyncWriteExt;

use crate::agent::adapters::rig::memory::models::{InteractionRecord, SearchResult};
use crate::agent::adapters::rig::memory::traits::MemoryStorage;

/// Maximum number of records to keep before rotation
const MAX_RECORDS: usize = 200;

/// Storage backend using append-only JSONL files
#[derive(Debug, Clone)]
pub struct JsonlStorage {
    path: PathBuf,
}

impl JsonlStorage {
    /// Create a new JSONL storage, creating the data directory if needed
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir)
            .with_context(|| format!("Failed to create data dir: {}", data_dir.display()))?;
        Ok(Self {
            path: data_dir.join("interactions.jsonl"),
        })
    }

    /// Rotate file if it exceeds MAX_RECORDS, keeping the most recent entries.
    /// Best-effort: failures are logged but don't propagate.
    async fn rotate_if_needed(&self) {
        let records = match self.read_all().await {
            Ok(r) => r,
            Err(_) => return,
        };

        if records.len() <= MAX_RECORDS {
            return;
        }

        // Keep only the most recent MAX_RECORDS
        let keep = &records[records.len() - MAX_RECORDS..];
        let mut content = String::new();
        for record in keep {
            if let Ok(line) = serde_json::to_string(record) {
                content.push_str(&line);
                content.push('\n');
            }
        }

        if let Err(e) = fs::write(&self.path, content.as_bytes()).await {
            tracing::warn!(error = ?e, "Failed to rotate memory file");
        } else {
            tracing::info!(
                before = records.len(),
                after = MAX_RECORDS,
                "Memory rotated: removed {} old interactions",
                records.len() - MAX_RECORDS
            );
        }
    }

    /// Read all records from the JSONL file
    async fn read_all(&self) -> Result<Vec<InteractionRecord>> {
        let content = match fs::read_to_string(&self.path).await {
            Ok(c) => c,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => {
                return Err(
                    anyhow::anyhow!(e).context(format!("Failed to read {}", self.path.display()))
                );
            }
        };

        let records: Vec<InteractionRecord> = content
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| match serde_json::from_str(line) {
                Ok(record) => Some(record),
                Err(e) => {
                    tracing::warn!(error = ?e, line = %line, "Skipping malformed JSONL line");
                    None
                }
            })
            .collect();

        Ok(records)
    }
}

/// Compute text similarity between a query and a record's intent + input.
/// Returns a score between 0.0 and 1.0.
fn text_similarity(query: &str, record: &InteractionRecord) -> f32 {
    let query_lower = query.to_lowercase();
    let query_words: HashSet<&str> = query_lower.split_whitespace().collect();

    if query_words.is_empty() {
        return 0.0;
    }

    // Combine intent, input, and output for matching
    let output_str = record.output.as_deref().unwrap_or("");
    let target = format!("{} {} {}", record.intent, record.input, output_str).to_lowercase();
    let target_words: HashSet<&str> = target.split_whitespace().collect();

    if target_words.is_empty() {
        return 0.0;
    }

    // Word overlap score (Jaccard-like)
    let intersection = query_words.intersection(&target_words).count();
    let union = query_words.union(&target_words).count();
    let mut score = if union > 0 {
        intersection as f32 / union as f32
    } else {
        0.0
    };

    // Exact substring bonus
    if target.contains(&query_lower) {
        score = (score + 0.3).min(1.0);
    }

    score
}

impl MemoryStorage for JsonlStorage {
    async fn append(&self, record: &InteractionRecord) -> Result<()> {
        let mut line = serde_json::to_string(record)?;
        line.push('\n');

        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .await
            .with_context(|| format!("Failed to open {} for append", self.path.display()))?;

        file.write_all(line.as_bytes()).await?;
        file.flush().await?;

        // Rotate if over limit: keep only the most recent records
        self.rotate_if_needed().await;

        Ok(())
    }

    async fn get_by_id(&self, id: &str) -> Result<Option<InteractionRecord>> {
        let records = self.read_all().await?;
        Ok(records.into_iter().find(|r| r.id == id))
    }

    async fn search(
        &self,
        query: &str,
        top_k: usize,
        working_dir: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        let records = self.read_all().await?;

        let mut results: Vec<SearchResult> = records
            .into_iter()
            .filter(|r| {
                working_dir
                    .map(|wd| r.context.working_dir.as_deref() == Some(wd))
                    .unwrap_or(true)
            })
            .map(|r| {
                let similarity = text_similarity(query, &r);
                SearchResult {
                    record: r,
                    similarity,
                }
            })
            .filter(|r| r.similarity > 0.0)
            .collect();

        results.sort_by(|a, b| {
            b.similarity
                .partial_cmp(&a.similarity)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(top_k);

        Ok(results)
    }

    async fn list_recent(&self, limit: usize) -> Result<Vec<InteractionRecord>> {
        let mut records = self.read_all().await?;
        records.reverse();
        records.truncate(limit);
        Ok(records)
    }

    async fn count(&self) -> Result<usize> {
        let records = self.read_all().await?;
        Ok(records.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::adapters::rig::memory::models::DataType;

    fn make_record(intent: &str, input: &str, working_dir: Option<&str>) -> InteractionRecord {
        InteractionRecord::new(
            DataType::Command,
            intent.to_string(),
            input.to_string(),
            false,
            working_dir.map(String::from),
        )
    }

    #[tokio::test]
    async fn test_append_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        let record = make_record("executed ls", "ls -la", None);
        storage.append(&record).await.unwrap();

        let count = storage.count().await.unwrap();
        assert_eq!(count, 1);

        let found = storage.get_by_id(&record.id).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().input, "ls -la");
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        let found = storage.get_by_id("nonexistent").await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_search_finds_similar() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        storage
            .append(&make_record(
                "executed docker-compose up",
                "docker-compose up -d",
                None,
            ))
            .await
            .unwrap();
        storage
            .append(&make_record(
                "executed pip install",
                "pip install flask",
                None,
            ))
            .await
            .unwrap();
        storage
            .append(&make_record("executed git status", "git status", None))
            .await
            .unwrap();

        let results = storage.search("docker", 2, None).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].record.input.contains("docker"));
    }

    #[tokio::test]
    async fn test_search_working_dir_filter() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        storage
            .append(&make_record(
                "executed npm install",
                "npm install",
                Some("/home/user/web"),
            ))
            .await
            .unwrap();
        storage
            .append(&make_record(
                "executed cargo build",
                "cargo build",
                Some("/home/user/rust"),
            ))
            .await
            .unwrap();

        let results = storage
            .search("install", 5, Some("/home/user/web"))
            .await
            .unwrap();

        // Should only find the npm result
        for r in &results {
            assert_eq!(
                r.record.context.working_dir.as_deref(),
                Some("/home/user/web")
            );
        }
    }

    #[tokio::test]
    async fn test_list_recent() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        for i in 0..5 {
            storage
                .append(&make_record(
                    &format!("cmd {}", i),
                    &format!("cmd-{}", i),
                    None,
                ))
                .await
                .unwrap();
        }

        let recent = storage.list_recent(3).await.unwrap();
        assert_eq!(recent.len(), 3);
        // Most recent first
        assert_eq!(recent[0].input, "cmd-4");
    }

    #[tokio::test]
    async fn test_empty_file() {
        let dir = tempfile::tempdir().unwrap();
        let storage = JsonlStorage::new(dir.path()).unwrap();

        assert_eq!(storage.count().await.unwrap(), 0);
        assert!(
            storage
                .search("anything", 5, None)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(storage.list_recent(10).await.unwrap().is_empty());
    }

    #[test]
    fn test_text_similarity() {
        let record = make_record("executed docker-compose up", "docker-compose up -d", None);

        let score = text_similarity("docker", &record);
        assert!(score > 0.0);

        let score_exact = text_similarity("docker-compose up", &record);
        assert!(score_exact > score);

        let score_unrelated = text_similarity("python flask", &record);
        assert_eq!(score_unrelated, 0.0);
    }
}
