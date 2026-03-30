use thiserror::Error;

#[derive(Error, Debug)]
pub enum EmbedError {
    #[error("Model not available: {0}")]
    ModelNotAvailable(String),

    #[error("Embedding failed: {0}")]
    EmbeddingFailed(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),
}
