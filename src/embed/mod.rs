mod content;
mod ollama;
mod similarity;

pub use content::{gather_text, compute_content_hash, GatheredContent};
pub use ollama::{OllamaClient, embedding_to_bytes, bytes_to_embedding};
pub use similarity::cosine_similarity;
