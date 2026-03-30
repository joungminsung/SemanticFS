use crate::traits::Embedder;
use anyhow::Result;
use tracing::{debug, info, warn};

/// ONNX Runtime embedder for local model inference
pub struct OnnxEmbedder {
    model_path: String,
    dimensions: usize,
    session: ort::Session,
}

impl OnnxEmbedder {
    pub fn new(model_path: &str) -> Result<Self> {
        info!(path = model_path, "Loading ONNX embedding model");

        let session = ort::Session::builder()?
            .with_optimization_level(ort::GraphOptimizationLevel::Level3)?
            .with_intra_threads(4)?
            .commit_from_file(model_path)?;

        // Determine dimensions from model output shape
        let output_info = &session.outputs[0];
        let dimensions = match &output_info.output_type {
            ort::ValueType::Tensor { dimensions: dims, .. } => {
                dims.last().copied().unwrap_or(Some(768)).unwrap_or(768) as usize
            }
            _ => 768,
        };

        info!(dimensions, "ONNX model loaded successfully");

        Ok(Self {
            model_path: model_path.to_string(),
            dimensions,
            session,
        })
    }

    /// Simple whitespace tokenizer as fallback
    /// Real implementation should use a proper tokenizer (tokenizers crate)
    fn tokenize(&self, text: &str) -> Vec<i64> {
        // Simplified: each character gets a token ID
        // In production, use the `tokenizers` crate with the model's tokenizer.json
        let mut ids: Vec<i64> = Vec::with_capacity(512);
        ids.push(101); // [CLS]
        for ch in text.chars().take(510) {
            ids.push(ch as i64 % 30000 + 100); // Simplified token mapping
        }
        ids.push(102); // [SEP]
        ids
    }

    fn mean_pooling(token_embeddings: &[f32], dimensions: usize, seq_len: usize) -> Vec<f32> {
        let mut result = vec![0.0f32; dimensions];
        for i in 0..seq_len {
            for j in 0..dimensions {
                result[j] += token_embeddings[i * dimensions + j];
            }
        }
        for val in &mut result {
            *val /= seq_len as f32;
        }

        // L2 normalize
        let norm: f32 = result.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for val in &mut result {
                *val /= norm;
            }
        }

        result
    }
}

impl Embedder for OnnxEmbedder {
    fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let token_ids = self.tokenize(text);
        let seq_len = token_ids.len();

        let input_ids = ndarray::Array2::from_shape_vec(
            (1, seq_len),
            token_ids.clone(),
        )?;

        let attention_mask = ndarray::Array2::from_shape_vec(
            (1, seq_len),
            vec![1i64; seq_len],
        )?;

        let token_type_ids = ndarray::Array2::from_shape_vec(
            (1, seq_len),
            vec![0i64; seq_len],
        )?;

        let outputs = self.session.run(ort::inputs![
            "input_ids" => input_ids,
            "attention_mask" => attention_mask,
            "token_type_ids" => token_type_ids,
        ]?)?;

        let output_tensor = outputs[0].try_extract_tensor::<f32>()?;
        let output_view = output_tensor.view();
        let flat: Vec<f32> = output_view.iter().copied().collect();

        let embedding = Self::mean_pooling(&flat, self.dimensions, seq_len);

        debug!(dims = embedding.len(), "ONNX embedding generated");
        Ok(embedding)
    }

    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        // ONNX supports batched inference, but for simplicity we process sequentially
        // TODO: Implement true batched inference for better throughput
        texts.iter().map(|t| self.embed_text(t)).collect()
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn model_name(&self) -> &str {
        &self.model_path
    }
}
