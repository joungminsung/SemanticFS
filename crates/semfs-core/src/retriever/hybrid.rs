use super::keyword::KeywordRetriever;
use super::rrf::reciprocal_rank_fusion;
use super::semantic::SemanticRetriever;
use crate::error::Result;
use crate::query::types::ParsedQuery;
use semfs_storage::{SearchResult, SqliteStore, MetadataFilter};
use std::sync::Arc;
use tracing::debug;

pub struct HybridRetriever {
    keyword: KeywordRetriever,
    semantic: SemanticRetriever,
    metadata_store: Arc<SqliteStore>,
    #[allow(dead_code)]
    alpha: f32,
    rrf_k: f32,
}

impl HybridRetriever {
    pub fn new(
        keyword: KeywordRetriever,
        semantic: SemanticRetriever,
        metadata_store: Arc<SqliteStore>,
        alpha: f32,
    ) -> Self {
        Self {
            keyword,
            semantic,
            metadata_store,
            alpha,
            rrf_k: 60.0,
        }
    }

    pub fn search(&self, query: &ParsedQuery, top_k: usize) -> Result<Vec<SearchResult>> {
        let search_limit = top_k * 3; // Over-fetch for re-ranking

        // 1. Apply metadata filters to narrow candidates
        let metadata_filters: Vec<MetadataFilter> = query.filters.iter()
            .map(|f| f.to_metadata_filter())
            .collect();

        let filtered_ids = if !metadata_filters.is_empty() {
            Some(self.metadata_store.filter_by(&metadata_filters)?)
        } else {
            None
        };

        // 2. Run keyword search
        let keyword_results = self.keyword.search(&query.semantic_query, search_limit)?;

        // 3. Run semantic search (if embedder available)
        let semantic_results = if self.semantic.is_available() {
            self.semantic.search(&query.semantic_query, search_limit)?
        } else {
            Vec::new()
        };

        debug!(
            keyword_count = keyword_results.len(),
            semantic_count = semantic_results.len(),
            "Search results before fusion"
        );

        // 4. RRF fusion
        let mut fused = if semantic_results.is_empty() {
            keyword_results
        } else {
            reciprocal_rank_fusion(
                &[semantic_results, keyword_results],
                self.rrf_k,
            )
        };

        // 5. Filter by metadata results if applicable
        if let Some(ref valid_ids) = filtered_ids {
            fused.retain(|(id, _)| valid_ids.contains(id));
        }

        // 6. Take top_k and resolve to SearchResult
        let results: Vec<SearchResult> = fused.into_iter()
            .take(top_k)
            .filter_map(|(file_id, score)| {
                match self.metadata_store.get_file(file_id) {
                    Ok(meta) => Some(SearchResult {
                        file_id,
                        path: meta.path,
                        name: meta.name,
                        score,
                        matched_chunks: Vec::new(),
                    }),
                    Err(_) => None,
                }
            })
            .collect();

        debug!(result_count = results.len(), "Final search results");
        Ok(results)
    }
}
