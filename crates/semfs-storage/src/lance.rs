use crate::error::{Result, StorageError};
use crate::types::*;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

/// Vector store for semantic search.
///
/// Default: in-memory brute-force search (no external dependencies).
/// With `lancedb-backend` feature: backed by LanceDB for ANN search at scale.
pub struct LanceStore {
    dimensions: usize,
    // In-memory store: file_id -> Vec<(chunk_id, vector)>
    data: RwLock<HashMap<FileId, Vec<(ChunkId, Vec<f32>)>>>,
    db_path: std::path::PathBuf,
}

impl LanceStore {
    pub fn new(path: &Path, dimensions: usize) -> Result<Self> {
        std::fs::create_dir_all(path).map_err(StorageError::Io)?;
        info!(path = %path.display(), dimensions, "Vector store initialized (in-memory)");
        Ok(Self {
            dimensions,
            data: RwLock::new(HashMap::new()),
            db_path: path.to_path_buf(),
        })
    }

    pub fn insert(&self, embeddings: &[ChunkEmbedding]) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        for emb in embeddings {
            if emb.vector.len() != self.dimensions {
                return Err(StorageError::VectorStore(
                    format!("Expected {} dimensions, got {}", self.dimensions, emb.vector.len()),
                ));
            }
        }

        let mut data = self.data.write();
        for emb in embeddings {
            data.entry(emb.file_id)
                .or_insert_with(Vec::new)
                .push((emb.chunk_id, emb.vector.clone()));
        }

        info!(count = embeddings.len(), "Inserted embeddings");
        Ok(())
    }

    pub fn search(&self, query_vector: &[f32], top_k: usize) -> Result<Vec<(FileId, f32)>> {
        if query_vector.len() != self.dimensions {
            return Err(StorageError::VectorStore(
                format!("Expected {} dimensions, got {}", self.dimensions, query_vector.len()),
            ));
        }

        let data = self.data.read();

        // Brute-force cosine similarity search
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_insert_and_search() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 3).unwrap();

        let embeddings = vec![
            ChunkEmbedding { chunk_id: 1, file_id: 1, vector: vec![1.0, 0.0, 0.0], content_preview: "file1".into() },
            ChunkEmbedding { chunk_id: 2, file_id: 2, vector: vec![0.0, 1.0, 0.0], content_preview: "file2".into() },
            ChunkEmbedding { chunk_id: 3, file_id: 3, vector: vec![0.9, 0.1, 0.0], content_preview: "file3".into() },
        ];
        store.insert(&embeddings).unwrap();

        let results = store.search(&[1.0, 0.0, 0.0], 2).unwrap();
        assert_eq!(results.len(), 2);
        // file1 should be most similar to [1,0,0]
        assert_eq!(results[0].0, 1);
    }

    #[test]
    fn test_delete() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 2).unwrap();

        store.insert(&[
            ChunkEmbedding { chunk_id: 1, file_id: 1, vector: vec![1.0, 0.0], content_preview: "a".into() },
        ]).unwrap();
        assert_eq!(store.count().unwrap(), 1);

        store.delete_by_file(1).unwrap();
        assert_eq!(store.count().unwrap(), 0);
    }

    #[test]
    fn test_dimension_mismatch() {
        let dir = TempDir::new().unwrap();
        let store = LanceStore::new(dir.path(), 3).unwrap();

        let result = store.insert(&[
            ChunkEmbedding { chunk_id: 1, file_id: 1, vector: vec![1.0, 0.0], content_preview: "x".into() },
        ]);
        assert!(result.is_err());
    }
}
