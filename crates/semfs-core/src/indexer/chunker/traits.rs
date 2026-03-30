use semfs_storage::ChunkType;
use std::path::Path;

/// Trait for splitting files into semantic chunks
pub trait Chunker: Send + Sync {
    /// File extensions this chunker supports
    fn supported_extensions(&self) -> &[&str];

    /// Check if this chunker can handle the given file
    fn can_handle(&self, path: &Path) -> bool {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            self.supported_extensions().contains(&ext)
        } else {
            false
        }
    }

    /// Split file content into chunks with hierarchy
    fn chunk(&self, path: &Path, content: &str) -> Vec<ChunkData>;
}

/// Raw chunk data before storage
#[derive(Debug, Clone)]
pub struct ChunkData {
    pub content: String,
    pub chunk_type: ChunkType,
    pub parent_index: Option<usize>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub name: Option<String>,
}
