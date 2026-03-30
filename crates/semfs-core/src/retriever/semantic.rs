use crate::error::{CoreError, Result};
use semfs_embed::Embedder;
use semfs_storage::LanceStore;
use std::sync::Arc;

pub struct SemanticRetriever {
    embedder: Arc<dyn Embedder>,
    vector_store: Arc<LanceStore>,
}

impl SemanticRetriever {
    pub fn new(embedder: Arc<dyn Embedder>, vector_store: Arc<LanceStore>) -> Self {
        Self { embedder, vector_store }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f32)>> {
        if query.trim().is_empty() || self.embedder.dimensions() == 0 {
            return Ok(Vec::new());
        }

        let query_vector = self.embedder.embed_text(query)
            .map_err(|e| CoreError::Embedding(e.to_string()))?;

        let results = self.vector_store.search(&query_vector, limit)
            .map_err(CoreError::Storage)?;

        Ok(results)
    }

    pub fn is_available(&self) -> bool {
        self.embedder.dimensions() > 0
    }
}
