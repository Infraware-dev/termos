//! Contextual memory system for the RigEngine
//!
//! Stores and retrieves past interactions (commands + NL queries) to provide
//! context-aware assistance. Uses a strategy pattern with generics for
//! compile-time swappable backends.

pub mod embeddings;
pub mod intent;
pub mod models;
pub mod session;
pub mod storage;
pub mod traits;

use std::path::PathBuf;

use anyhow::Result;
use embeddings::NoopEmbedding;
use intent::RegexIntentGenerator;
use models::{DataType, InteractionRecord, SearchResult};
use storage::JsonlStorage;
use traits::{EmbeddingEngine, IntentGenerator, MemoryStorage};

use super::config::MemoryConfig;

/// Composable memory store with pluggable storage, embedding, and intent backends
#[derive(Debug)]
pub struct MemoryStore<S, E, I>
where
    S: MemoryStorage,
    E: EmbeddingEngine,
    I: IntentGenerator,
{
    storage: S,
    embeddings: E,
    intent_gen: I,
    data_dir: PathBuf,
}

/// Phase 1 active memory type: JSONL storage, no embeddings, regex intent
pub type ActiveMemory = MemoryStore<JsonlStorage, NoopEmbedding, RegexIntentGenerator>;

impl ActiveMemory {
    /// Create a new Phase 1 memory store from configuration
    pub fn new(config: &MemoryConfig) -> Result<Self> {
        let storage = JsonlStorage::new(&config.path)?;
        let embeddings = NoopEmbedding;
        let intent_gen = RegexIntentGenerator::new();

        Ok(Self {
            storage,
            embeddings,
            intent_gen,
            data_dir: config.path.clone(),
        })
    }
}

impl<S, E, I> MemoryStore<S, E, I>
where
    S: MemoryStorage,
    E: EmbeddingEngine,
    I: IntentGenerator,
{
    /// Add an interaction record to storage
    pub async fn add(&self, record: &InteractionRecord) -> Result<()> {
        self.storage.append(record).await
    }

    /// Search for similar past interactions
    pub async fn search(
        &self,
        query: &str,
        top_k: usize,
        working_dir: Option<&str>,
    ) -> Result<Vec<SearchResult>> {
        self.storage.search(query, top_k, working_dir).await
    }

    /// Generate a semantic intent for the given input
    pub async fn generate_intent(&self, input: &str, data_type: DataType) -> Result<String> {
        self.intent_gen.generate(input, data_type).await
    }

    /// Count total stored interactions
    pub async fn count(&self) -> Result<usize> {
        self.storage.count().await
    }

    /// List recent interactions
    pub async fn list_recent(&self, limit: usize) -> Result<Vec<InteractionRecord>> {
        self.storage.list_recent(limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config(dir: &std::path::Path) -> MemoryConfig {
        MemoryConfig {
            path: dir.to_path_buf(),
            limit: 100,
        }
    }

    #[tokio::test]
    async fn test_active_memory_creation() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let memory = ActiveMemory::new(&config);
        assert!(memory.is_ok());
    }

    #[tokio::test]
    async fn test_add_and_search() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let memory = ActiveMemory::new(&config).unwrap();

        let record = InteractionRecord::new(
            DataType::Command,
            "executed docker-compose up".to_string(),
            "docker-compose up -d".to_string(),
            false,
            None,
        );

        memory.add(&record).await.unwrap();
        assert_eq!(memory.count().await.unwrap(), 1);

        let results = memory.search("docker", 5, None).await.unwrap();
        assert!(!results.is_empty());
        assert!(results[0].record.input.contains("docker"));
    }

    #[tokio::test]
    async fn test_generate_intent() {
        let dir = tempfile::tempdir().unwrap();
        let config = test_config(dir.path());
        let memory = ActiveMemory::new(&config).unwrap();

        let intent = memory
            .generate_intent("sudo apt-get install nginx", DataType::Command)
            .await
            .unwrap();
        assert_eq!(intent, "executed apt-get install nginx");

        let intent = memory
            .generate_intent("how do I install redis", DataType::NaturalLanguage)
            .await
            .unwrap();
        assert_eq!(intent, "install redis");
    }
}
