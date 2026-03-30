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
struct EmbedRequest {
    model: String,
    prompt: String,
}

#[derive(Deserialize)]
struct EmbedResponse {
    embeddings: Vec<Vec<f32>>,
}

impl OllamaEmbedder {
    pub fn new(model: &str) -> Result<Self> {
        let base_url = std::env::var("OLLAMA_HOST")
            .unwrap_or_else(|_| "http://localhost:11434".to_string());

        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            base_url,
            model: model.to_string(),
        })
    }

    pub fn with_url(model: &str, base_url: &str) -> Result<Self> {
        Ok(Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()?,
            base_url: base_url.to_string(),
            model: model.to_string(),
        })
    }

    /// Check if Ollama is running and the model is available
    pub fn is_available(&self) -> bool {
        let url = format!("{}/api/tags", self.base_url);
        match self.client.get(&url).send() {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }
}

impl Embedder for OllamaEmbedder {
    fn embed_text(&self, text: &str) -> Result<Vec<f32>> {
        let url = format!("{}/api/embed", self.base_url);
        let request = EmbedRequest {
            model: self.model.clone(),
            prompt: text.to_string(),
        };

        let response = self.client
            .post(&url)
            .json(&request)
            .send()
            .map_err(|e| anyhow::anyhow!("Ollama connection error: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().unwrap_or_default();
            anyhow::bail!("Ollama error {}: {}", status, body);
        }

        let embed_response: EmbedResponse = response.json()
            .map_err(|e| anyhow::anyhow!("Failed to parse Ollama response: {}", e))?;

        let embedding = embed_response.embeddings.into_iter().next()
            .ok_or_else(|| anyhow::anyhow!("Ollama returned no embeddings"))?;

        debug!(model = %self.model, dims = embedding.len(), "Embedded text");
        Ok(embedding)
    }

    fn dimensions(&self) -> usize {
        // Common dimensions for popular models
        match self.model.as_str() {
            "multilingual-e5-base" => 768,
            "nomic-embed-text" => 768,
            "all-minilm" => 384,
            "mxbai-embed-large" => 1024,
            _ => 768, // default assumption
        }
    }

    fn model_name(&self) -> &str {
        &self.model
    }
}
