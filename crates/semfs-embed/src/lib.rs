pub mod noop;
pub mod traits;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "onnx")]
pub mod onnx;

pub use noop::NoopEmbedder;
pub use traits::{auto_detect_embedder, create_embedder, Embedder, EmbedderProvider};
