pub mod code;
pub mod text;
pub mod traits;

pub use code::CodeChunker;
pub use text::TextChunker;
pub use traits::{ChunkData, Chunker};

/// Get the appropriate chunker for a file path
pub fn get_chunker(path: &std::path::Path) -> Option<Box<dyn Chunker>> {
    let text_chunker = TextChunker::new();
    let code_chunker = CodeChunker::new();

    if text_chunker.can_handle(path) {
        Some(Box::new(text_chunker))
    } else if code_chunker.can_handle(path) {
        Some(Box::new(code_chunker))
    } else {
        // Default: treat as plain text if it looks like text
        if is_likely_text(path) {
            Some(Box::new(text_chunker))
        } else {
            None
        }
    }
}

fn is_likely_text(path: &std::path::Path) -> bool {
    let text_extensions = [
        "json", "yaml", "yml", "toml", "xml", "html", "css", "scss", "less", "sql", "sh", "bash",
        "zsh", "fish", "env", "cfg", "ini", "conf", "csv", "tsv", "log",
    ];
    path.extension()
        .and_then(|e| e.to_str())
        .map(|ext| text_extensions.contains(&ext))
        .unwrap_or(false)
}
