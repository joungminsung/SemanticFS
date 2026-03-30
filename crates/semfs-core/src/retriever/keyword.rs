use crate::error::Result;
use semfs_storage::SqliteStore;
use std::sync::Arc;

pub struct KeywordRetriever {
    store: Arc<SqliteStore>,
}

impl KeywordRetriever {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }

    pub fn search(&self, query: &str, limit: usize) -> Result<Vec<(i64, f32)>> {
        if query.trim().is_empty() {
            return Ok(Vec::new());
        }

        let safe_query = format!("\"{}\"", query.replace('"', "\"\""));
        let results = self.store.search_fts(&safe_query)
            .map_err(crate::error::CoreError::Storage)?;

        // Normalize FTS5 scores (they're negative, lower = better match)
        let results: Vec<(i64, f32)> = results.into_iter()
            .take(limit)
            .map(|(id, score)| {
                // FTS5 rank is negative, convert to positive score
                (id, (-score) as f32)
            })
            .collect();

        Ok(results)
    }
}
