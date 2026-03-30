use crate::error::Result;
use crate::indexer::chunker;
use crate::indexer::crawler;
use semfs_embed::Embedder;
use semfs_storage::{CacheManager, Chunk, ChunkEmbedding, FileMeta, LanceStore, SqliteStore};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub struct IndexingPipeline {
    sqlite: Arc<SqliteStore>,
    lance: Arc<LanceStore>,
    embedder: Arc<dyn Embedder>,
    cache: Arc<CacheManager>,
    ignore_patterns: Vec<String>,
    max_file_size: u64,
    batch_size: usize,
}

/// Pending embedding work collected during file processing
struct PendingEmbedding {
    chunk_id: i64,
    file_id: i64,
    text: String,
}

impl IndexingPipeline {
    pub fn new(
        sqlite: Arc<SqliteStore>,
        lance: Arc<LanceStore>,
        embedder: Arc<dyn Embedder>,
        cache: Arc<CacheManager>,
        ignore_patterns: Vec<String>,
        max_file_size: u64,
        batch_size: usize,
    ) -> Self {
        Self {
            sqlite,
            lance,
            embedder,
            cache,
            ignore_patterns,
            max_file_size,
            batch_size,
        }
    }

    /// Run full indexing on a directory
    pub fn index_directory(&self, root: &Path) -> Result<IndexingStats> {
        info!(root = %root.display(), "Starting directory indexing");
        let files = crawler::crawl_directory(root, &self.ignore_patterns, self.max_file_size)?;

        let mut stats = IndexingStats {
            total_files: files.len(),
            ..Default::default()
        };

        // Phase 1: Process all files — crawl, chunk, store metadata + FTS
        // Collect embedding work without blocking on model inference
        let mut pending_embeddings: Vec<PendingEmbedding> = Vec::new();

        for path in &files {
            match self.process_file(path, &mut pending_embeddings) {
                Ok(true) => stats.indexed += 1,
                Ok(false) => stats.skipped += 1,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "Failed to index file");
                    stats.errors += 1;
                }
            }
        }

        // Phase 2: Batch embed all collected chunks at once
        if self.embedder.dimensions() > 0 && !pending_embeddings.is_empty() {
            info!(
                chunks = pending_embeddings.len(),
                "Embedding all chunks in batches"
            );
            self.flush_embeddings(&pending_embeddings);
        }

        info!(
            total = stats.total_files,
            indexed = stats.indexed,
            skipped = stats.skipped,
            errors = stats.errors,
            "Indexing complete"
        );
        Ok(stats)
    }

    /// Process a single file: read, hash-check, chunk, store metadata.
    /// Embedding work is deferred to `pending_embeddings`.
    fn process_file(
        &self,
        path: &Path,
        pending_embeddings: &mut Vec<PendingEmbedding>,
    ) -> Result<bool> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                debug!(path = %path.display(), error = %e, "Cannot read file as text");
                return Ok(false);
            }
        };

        let hash = compute_hash(&content);

        // Check if already indexed with same hash
        if let Ok(Some(existing_hash)) = self.sqlite.get_file_hash(path) {
            if existing_hash == hash {
                return Ok(false);
            }
        }

        let metadata = std::fs::metadata(path)?;
        let file_meta = FileMeta {
            id: None,
            path: path.to_path_buf(),
            name: path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            extension: path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string()),
            size: metadata.len(),
            hash: hash.clone(),
            created_at: metadata
                .created()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            modified_at: metadata
                .modified()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            indexed_at: chrono::Utc::now().timestamp(),
            mime_type: detect_mime_type(path),
        };

        // Delete existing entry if updating
        if let Ok(existing) = self.sqlite.get_file_by_path(path) {
            if let Some(id) = existing.id {
                let _ = self.sqlite.delete_chunks_for_file(id);
                let _ = self.sqlite.delete_file(id);
                let _ = self.lance.delete_by_file(id);
                self.cache.on_file_changed(id, Some(&existing.hash));
            }
        }

        // Insert file metadata
        let file_id = self.sqlite.insert_file(&file_meta)?;

        // Index content for FTS
        self.sqlite
            .index_content(file_id, &file_meta.name, &path.to_string_lossy(), &content)?;

        // Chunk the file and collect embedding work
        if let Some(chunker) = chunker::get_chunker(path) {
            let chunk_data = chunker.chunk(path, &content);

            for (idx, cd) in chunk_data.iter().enumerate() {
                let chunk = Chunk {
                    id: None,
                    file_id,
                    chunk_index: idx,
                    parent_chunk_id: None, // Simplified — parent resolution deferred
                    content: cd.content.clone(),
                    chunk_type: cd.chunk_type.clone(),
                    start_line: cd.start_line,
                    end_line: cd.end_line,
                    metadata: std::collections::HashMap::new(),
                };
                let chunk_id = self.sqlite.insert_chunk(&chunk)?;

                // Queue non-empty chunks for embedding
                if self.embedder.dimensions() > 0 && !cd.content.trim().is_empty() {
                    pending_embeddings.push(PendingEmbedding {
                        chunk_id,
                        file_id,
                        text: cd.content.clone(),
                    });
                }
            }
        }

        debug!(file_id, path = %path.display(), "Indexed file metadata");
        Ok(true)
    }

    /// Flush all pending embeddings in large batches
    fn flush_embeddings(&self, pending: &[PendingEmbedding]) {
        let embed_batch_size = self.batch_size.max(50); // At least 50 per batch

        for batch in pending.chunks(embed_batch_size) {
            let texts: Vec<&str> = batch.iter().map(|p| p.text.as_str()).collect();

            match self.embedder.embed_batch(&texts) {
                Ok(embeddings) => {
                    let chunk_embeddings: Vec<ChunkEmbedding> = embeddings
                        .into_iter()
                        .enumerate()
                        .map(|(i, vec)| ChunkEmbedding {
                            chunk_id: batch[i].chunk_id,
                            file_id: batch[i].file_id,
                            vector: vec,
                            content_preview: batch[i].text.chars().take(100).collect(),
                        })
                        .collect();

                    if let Err(e) = self.lance.insert(&chunk_embeddings) {
                        warn!(error = %e, "Failed to insert embeddings");
                    } else {
                        info!(count = chunk_embeddings.len(), "Embedded batch");
                    }
                }
                Err(e) => {
                    warn!(error = %e, count = batch.len(), "Batch embedding failed");
                }
            }
        }
    }

    /// Index a single file (for incremental updates)
    pub fn index_file(&self, path: &Path) -> Result<bool> {
        let mut pending = Vec::new();
        let result = self.process_file(path, &mut pending)?;
        if !pending.is_empty() {
            self.flush_embeddings(&pending);
        }
        Ok(result)
    }
}

#[derive(Debug, Default)]
pub struct IndexingStats {
    pub total_files: usize,
    pub indexed: usize,
    pub skipped: usize,
    pub errors: usize,
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn detect_mime_type(path: &Path) -> Option<String> {
    path.extension().and_then(|ext| ext.to_str()).map(|ext| {
        match ext {
            "rs" => "text/x-rust",
            "py" => "text/x-python",
            "js" | "jsx" => "text/javascript",
            "ts" | "tsx" => "text/typescript",
            "go" => "text/x-go",
            "java" => "text/x-java",
            "md" => "text/markdown",
            "txt" => "text/plain",
            "json" => "application/json",
            "yaml" | "yml" => "text/yaml",
            "toml" => "text/toml",
            "html" => "text/html",
            "css" => "text/css",
            "csv" => "text/csv",
            _ => "application/octet-stream",
        }
        .to_string()
    })
}
