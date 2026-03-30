use crate::traits::Embedder;
use anyhow::Result;
use tracing::{debug, info};

/// ONNX Runtime embedder for local model inference.
///
/// Current status: Stub implementation that returns zero vectors.
/// The `ort` crate v2 API is still in release-candidate phase and changing rapidly.
/// Full implementation will be added when ort reaches a stable release.
///
/// To contribute: see https://github.com/joungminsung/SemanticFS/issues
pub struct OnnxEmbedder {
    model_path: String,
    dimensions: usize,
}

impl OnnxEmbedder {
    pub fn new(model_path: &str) -> Result<Self> {
        info!(path = model_path, "ONNX embedder initialized (stub — ort v2 API pending stable release)");

        let dimensions = if model_path.contains("MiniLM-L6") {
            384
        } else if model_path.contains("e5") || model_path.contains("nomic") {
            768
        } else {
            768
        };

        Ok(Self {
            model_path: model_path.to_string(),
            dimensions,
        })
    }
}

impl Embedder for OnnxEmbedder {
    fn embed_text(&self, _text: &str) -> Result<Vec<f32>> {
        debug!(model = %self.model_path, "ONNX embed (stub — returning zero vector)");
        Ok(vec![0.0; self.dimensions])
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model_path
    }
}
