use crate::error::Result;
use crate::indexer::chunker;
use crate::indexer::crawler;
use semfs_embed::Embedder;
use semfs_storage::{Chunk, ChunkEmbedding, FileMeta, LanceStore, SqliteStore, CacheManager};
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

        // Process in batches
        for batch in files.chunks(self.batch_size) {
            match self.process_batch(batch) {
                Ok(batch_stats) => {
                    stats.indexed += batch_stats.indexed;
                    stats.skipped += batch_stats.skipped;
                    stats.errors += batch_stats.errors;
                }
                Err(e) => {
                    error!("Batch processing error: {}", e);
                    stats.errors += batch.len();
                }
            }
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

    /// Index a single file (for incremental updates)
    pub fn index_file(&self, path: &Path) -> Result<bool> {
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
                debug!(path = %path.display(), "File unchanged, skipping");
                return Ok(false);
            }
        }

        // Get file metadata
        let metadata = std::fs::metadata(path)?;
        let file_meta = FileMeta {
            id: None,
            path: path.to_path_buf(),
            name: path.file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown")
                .to_string(),
            extension: path.extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_string()),
            size: metadata.len(),
            hash: hash.clone(),
            created_at: metadata.created()
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
                .duration_since(std::time::SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            modified_at: metadata.modified()
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
        self.sqlite.index_content(file_id, &file_meta.name,
            &path.to_string_lossy(), &content)?;

        // Chunk the file
        if let Some(chunker) = chunker::get_chunker(path) {
            let chunk_data = chunker.chunk(path, &content);
            let mut chunk_ids = Vec::new();

            for (idx, cd) in chunk_data.iter().enumerate() {
                let chunk = Chunk {
                    id: None,
                    file_id,
                    chunk_index: idx,
                    parent_chunk_id: cd.parent_index.and_then(|pi| chunk_ids.get(pi).copied()),
                    content: cd.content.clone(),
                    chunk_type: cd.chunk_type.clone(),
                    start_line: cd.start_line,
                    end_line: cd.end_line,
                    metadata: std::collections::HashMap::new(),
                };
                let chunk_id = self.sqlite.insert_chunk(&chunk)?;
                chunk_ids.push(chunk_id);
            }

            // Generate embeddings for chunks
            if self.embedder.dimensions() > 0 {
                let texts: Vec<&str> = chunk_data.iter()
                    .map(|cd| cd.content.as_str())
                    .collect();

                match self.embedder.embed_batch(&texts) {
                    Ok(embeddings) => {
                        let chunk_embeddings: Vec<ChunkEmbedding> = embeddings.into_iter()
                            .enumerate()
                            .map(|(i, vec)| ChunkEmbedding {
                                chunk_id: chunk_ids[i],
                                file_id,
                                vector: vec,
                                content_preview: chunk_data[i].content.chars().take(100).collect(),
                            })
                            .collect();

                        if let Err(e) = self.lance.insert(&chunk_embeddings) {
                            warn!(error = %e, "Failed to insert embeddings");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, path = %path.display(), "Failed to embed chunks");
                    }
                }
            }
        }

        debug!(file_id, path = %path.display(), "Indexed file");
        Ok(true)
    }

    fn process_batch(&self, files: &[PathBuf]) -> Result<IndexingStats> {
        let mut stats = IndexingStats::default();

        for path in files {
            match self.index_file(path) {
                Ok(true) => stats.indexed += 1,
                Ok(false) => stats.skipped += 1,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "Failed to index file");
                    stats.errors += 1;
                }
            }
        }

        Ok(stats)
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
        }.to_string()
    })
}
