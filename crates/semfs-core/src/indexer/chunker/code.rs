use super::traits::{ChunkData, Chunker};
use semfs_storage::ChunkType;
use std::path::Path;
use tracing::debug;

/// Code chunker using tree-sitter for AST-based hierarchical chunking
pub struct CodeChunker;

impl CodeChunker {
    pub fn new() -> Self {
        Self
    }

    fn get_language(path: &Path) -> Option<tree_sitter::Language> {
        let ext = path.extension()?.to_str()?;
        match ext {
            "rs" => Some(tree_sitter_rust::LANGUAGE.into()),
            "py" => Some(tree_sitter_python::LANGUAGE.into()),
            "js" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),
            "ts" | "tsx" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
            "go" => Some(tree_sitter_go::LANGUAGE.into()),
            "java" => Some(tree_sitter_java::LANGUAGE.into()),
            "kt" | "kts" => Some(tree_sitter_kotlin::LANGUAGE.into()),
            _ => None,
        }
    }

    fn node_to_chunk_type(kind: &str) -> Option<ChunkType> {
        match kind {
            // Rust
            "function_item"
            | "function_definition"
            | "method_definition"
            | "function_declaration"
            | "method_declaration"
            | "arrow_function" => Some(ChunkType::Function),
            // Classes/Structs
            "struct_item"
            | "enum_item"
            | "class_declaration"
            | "class_definition"
            | "impl_item"
            | "trait_item"
            | "interface_declaration"
            | "type_declaration"
            | "object_declaration"
            | "companion_object" => Some(ChunkType::Class),
            // Module-level
            "source_file" | "module" | "program" => Some(ChunkType::Module),
            _ => None,
        }
    }

    fn extract_node_name(node: &tree_sitter::Node, source: &[u8]) -> Option<String> {
        // Try to find a name/identifier child
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            match child.kind() {
                "identifier"
                | "name"
                | "type_identifier"
                | "field_identifier"
                | "property_identifier" => {
                    return child.utf8_text(source).ok().map(|s| s.to_string());
                }
                _ => {}
            }
        }
        None
    }

    fn walk_tree(
        node: tree_sitter::Node,
        source: &[u8],
        parent_index: Option<usize>,
        chunks: &mut Vec<ChunkData>,
    ) {
        let kind = node.kind();

        if let Some(chunk_type) = Self::node_to_chunk_type(kind) {
            let content = if chunk_type == ChunkType::Module {
                String::new() // Don't duplicate entire file content
            } else {
                node.utf8_text(source).unwrap_or("").to_string()
            };

            // Skip empty or very small nodes
            if content.trim().len() < 5 && chunk_type != ChunkType::Module {
                return;
            }

            let name = Self::extract_node_name(&node, source);
            let current_index = chunks.len();

            chunks.push(ChunkData {
                content,
                chunk_type: chunk_type.clone(),
                parent_index,
                start_line: Some(node.start_position().row),
                end_line: Some(node.end_position().row),
                name,
            });

            // Recurse into children with this node as parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                Self::walk_tree(child, source, Some(current_index), chunks);
            }
        } else {
            // Not a chunk-worthy node, recurse with same parent
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                Self::walk_tree(child, source, parent_index, chunks);
            }
        }
    }
}

impl Default for CodeChunker {
    fn default() -> Self {
        Self::new()
    }
}

impl Chunker for CodeChunker {
    fn supported_extensions(&self) -> &[&str] {
        &["rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "kt", "kts"]
    }

    fn chunk(&self, path: &Path, content: &str) -> Vec<ChunkData> {
        if content.trim().is_empty() {
            return Vec::new();
        }

        let language = match Self::get_language(path) {
            Some(lang) => lang,
            None => {
                debug!(path = %path.display(), "No tree-sitter grammar for file, using fallback");
                return fallback_chunk(content);
            }
        };

        let mut parser = tree_sitter::Parser::new();
        if parser.set_language(&language).is_err() {
            debug!(path = %path.display(), "Failed to set tree-sitter language");
            return fallback_chunk(content);
        }

        let tree = match parser.parse(content, None) {
            Some(tree) => tree,
            None => {
                debug!(path = %path.display(), "Failed to parse file with tree-sitter");
                return fallback_chunk(content);
            }
        };

        let root = tree.root_node();
        let mut chunks = Vec::new();

        Self::walk_tree(root, content.as_bytes(), None, &mut chunks);

        if chunks.is_empty() {
            return fallback_chunk(content);
        }

        debug!(
            path = %path.display(),
            chunk_count = chunks.len(),
            "tree-sitter chunking complete"
        );
        chunks
    }
}

/// Fallback for unsupported languages: chunk entire file
fn fallback_chunk(content: &str) -> Vec<ChunkData> {
    if content.trim().is_empty() {
        return Vec::new();
    }
    vec![ChunkData {
        content: content.to_string(),
        chunk_type: ChunkType::File,
        parent_index: None,
        start_line: Some(0),
        end_line: Some(content.lines().count().saturating_sub(1)),
        name: None,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_ast_chunking() {
        let content = r#"
use std::io;

pub struct Server {
    port: u16,
}

impl Server {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn start(&self) {
        println!("Starting on port {}", self.port);
    }
}

fn main() {
    let config = Server::new(8080);
    config.start();
}
"#;
        let chunker = CodeChunker::new();
        let chunks = chunker.chunk(Path::new("server.rs"), content);

        assert!(!chunks.is_empty(), "Should produce chunks");

        // Should have functions with parent relationships
        let functions: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Function)
            .collect();
        assert!(
            functions.len() >= 2,
            "Should detect at least 2 functions, got {}",
            functions.len()
        );

        // At least some functions should have parent_index (inside impl block)
        let with_parent: Vec<_> = functions
            .iter()
            .filter(|f| f.parent_index.is_some())
            .collect();
        assert!(
            !with_parent.is_empty(),
            "Some functions should have parents (inside impl)"
        );
    }

    #[test]
    fn test_python_ast_chunking() {
        let content = r#"
import os

class Server:
    def __init__(self, port):
        self.port = port

    def start(self):
        print(f"Starting on {self.port}")

def main():
    server = Server(8080)
    server.start()
"#;
        let chunker = CodeChunker::new();
        let chunks = chunker.chunk(Path::new("server.py"), content);

        assert!(!chunks.is_empty(), "Should produce chunks");

        let classes: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Class)
            .collect();
        assert!(!classes.is_empty(), "Should detect class");
    }

    #[test]
    fn test_unsupported_extension_fallback() {
        let chunker = CodeChunker::new();
        let chunks = chunker.chunk(Path::new("script.rb"), "def hello\n  puts 'hi'\nend");
        // Should fallback since .rb isn't in supported_extensions for tree-sitter
        assert!(!chunks.is_empty());
    }

    #[test]
    fn test_empty_file() {
        let chunker = CodeChunker::new();
        let chunks = chunker.chunk(Path::new("empty.rs"), "");
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_kotlin_chunking() {
        let content = r#"
package com.example

class Server {
    private val port: Int
    
    constructor(port: Int) {
        this.port = port
    }
    
    fun start() {
        println("Starting on port $port")
    }
    
    companion object {
        fun create(port: Int): Server = Server(port)
    }
}

object Constants {
    const val DEFAULT_PORT = 8080
}
"#;
        let chunker = CodeChunker::new();
        let chunks = chunker.chunk(Path::new("server.kt"), content);

        assert!(!chunks.is_empty(), "Should produce chunks");

        // Should have classes (class Server, object Constants)
        let classes: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Class)
            .collect();
        assert!(
            classes.len() >= 2,
            "Should detect at least 2 classes (Server and Constants), got {}",
            classes.len()
        );

        // Should have functions (fun start, fun create)
        let functions: Vec<_> = chunks
            .iter()
            .filter(|c| c.chunk_type == ChunkType::Function)
            .collect();
        assert!(
            functions.len() >= 2,
            "Should detect at least 2 functions, got {}",
            functions.len()
        );

        // Functions inside class should have parent_index
        let with_parent: Vec<_> = functions
            .iter()
            .filter(|f| f.parent_index.is_some())
            .collect();
        assert!(
            !with_parent.is_empty(),
            "Some functions should have parents (inside class)"
        );
    }
}
