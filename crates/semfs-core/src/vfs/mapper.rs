use crate::error::Result;
use crate::query::parse_query;
use crate::retriever::HybridRetriever;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Maps semantic paths to actual files via search
pub struct VfsMapper {
    retriever: HybridRetriever,
    source_root: PathBuf,
    max_results: usize,
}

/// A virtual directory entry
#[derive(Debug, Clone)]
pub struct VfsEntry {
    pub name: String,
    pub real_path: PathBuf,
    pub is_dir: bool,
    pub size: u64,
}

impl VfsMapper {
    pub fn new(retriever: HybridRetriever, source_root: PathBuf, max_results: usize) -> Self {
        Self {
            retriever,
            source_root,
            max_results,
        }
    }

    /// Resolve a semantic path to a list of virtual entries
    pub fn readdir(&self, semantic_path: &str) -> Result<Vec<VfsEntry>> {
        let query = parse_query(semantic_path);
        let results = self.retriever.search(&query, self.max_results)?;

        let entries: Vec<VfsEntry> = results
            .into_iter()
            .filter_map(|result| {
                let real_path = &result.path;
                if real_path.exists() {
                    let metadata = std::fs::metadata(real_path).ok()?;
                    Some(VfsEntry {
                        name: result.name,
                        real_path: real_path.clone(),
                        is_dir: metadata.is_dir(),
                        size: metadata.len(),
                    })
                } else {
                    None
                }
            })
            .collect();

        debug!(query = semantic_path, count = entries.len(), "VFS readdir");
        Ok(entries)
    }

    /// Resolve a semantic path + filename to a real file path
    pub fn resolve_file(&self, semantic_path: &str, filename: &str) -> Result<Option<PathBuf>> {
        let entries = self.readdir(semantic_path)?;
        Ok(entries
            .into_iter()
            .find(|e| e.name == filename)
            .map(|e| e.real_path))
    }

    pub fn source_root(&self) -> &Path {
        &self.source_root
    }
}
