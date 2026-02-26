//! Core trait definitions for the memory system
//!
//! Uses native Rust 2024 async trait syntax with explicit `Send` bounds
//! on returned futures (required for tokio multi-threaded runtime).

use anyhow::Result;

use super::models::{DataType, InteractionRecord, SearchResult};

/// Storage backend for interaction records
#[expect(
    dead_code,
    reason = "Phase 2 memory infrastructure - used in tests and future phases"
)]
pub trait MemoryStorage: Send + Sync {
    /// Append an interaction record to storage
    fn append(&self, record: &InteractionRecord) -> impl Future<Output = Result<()>> + Send;

    /// Retrieve a record by its ID
    fn get_by_id(&self, id: &str)
    -> impl Future<Output = Result<Option<InteractionRecord>>> + Send;

    /// Search for similar interactions
    fn search(
        &self,
        query: &str,
        top_k: usize,
        working_dir: Option<&str>,
    ) -> impl Future<Output = Result<Vec<SearchResult>>> + Send;

    /// List recent interactions in reverse chronological order
    fn list_recent(
        &self,
        limit: usize,
    ) -> impl Future<Output = Result<Vec<InteractionRecord>>> + Send;

    /// Count total stored interactions
    fn count(&self) -> impl Future<Output = Result<usize>> + Send;
}

/// Embedding engine for generating vector representations
#[expect(
    dead_code,
    reason = "Phase 2 memory infrastructure - used in tests and future phases"
)]
pub trait EmbeddingEngine: Send + Sync {
    /// Generate an embedding vector for the given text
    fn embed(&self, text: &str) -> Result<Vec<f32>>;

    /// Return the dimensionality of the embeddings
    fn dimension(&self) -> usize;
}

/// Generator for semantic intent descriptions
pub trait IntentGenerator: Send + Sync {
    /// Generate a semantic intent from user input
    fn generate(
        &self,
        input: &str,
        data_type: DataType,
    ) -> impl Future<Output = Result<String>> + Send;
}
