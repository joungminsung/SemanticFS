use crate::error::{Result, StorageError};
use crate::types::*;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};

/// Vector store for semantic search.
/// Persists embeddings to disk as a binary file for cross-process durability.
type EmbeddingData = HashMap<FileId, Vec<(ChunkId, Vec<f32>)>>;

#[derive(Serialize, Deserialize)]
struct VectorStoreData {
    dimensions: usize,
    entries: Vec<VectorEntry>,
}

#[derive(Serialize, Deserialize)]
struct VectorEntry {
    file_id: FileId,
    chunk_id: ChunkId,
    vector: Vec<f32>,
}

pub struct LanceStore {
    dimensions: usize,
    data: RwLock<EmbeddingData>,
    db_path: PathBuf,
}

impl LanceStore {
    pub fn new(path: &Path, dimensions: usize) -> Result<Self> {
        std::fs::create_dir_all(path).map_err(StorageError::Io)?;

        let store = Self {
            dimensions,
            data: RwLock::new(HashMap::new()),
            db_path: path.to_path_buf(),
        };

        // Load existing data from disk
        store.load_from_disk();

        info!(
            path = %path.display(),
            dimensions,
            count = store.count().unwrap_or(0),
            "Vector store initialized"
        );
        Ok(store)
    }

    fn data_file(&self) -> PathBuf {
        self.db_path.join("vectors.bin")
    }

    fn load_from_disk(&self) {
        let path = self.data_file();
        if !path.exists() {
            return;
        }

        match std::fs::read(&path) {
            Ok(bytes) => match bincode_decode::<VectorStoreData>(&bytes) {
                Ok(store_data) => {
                    if store_data.dimensions != self.dimensions {
                        warn!(
                            stored = store_data.dimensions,
                            expected = self.dimensions,
                            "Vector dimensions mismatch, discarding stored vectors"
                        );
                        return;
                    }
                    let mut data = self.data.write();
                    for entry in store_data.entries {
                        data.entry(entry.file_id)
                            .or_default()
                            .push((entry.chunk_id, entry.vector));
                    }
                    debug!(
                        count = data.values().map(|v| v.len()).sum::<usize>(),
                        "Loaded vectors from disk"
                    );
                }
                Err(e) => {
                    warn!(error = %e, "Failed to parse vector store, starting fresh");
                }
            },
            Err(e) => {
                warn!(error = %e, "Failed to read vector store file");
            }
        }
    }

    fn save_to_disk(&self) {
        let data = self.data.read();
        let mut entries = Vec::new();
        for (&file_id, chunks) in data.iter() {
            for (chunk_id, vector) in chunks {
                entries.push(VectorEntry {
                    file_id,
                    chunk_id: *chunk_id,
                    vector: vector.clone(),
                });
            }
        }

        let store_data = VectorStoreData {
            dimensions: self.dimensions,
            entries,
        };

        match bincode_encode(&store_data) {
            Ok(bytes) => {
                if let Err(e) = std::fs::write(self.data_file(), bytes) {
                    warn!(error = %e, "Failed to persist vector store");
                }
            }
            Err(e) => {
                warn!(error = %e, "Failed to serialize vector store");
            }
        }
    }

    pub fn insert(&self, embeddings: &[ChunkEmbedding]) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        for emb in embeddings {
            if emb.vector.len() != self.dimensions {
                return Err(StorageError::VectorStore(format!(
                    "Expected {} dimensions, got {}",
                    self.dimensions,
                    emb.vector.len()
                )));
            }
        }

        {
            let mut data = self.data.write();
            for emb in embeddings {
                data.entry(emb.file_id)
                    .or_default()
                    .push((emb.chunk_id, emb.vector.clone()));
            }
        }

        // Persist after each batch insert
        self.save_to_disk();

        info!(count = embeddings.len(), "Inserted embeddings");
        Ok(())
    }

    pub fn search(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<(FileId, f32)>> {
        if query_vector.len() != self.dimensions {
            return Err(StorageError::VectorStore(format!(
                "Expected {} dimensions, got {}",
                self.dimensions,
                query_vector.len()
            )));
        }

        let data = self.data.read();

        let mut file_scores: HashMap<FileId, f32> = HashMap::new();

        for (&file_id, chunks) in data.iter() {
            let mut best_score: f32 = f32::NEG_INFINITY;
            for (_chunk_id, vec) in chunks {
                let score = cosine_similarity(query_vector, vec);
                if score > best_score {
                    best_score = score;
                }
            }
            file_scores.insert(file_id, best_score);
        }

        let mut results: Vec<(FileId, f32)> = file_scores.into_iter().collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);

        debug!(results = results.len(), "Vector search complete");
        Ok(results)
    }

    pub fn delete_by_file(&self, file_id: FileId) -> Result<()> {
        self.data.write().remove(&file_id);
        self.save_to_disk();
        debug!(file_id, "Deleted embeddings for file");
        Ok(())
    }

    pub fn count(&self) -> Result<usize> {
        let data = self.data.read();
        Ok(data.values().map(|v| v.len()).sum())
    }
}

fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

// Simple binary serialization using serde_json (no extra deps)
fn bincode_encode(data: &VectorStoreData) -> std::result::Result<Vec<u8>, String> {
    serde_json::to_vec(data).map_err(|e| e.to_string())
}

fn bincode_decode<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> std::result::Result<T, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_insert_and_search() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 3).unwrap();

        let embeddings = vec![
            ChunkEmbedding {
                chunk_id: 1,
                file_id: 1,
                vector: vec![1.0, 0.0, 0.0],
                content_preview: "file1".into(),
            },
            ChunkEmbedding {
                chunk_id: 2,
                file_id: 2,
                vector: vec![0.0, 1.0, 0.0],
                content_preview: "file2".into(),
            },
            ChunkEmbedding {
                chunk_id: 3,
                file_id: 3,
                vector: vec![0.9, 0.1, 0.0],
                content_preview: "file3".into(),
            },
        ];
        store.insert(&embeddings).unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_persistence() {
        let dir = TempDir::new().unwrap();

        // Write
        {
            let store = LanceStore::new(dir.path(), 3).unwrap();
            store
                .insert(&[ChunkEmbedding {
                    chunk_id: 1,
                    file_id: 1,
                    vector: vec![1.0, 0.0, 0.0],
                    content_preview: "test".into(),
                }])
                .unwrap();
            assert_eq!(store.count().unwrap(), 1);
        }

        // Read back in new instance
        {
            let store = LanceStore::new(dir.path(), 3).unwrap();
            assert_eq!(store.count().unwrap(), 1);
            let results = store.search(&[1.0, 0.0, 0.0], 5).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].0, 1);
        }
    }

    #[test]
    fn test_delete() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 2).unwrap();

        store
            .insert(&[ChunkEmbedding {
                chunk_id: 1,
                file_id: 1,
                vector: vec![1.0, 0.0],
                content_preview: "a".into(),
            }])
            .unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.delete_by_file(1).unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_dimension_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 3).unwrap();

        let result = store.insert(&[ChunkEmbedding {
            chunk_id: 1,
            file_id: 1,
            vector: vec![1.0, 0.0],
            content_preview: "x".into(),
        }]);
        assert!(result.is_err());
    }
}
