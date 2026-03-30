use crate::traits::Embedder;
use anyhow::Result;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use tracing::debug;

pub struct OllamaEmbedder {
    client: Client,
    base_url: String,
    model: String,
}

#[derive(Serialize)]
struct EmbedRequestSingle {
    model: String,
    input: String,
}

#[derive(Serialize)]
struct EmbedRequestBatch {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaEmbedder {
    pub fn new(model: &str) -> Result<Self> {
        let base_url =
            std::env::var("OLLAMA_HOST").unwrap_or_else(|_| "http://localhost:11434".to_string());

        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url,
            model: model.to_string(),
        })
    }

    pub fn with_url(model: &str, base_url: &str) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(120))
                .build()?,
            base_url: base_url.to_string(),
            model: model.to_string(),
        })
    }

    /// Check if Ollama is running
    pub fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send() {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    fn call_embed(&self, body: &impl Serialize) -> Result<EmbedResponse> {
        let url = format!("{}/api/embed", self.base_url);

        let response = self
            .client
            .post(&url)
            .json(body)
            .send()
            .map_err(|e| anyhow::anyhow!("Ollama connection error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Ollama error {}: {}", status, body);
        }

        response
            .json()
            .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))
    }
}

impl Embedder for OllamaEmbedder {
    fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let request = EmbedRequestSingle {
            model: self.model.clone(),
            input: text.to_string(),
        };

        let resp = self.call_embed(&request)?;
        let embedding = resp
            .embeddings
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("Ollama returned no embeddings"))?;

        debug!(model = %self.model, dims = embedding.len(), "Embedded text");
        Ok(embedding)
    }

    /// Batch embed — sends all texts in a single API call to Ollama.
    /// This is ~100x faster than individual calls.
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        // Ollama /api/embed accepts input as array
        let request = EmbedRequestBatch {
            model: self.model.clone(),
            input: texts.iter().map(|t| t.to_string()).collect(),
        };

        let resp = self.call_embed(&request)?;

        if resp.embeddings.len() != texts.len() {
            anyhow::bail!(
                "Ollama returned {} embeddings for {} inputs",
                resp.embeddings.len(),
                texts.len()
            );
        }

        debug!(
            model = %self.model,
            count = texts.len(),
            "Batch embedded"
        );
        Ok(resp.embeddings)
    }

    fn dimensions(&self) -> usize {
        match self.model.as_str() {
            "bge-m3" => 1024,
            "multilingual-e5-base" | "multilingual-e5-large" => 768,
            "nomic-embed-text" => 768,
            "all-minilm" => 384,
            "mxbai-embed-large" => 1024,
            _ => 768,
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
