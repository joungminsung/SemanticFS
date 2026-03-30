use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub type FileId = i64;
pub type ChunkId = i64;
pub type WalEntryId = i64;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMeta {
    pub id: Option<FileId>,
    pub path: PathBuf,
    pub name: String,
    pub extension: Option<String>,
    pub size: u64,
    pub hash: String,
    pub created_at: i64,
    pub modified_at: i64,
    pub indexed_at: i64,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub id: Option<ChunkId>,
    pub file_id: FileId,
    pub chunk_index: usize,
    pub parent_chunk_id: Option<ChunkId>,
    pub content: String,
    pub chunk_type: ChunkType,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ChunkType {
    Module,
    Class,
    Function,
    Section,
    Paragraph,
    File,
    DataKey,
}

impl ChunkType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Module => "module",
            Self::Class => "class",
            Self::Function => "function",
            Self::Section => "section",
            Self::Paragraph => "paragraph",
            Self::File => "file",
            Self::DataKey => "data_key",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "module" => Self::Module,
            "class" => Self::Class,
            "function" => Self::Function,
            "section" => Self::Section,
            "paragraph" => Self::Paragraph,
            "file" => Self::File,
            "data_key" => Self::DataKey,
            _ => Self::File,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkEmbedding {
    pub chunk_id: ChunkId,
    pub file_id: FileId,
    pub vector: Vec<f32>,
    pub content_preview: String,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_id: FileId,
    pub path: PathBuf,
    pub name: String,
    pub score: f32,
    pub matched_chunks: Vec<ChunkId>,
}

#[derive(Debug, Clone)]
pub enum FileOperation {
    Move { source: PathBuf, dest: PathBuf },
    Copy { source: PathBuf, dest: PathBuf },
    Delete { path: PathBuf },
    Write { path: PathBuf, data: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq)]
pub enum OperationStatus {
    Pending,
    Executing,
    Completed,
    Failed,
}

impl OperationStatus {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Executing => "executing",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "pending" => Self::Pending,
            "executing" => Self::Executing,
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

#[derive(Debug, Clone)]
pub struct WalEntry {
    pub id: WalEntryId,
    pub operation: FileOperation,
    pub status: OperationStatus,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub enum MetadataFilter {
    DateRange { start: i64, end: i64 },
    Extension(Vec<String>),
    Size { min: Option<u64>, max: Option<u64> },
    MimeType(Vec<String>),
    PathPrefix(String),
}

#[derive(Debug, Clone)]
pub struct AclRule {
    pub id: Option<i64>,
    pub pattern: String,
    pub permission: Permission,
    pub mount_point: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Permission {
    Read,
    Write,
    Deny,
}
