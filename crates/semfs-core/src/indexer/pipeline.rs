use crate::error::Result;
use crate::indexer::chunker;
use crate::indexer::crawler;
use crossbeam_channel::{bounded, Sender};
use semfs_embed::Embedder;
use semfs_storage::{CacheManager, Chunk, ChunkEmbedding, FileMeta, LanceStore, SqliteStore};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tracing::{debug, info, warn};

pub struct IndexingPipeline {
    sqlite: Arc<SqliteStore>,
    lance: Arc<LanceStore>,
    embedder: Arc<dyn Embedder>,
    cache: Arc<CacheManager>,
    ignore_patterns: Vec<String>,
    max_file_size: u64,
    batch_size: usize,
}

/// A batch of chunks ready for embedding
struct EmbedBatch {
    items: Vec<EmbedItem>,
}

struct EmbedItem {
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

    /// Run full indexing with pipelined embedding.
    ///
    /// Main thread: read files → chunk → store metadata + FTS → send chunks to embed channel
    /// Embed thread: receive chunk batches → call Ollama → store vectors
    ///
    /// This overlaps file I/O with model inference for maximum throughput.
    pub fn index_directory(&self, root: &Path) -> Result<IndexingStats> {
        info!(root = %root.display(), "Starting directory indexing");
        let files = crawler::crawl_directory(root, &self.ignore_patterns, self.max_file_size)?;

        let mut stats = IndexingStats {
            total_files: files.len(),
            ..Default::default()
        };

        let has_embedder = self.embedder.dimensions() > 0;
        // Smaller batches = faster pipeline throughput (Ollama processes faster with less input)
        let embed_batch_size = self.batch_size.max(50);

        // Set up producer-consumer pipeline for embeddings
        let (embed_tx, embed_rx) = bounded::<EmbedBatch>(4); // Buffer 4 batches ahead
        let embedded_count = Arc::new(AtomicUsize::new(0));
        let embedded_count_clone = embedded_count.clone();

        // Spawn embedding worker thread
        let embedder = self.embedder.clone();
        let lance = self.lance.clone();
        let embed_thread = if has_embedder {
            std::thread::Builder::new()
                .name("semfs-embed-worker".to_string())
                .spawn(move || {
                    while let Ok(batch) = embed_rx.recv() {
                        let texts: Vec<&str> =
                            batch.items.iter().map(|p| p.text.as_str()).collect();

                        match embedder.embed_batch(&texts) {
                            Ok(embeddings) => {
                                let chunk_embeddings: Vec<ChunkEmbedding> = embeddings
                                    .into_iter()
                                    .enumerate()
                                    .map(|(i, vec)| ChunkEmbedding {
                                        chunk_id: batch.items[i].chunk_id,
                                        file_id: batch.items[i].file_id,
                                        vector: vec,
                                        content_preview: batch.items[i]
                                            .text
                                            .chars()
                                            .take(100)
                                            .collect(),
                                    })
                                    .collect();

                                let count = chunk_embeddings.len();
                                if let Err(e) = lance.insert(&chunk_embeddings) {
                                    warn!(error = %e, "Failed to insert embeddings");
                                } else {
                                    embedded_count_clone.fetch_add(count, Ordering::Relaxed);
                                    info!(count, "Embedded batch");
                                }
                            }
                            Err(e) => {
                                warn!(
                                    error = %e,
                                    count = batch.items.len(),
                                    "Batch embedding failed"
                                );
                            }
                        }
                    }
                })
                .ok()
        } else {
            None
        };

        // Main thread: process files and stream batches to embed worker
        let mut current_batch: Vec<EmbedItem> = Vec::with_capacity(embed_batch_size);

        for path in &files {
            match self.process_file(
                path,
                has_embedder,
                &mut current_batch,
                &embed_tx,
                embed_batch_size,
            ) {
                Ok(true) => stats.indexed += 1,
                Ok(false) => stats.skipped += 1,
                Err(e) => {
                    debug!(path = %path.display(), error = %e, "Failed to index file");
                    stats.errors += 1;
                }
            }
        }

        // Flush remaining batch
        if !current_batch.is_empty() {
            let batch = EmbedBatch {
                items: std::mem::take(&mut current_batch),
            };
            let _ = embed_tx.send(batch);
        }

        // Signal worker to finish and wait
        drop(embed_tx);
        if let Some(thread) = embed_thread {
            let _ = thread.join();
        }

        let total_embedded = embedded_count.load(Ordering::Relaxed);

        info!(
            total = stats.total_files,
            indexed = stats.indexed,
            skipped = stats.skipped,
            errors = stats.errors,
            embedded = total_embedded,
            "Indexing complete"
        );
        Ok(stats)
    }

    /// Process a single file and stream embedding work to the worker.
    fn process_file(
        &self,
        path: &Path,
        has_embedder: bool,
        current_batch: &mut Vec<EmbedItem>,
        embed_tx: &Sender<EmbedBatch>,
        embed_batch_size: usize,
    ) -> Result<bool> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                debug!(path = %path.display(), error = %e, "Cannot read file as text");
                return Ok(false);
            }
        };

        let hash = compute_hash(&content);

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

        if let Ok(existing) = self.sqlite.get_file_by_path(path) {
            if let Some(id) = existing.id {
                let _ = self.sqlite.delete_chunks_for_file(id);
                let _ = self.sqlite.delete_file(id);
                let _ = self.lance.delete_by_file(id);
                self.cache.on_file_changed(id, Some(&existing.hash));
            }
        }

        let file_id = self.sqlite.insert_file(&file_meta)?;

        self.sqlite
            .index_content(file_id, &file_meta.name, &path.to_string_lossy(), &content)?;

        if let Some(chunker) = chunker::get_chunker(path) {
            let chunk_data = chunker.chunk(path, &content);

            for (idx, cd) in chunk_data.iter().enumerate() {
                let chunk = Chunk {
                    id: None,
                    file_id,
                    chunk_index: idx,
                    parent_chunk_id: None,
                    content: cd.content.clone(),
                    chunk_type: cd.chunk_type.clone(),
                    start_line: cd.start_line,
                    end_line: cd.end_line,
                    metadata: std::collections::HashMap::new(),
                };
                let chunk_id = self.sqlite.insert_chunk(&chunk)?;

                if has_embedder && !cd.content.trim().is_empty() {
                    current_batch.push(EmbedItem {
                        chunk_id,
                        file_id,
                        text: cd.content.clone(),
                    });

                    // Send batch when full — worker processes while we keep reading files
                    if current_batch.len() >= embed_batch_size {
                        let batch = EmbedBatch {
                            items: std::mem::take(current_batch),
                        };
                        let _ = embed_tx.send(batch);
                    }
                }
            }
        }

        debug!(file_id, path = %path.display(), "Indexed file");
        Ok(true)
    }

    /// Index a single file (for incremental updates)
    pub fn index_file(&self, path: &Path) -> Result<bool> {
        let (tx, rx) = bounded::<EmbedBatch>(1);
        let mut batch = Vec::new();
        let result =
            self.process_file(path, self.embedder.dimensions() > 0, &mut batch, &tx, 100)?;
        if !batch.is_empty() {
            let _ = tx.send(EmbedBatch { items: batch });
        }
        drop(tx);

        // Process embeddings inline for single file
        while let Ok(b) = rx.recv() {
            let texts: Vec<&str> = b.items.iter().map(|p| p.text.as_str()).collect();
            if let Ok(embeddings) = self.embedder.embed_batch(&texts) {
                let chunk_embeddings: Vec<ChunkEmbedding> = embeddings
                    .into_iter()
                    .enumerate()
                    .map(|(i, vec)| ChunkEmbedding {
                        chunk_id: b.items[i].chunk_id,
                        file_id: b.items[i].file_id,
                        vector: vec,
                        content_preview: b.items[i].text.chars().take(100).collect(),
                    })
                    .collect();
                let _ = self.lance.insert(&chunk_embeddings);
            }
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
