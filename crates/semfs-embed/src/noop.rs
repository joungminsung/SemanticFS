use crate::traits::Embedder;
use anyhow::Result;
use tracing::debug;

/// No-op embedder for FTS5-only fallback mode.
/// Returns empty vectors — the retriever will use keyword search only.
pub struct NoopEmbedder;

impl NoopEmbedder {
    pub fn new() -> Self {
        Self
    }

    pub fn is_noop(&self) -> bool {
        true
    }
}

impl Default for NoopEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

impl Embedder for NoopEmbedder {
    fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
        debug!("NoopEmbedder: returning empty vector");
        Ok(Vec::new())
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        Ok(vec![Vec::new(); texts.len()])
    }

    fn dimensions(&self) -> usize {
        0
    }

    fn model_name(&self) -> &str {
        "noop"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_noop_embedder() {
        let embedder = NoopEmbedder::new();
        let result = embedder.embed_text("hello").unwrap();
        assert!(result.is_empty());
        assert_eq!(embedder.dimensions(), 0);
        assert_eq!(embedder.model_name(), "noop");
    }

    #[test]
    fn test_noop_batch() {
        let embedder = NoopEmbedder::new();
        let results = embedder.embed_batch(&["a", "b", "c"]).unwrap();
        assert_eq!(results.len(), 3);
        assert!(results.iter().all(|v| v.is_empty()));
    }
}
