use anyhow::Result;

/// Core embedding trait - all providers implement this
pub trait Embedder: Send + Sync {
    /// Embed a single text string
    fn embed_text(&self, text: &str) -> Result<Vec<f32>>;

    /// Embed a batch of texts (default: sequential calls)
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        texts.iter().map(|t| self.embed_text(t)).collect()
    }

    /// Number of dimensions in the output vectors
    fn dimensions(&self) -> usize;

    /// Human-readable model name
    fn model_name(&self) -> &str;
}

/// Provider selection based on available resources
pub enum EmbedderProvider {
    Ollama(String), // model name
    Onnx(String),   // model path
    Noop,
}

/// Create an embedder from provider config
pub fn create_embedder(provider: EmbedderProvider) -> Result<Box<dyn Embedder>> {
    match provider {
        #[cfg(feature = "ollama")]
        EmbedderProvider::Ollama(model) => {
            Ok(Box::new(crate::ollama::OllamaEmbedder::new(&model)?))
        }
        #[cfg(not(feature = "ollama"))]
        EmbedderProvider::Ollama(_) => {
            anyhow::bail!("Ollama support not compiled. Enable the 'ollama' feature.")
        }
        #[cfg(feature = "onnx")]
        EmbedderProvider::Onnx(path) => Ok(Box::new(crate::onnx::OnnxEmbedder::new(&path)?)),
        #[cfg(not(feature = "onnx"))]
        EmbedderProvider::Onnx(_) => {
            anyhow::bail!("ONNX support not compiled. Enable the 'onnx' feature.")
        }
        EmbedderProvider::Noop => Ok(Box::new(crate::noop::NoopEmbedder::new())),
    }
}

/// Auto-detect the best available embedder
pub fn auto_detect_embedder() -> Result<Box<dyn Embedder>> {
    // Try Ollama first
    #[cfg(feature = "ollama")]
    {
        if let Ok(embedder) = crate::ollama::OllamaEmbedder::new("multilingual-e5-base") {
            if embedder.is_available() {
                tracing::info!("Auto-detected Ollama embedder");
                return Ok(Box::new(embedder));
            }
        }
    }

    // Try ONNX
    #[cfg(feature = "onnx")]
    {
        let default_path = dirs::data_dir()
            .unwrap_or_default()
            .join("semanticfs")
            .join("models")
            .join("all-MiniLM-L6-v2.onnx");
        if default_path.exists() {
            if let Ok(embedder) = crate::onnx::OnnxEmbedder::new(&default_path.to_string_lossy()) {
                tracing::info!("Auto-detected ONNX embedder");
                return Ok(Box::new(embedder));
            }
        }
    }

    // Fallback to noop
    tracing::warn!("No embedding model found, falling back to keyword-only search");
    Ok(Box::new(crate::noop::NoopEmbedder::new()))
}
