//! Integration tests for SemanticFS core pipeline
//!
//! Tests the full flow: crawl → chunk → index → search

use semfs_core::indexer::chunker::{CodeChunker, TextChunker, Chunker};
use semfs_core::query::parser::parse_query;
use semfs_core::retriever::rrf::reciprocal_rank_fusion;
use std::path::Path;

// ============================================================
// Query Parser Integration Tests
// ============================================================

#[test]
fn test_parse_korean_english_mixed_query() {
    let q = parse_query("2024년에 작성한 React 프로젝트 중 TypeScript 파일");

    // Should extract year filter
    assert!(!q.filters.is_empty(), "Should have filters");

    // Should preserve semantic terms
    assert!(
        q.semantic_query.contains("React") || q.semantic_query.contains("프로젝트"),
        "Semantic query should contain key terms: got '{}'",
        q.semantic_query
    );
}

#[test]
fn test_parse_simple_english_query() {
    let q = parse_query("API router files");
    assert!(q.semantic_query.contains("API"));
    assert!(q.filters.is_empty());
}

#[test]
fn test_parse_date_filter_variations() {
    // Year format
    let q1 = parse_query("2024년 프로젝트");
    assert!(!q1.filters.is_empty());

    // Recent days
    let q2 = parse_query("최근 7일 수정한 파일");
    assert!(!q2.filters.is_empty());

    // Last month
    let q3 = parse_query("지난달 작업한 코드");
    assert!(!q3.filters.is_empty());
}

#[test]
fn test_parse_extension_filter_korean() {
    let q = parse_query("Python 코드");
    let has_ext_filter = q.filters.iter().any(|f| {
        matches!(f, semfs_core::query::QueryFilter::Extension(exts) if exts.contains(&"py".to_string()))
    });
    assert!(has_ext_filter, "Should detect Python extension filter");
}

// ============================================================
// Chunker Integration Tests
// ============================================================

#[test]
fn test_text_chunker_markdown_hierarchy() {
    let content = r#"# Introduction

This is the introduction.

## Setup

Install the dependencies.

### Prerequisites

You need Rust installed.

## Usage

Run the command.
"#;

    let chunker = TextChunker::new();
    let chunks = chunker.chunk(Path::new("README.md"), content);

    // Should have sections and paragraphs
    assert!(chunks.len() >= 4, "Expected at least 4 chunks, got {}", chunks.len());

    // First chunk should be a Section
    assert_eq!(chunks[0].chunk_type, semfs_storage::ChunkType::Section);
}

#[test]
fn test_code_chunker_rust_functions() {
    let content = r#"use std::io;

pub struct Config {
    port: u16,
    host: String,
}

impl Config {
    pub fn new(port: u16) -> Self {
        Self { port, host: "localhost".to_string() }
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

fn main() {
    let config = Config::new(8080);
    println!("{}", config.address());
}
"#;

    let chunker = CodeChunker::new();
    let chunks = chunker.chunk(Path::new("main.rs"), content);

    // Should detect struct, impl, functions
    assert!(chunks.len() >= 3, "Expected at least 3 chunks, got {}", chunks.len());

    // Should have at least one function chunk
    let has_function = chunks.iter().any(|c| c.chunk_type == semfs_storage::ChunkType::Function);
    assert!(has_function, "Should detect at least one function");
}

#[test]
fn test_code_chunker_python() {
    let content = r#"import os

class Server:
    def __init__(self, port):
        self.port = port

    def start(self):
        print(f"Starting on {self.port}")

def main():
    server = Server(8080)
    server.start()
"#;

    let chunker = CodeChunker::new();
    let chunks = chunker.chunk(Path::new("server.py"), content);
    assert!(chunks.len() >= 2, "Expected at least 2 chunks, got {}", chunks.len());
}

// ============================================================
// RRF Fusion Tests
// ============================================================

#[test]
fn test_rrf_merges_ranked_lists_correctly() {
    // Semantic results
    let semantic = vec![(1, 0.95_f32), (3, 0.80), (5, 0.70)];
    // Keyword results
    let keyword = vec![(3, 0.90_f32), (1, 0.85), (2, 0.60)];

    let fused = reciprocal_rank_fusion(&[semantic, keyword], 60.0);

    // Files 1 and 3 appear in both lists, should rank highest
    let top_ids: Vec<i64> = fused.iter().take(2).map(|(id, _)| *id).collect();
    assert!(top_ids.contains(&1), "File 1 should be in top 2");
    assert!(top_ids.contains(&3), "File 3 should be in top 2");

    // File 2 should also appear
    let all_ids: Vec<i64> = fused.iter().map(|(id, _)| *id).collect();
    assert!(all_ids.contains(&2), "File 2 should appear in results");
}

#[test]
fn test_rrf_handles_disjoint_lists() {
    let list1 = vec![(1, 0.9_f32), (2, 0.8)];
    let list2 = vec![(3, 0.9_f32), (4, 0.8)];

    let fused = reciprocal_rank_fusion(&[list1, list2], 60.0);
    assert_eq!(fused.len(), 4, "Should have all 4 unique files");
}

// ============================================================
// Storage Integration Tests
// ============================================================

#[test]
fn test_sqlite_full_workflow() {
    use semfs_storage::{SqliteStore, FileMeta};
    use std::path::PathBuf;

    let store = SqliteStore::in_memory().unwrap();

    // Insert files
    let meta1 = FileMeta {
        id: None,
        path: PathBuf::from("/project/src/main.rs"),
        name: "main.rs".to_string(),
        extension: Some("rs".to_string()),
        size: 2048,
        hash: "hash1".to_string(),
        created_at: 1700000000,
        modified_at: 1700100000,
        indexed_at: 1700200000,
        mime_type: Some("text/x-rust".to_string()),
    };
    let meta2 = FileMeta {
        id: None,
        path: PathBuf::from("/project/src/lib.rs"),
        name: "lib.rs".to_string(),
        extension: Some("rs".to_string()),
        size: 1024,
        hash: "hash2".to_string(),
        created_at: 1700000000,
        modified_at: 1700050000,
        indexed_at: 1700200000,
        mime_type: Some("text/x-rust".to_string()),
    };
    let meta3 = FileMeta {
        id: None,
        path: PathBuf::from("/project/README.md"),
        name: "README.md".to_string(),
        extension: Some("md".to_string()),
        size: 512,
        hash: "hash3".to_string(),
        created_at: 1700000000,
        modified_at: 1700000000,
        indexed_at: 1700200000,
        mime_type: Some("text/markdown".to_string()),
    };

    let id1 = store.insert_file(&meta1).unwrap();
    let id2 = store.insert_file(&meta2).unwrap();
    let id3 = store.insert_file(&meta3).unwrap();

    // Index content for FTS
    store.index_content(id1, "main.rs", "/project/src/main.rs",
        "fn main() { let server = Server::new(8080); server.start(); }").unwrap();
    store.index_content(id2, "lib.rs", "/project/src/lib.rs",
        "pub mod server; pub mod config; pub mod router;").unwrap();
    store.index_content(id3, "README.md", "/project/README.md",
        "# My Project\nA web server written in Rust.").unwrap();

    // FTS search
    let results = store.search_fts("server").unwrap();
    assert!(results.len() >= 1, "Should find files mentioning 'server'");

    // Filter by extension
    let rs_files = store.filter_by(&[
        semfs_storage::MetadataFilter::Extension(vec!["rs".to_string()])
    ]).unwrap();
    assert_eq!(rs_files.len(), 2, "Should find 2 .rs files");

    // Filter by date range
    let recent = store.filter_by(&[
        semfs_storage::MetadataFilter::DateRange {
            start: 1700080000,
            end: 1700200000,
        }
    ]).unwrap();
    assert!(recent.len() >= 1, "Should find recently modified files");

    // File count
    assert_eq!(store.file_count().unwrap(), 3);

    // Delete
    store.delete_file(id3).unwrap();
    assert_eq!(store.file_count().unwrap(), 2);
}

#[test]
fn test_wal_crash_recovery_simulation() {
    use semfs_storage::{WalStore, FileOperation};
    use std::path::PathBuf;

    let wal = WalStore::in_memory().unwrap();

    // Simulate: log two operations, one gets to "executing"
    let op1 = FileOperation::Move {
        source: PathBuf::from("/a.txt"),
        dest: PathBuf::from("/b.txt"),
    };
    let op2 = FileOperation::Copy {
        source: PathBuf::from("/c.txt"),
        dest: PathBuf::from("/d.txt"),
    };

    let id1 = wal.log_operation(&op1).unwrap();
    let id2 = wal.log_operation(&op2).unwrap();

    // op1 started executing before crash
    wal.mark_executing(id1).unwrap();

    // Simulate crash recovery
    let pending = wal.recover_pending().unwrap();
    assert_eq!(pending.len(), 2, "Should find both pending operations");

    // In real recovery: check filesystem state and retry/rollback
    // Here we just mark them
    wal.mark_completed(id1).unwrap();
    wal.mark_failed(id2).unwrap();

    let still_pending = wal.recover_pending().unwrap();
    assert_eq!(still_pending.len(), 0, "No more pending after recovery");
}

// ============================================================
// Cache Integration Tests
// ============================================================

#[test]
fn test_cache_manager_file_change_invalidation() {
    use semfs_storage::{CacheManager, SearchResult};
    use std::path::PathBuf;

    let cache = CacheManager::new(100, 300, 100);

    // Cache a query result
    let results = vec![
        SearchResult {
            file_id: 1,
            path: PathBuf::from("/a.rs"),
            name: "a.rs".to_string(),
            score: 0.9,
            matched_chunks: vec![],
        },
        SearchResult {
            file_id: 2,
            path: PathBuf::from("/b.rs"),
            name: "b.rs".to_string(),
            score: 0.8,
            matched_chunks: vec![],
        },
    ];

    cache.query_cache.put(42, results, vec![1, 2]);
    assert!(cache.query_cache.get(42).is_some(), "Cache should have entry");

    // File 1 changes → should invalidate this query
    cache.on_file_changed(1, Some("old_hash"));

    assert!(cache.query_cache.get(42).is_none(), "Cache should be invalidated");
    assert!(cache.embedding_cache.get("old_hash").is_none(), "Embedding cache should be cleared");
}

// ============================================================
// Embedder Tests
// ============================================================

#[test]
fn test_noop_embedder_fallback() {
    use semfs_embed::{Embedder, NoopEmbedder};

    let embedder = NoopEmbedder::new();
    assert_eq!(embedder.dimensions(), 0);
    assert_eq!(embedder.model_name(), "noop");

    let result = embedder.embed_text("hello world").unwrap();
    assert!(result.is_empty());

    let batch = embedder.embed_batch(&["a", "b", "c"]).unwrap();
    assert_eq!(batch.len(), 3);
}

// ============================================================
// Property-Based Tests
// ============================================================

#[cfg(test)]
mod property_tests {
    use proptest::prelude::*;
    use semfs_core::query::parser::parse_query;
    use semfs_core::retriever::rrf::reciprocal_rank_fusion;

    proptest! {
        /// Any string input to parse_query should never panic
        #[test]
        fn query_parser_never_panics(input in "\\PC{0,500}") {
            let _ = parse_query(&input);
        }

        /// Parse result should always have raw_input matching input
        #[test]
        fn query_parser_preserves_raw_input(input in "[a-zA-Z0-9가-힣 ]{1,100}") {
            let result = parse_query(&input);
            prop_assert_eq!(&result.raw_input, &input);
        }

        /// RRF should never produce negative scores
        #[test]
        fn rrf_scores_are_non_negative(
            list_len in 0..20usize,
            k in 1.0..100.0f32,
        ) {
            let list: Vec<(i64, f32)> = (0..list_len as i64)
                .map(|i| (i, 1.0 / (i as f32 + 1.0)))
                .collect();
            let results = reciprocal_rank_fusion(&[list], k);
            for (_, score) in &results {
                prop_assert!(*score >= 0.0, "Score should be non-negative: {}", score);
            }
        }

        /// RRF should return unique file IDs
        #[test]
        fn rrf_returns_unique_ids(
            n1 in 1..10usize,
            n2 in 1..10usize,
        ) {
            let list1: Vec<(i64, f32)> = (0..n1 as i64).map(|i| (i, 1.0)).collect();
            let list2: Vec<(i64, f32)> = (0..n2 as i64).map(|i| (i + 5, 1.0)).collect();
            let results = reciprocal_rank_fusion(&[list1, list2], 60.0);

            let mut ids: Vec<i64> = results.iter().map(|(id, _)| *id).collect();
            let orig_len = ids.len();
            ids.sort();
            ids.dedup();
            prop_assert_eq!(ids.len(), orig_len, "RRF returned duplicate IDs");
        }

        /// SQLite store should handle any valid file name
        #[test]
        fn sqlite_handles_unicode_filenames(name in "[a-zA-Z0-9가-힣_\\-\\.]{1,50}") {
            let store = semfs_storage::SqliteStore::in_memory().unwrap();
            let meta = semfs_storage::FileMeta {
                id: None,
                path: std::path::PathBuf::from(format!("/test/{}", name)),
                name: name.clone(),
                extension: Some("txt".to_string()),
                size: 100,
                hash: "test_hash".to_string(),
                created_at: 1000,
                modified_at: 2000,
                indexed_at: 3000,
                mime_type: None,
            };
            let id = store.insert_file(&meta).unwrap();
            let retrieved = store.get_file(id).unwrap();
            prop_assert_eq!(&retrieved.name, &name);
        }
    }
}
