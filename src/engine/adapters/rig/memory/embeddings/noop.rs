//! No-op embedding engine for Phase 1 (text-based search only)

use anyhow::Result;

use crate::engine::adapters::rig::memory::traits::EmbeddingEngine;

/// Placeholder embedding engine that produces empty vectors.
/// Phase 1 uses text similarity instead of vector search.
#[derive(Debug, Clone, Default)]
pub struct NoopEmbedding;

impl EmbeddingEngine for NoopEmbedding {
    fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![])
    }

    fn dimension(&self) -> usize {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_returns_empty() {
        let engine = NoopEmbedding;
        let embedding = engine.embed("hello world").unwrap();
        assert!(embedding.is_empty());
        assert_eq!(engine.dimension(), 0);
    }
}
