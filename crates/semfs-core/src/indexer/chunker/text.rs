use super::traits::{ChunkData, Chunker};
use semfs_storage::ChunkType;
use std::path::Path;

/// Text/markdown chunker - splits by headings and paragraphs
pub struct TextChunker;

impl TextChunker {
    pub fn new() -> Self {
        Self
    }
}

impl Chunker for TextChunker {
    fn supported_extensions(&self) -> &[&str] {
        &["txt", "md", "rst", "adoc"]
    }

    fn chunk(&self, _path: &Path, content: &str) -> Vec<ChunkData> {
        let mut chunks = Vec::new();
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            return chunks;
        }

        let mut current_section_idx: Option<usize> = None;
        let mut current_text = String::new();
        let mut current_start = 0;

        for (i, line) in lines.iter().enumerate() {
            // Detect markdown headings
            if line.starts_with('#') {
                // Flush current paragraph
                if !current_text.trim().is_empty() {
                    chunks.push(ChunkData {
                        content: current_text.trim().to_string(),
                        chunk_type: ChunkType::Paragraph,
                        parent_index: current_section_idx,
                        start_line: Some(current_start),
                        end_line: Some(i.saturating_sub(1)),
                        name: None,
                    });
                }

                // Start new section
                let section_name = line.trim_start_matches('#').trim().to_string();
                current_section_idx = Some(chunks.len());
                chunks.push(ChunkData {
                    content: line.to_string(),
                    chunk_type: ChunkType::Section,
                    parent_index: None, // Top-level sections
                    start_line: Some(i),
                    end_line: Some(i),
                    name: Some(section_name),
                });
                current_text = String::new();
                current_start = i + 1;
            } else if line.trim().is_empty() && !current_text.trim().is_empty() {
                // Paragraph break
                chunks.push(ChunkData {
                    content: current_text.trim().to_string(),
                    chunk_type: ChunkType::Paragraph,
                    parent_index: current_section_idx,
                    start_line: Some(current_start),
                    end_line: Some(i.saturating_sub(1)),
                    name: None,
                });
                current_text = String::new();
                current_start = i + 1;
            } else {
                current_text.push_str(line);
                current_text.push('\n');
            }
        }

        // Flush remaining text
        if !current_text.trim().is_empty() {
            chunks.push(ChunkData {
                content: current_text.trim().to_string(),
                chunk_type: ChunkType::Paragraph,
                parent_index: current_section_idx,
                start_line: Some(current_start),
                end_line: Some(lines.len().saturating_sub(1)),
                name: None,
            });
        }

        // If no sections found, wrap everything as a single File chunk
        if chunks.is_empty() {
            chunks.push(ChunkData {
                content: content.to_string(),
                chunk_type: ChunkType::File,
                parent_index: None,
                start_line: Some(0),
                end_line: Some(lines.len().saturating_sub(1)),
                name: None,
            });
        }

        chunks
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_chunking() {
        let content = "# Heading 1\n\nParagraph one.\n\n## Heading 2\n\nParagraph two.\nMore text.";
        let chunker = TextChunker::new();
        let chunks = chunker.chunk(Path::new("test.md"), content);

        assert!(chunks.len() >= 4); // 2 sections + 2 paragraphs
        assert_eq!(chunks[0].chunk_type, ChunkType::Section);
        assert!(chunks[0].name.as_ref().unwrap().contains("Heading 1"));
    }

    #[test]
    fn test_plain_text() {
        let content = "Just some plain text\nwith multiple lines.";
        let chunker = TextChunker::new();
        let chunks = chunker.chunk(Path::new("test.txt"), content);

        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_empty_content() {
        let chunker = TextChunker::new();
        let chunks = chunker.chunk(Path::new("test.md"), "");
        assert!(chunks.is_empty());
    }
}
