pub mod traits;
pub mod noop;

#[cfg(feature = "ollama")]
pub mod ollama;

#[cfg(feature = "onnx")]
pub mod onnx;

pub use noop::NoopEmbedder;
pub use traits::{Embedder, EmbedderProvider, auto_detect_embedder, create_embedder};
