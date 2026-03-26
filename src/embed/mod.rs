mod content;
mod ollama;
mod openai;
mod similarity;

pub use content::{gather_text, compute_content_hash, GatheredContent};
pub use similarity::cosine_similarity;

use crate::config::{AppConfig, EmbeddingBackend};

pub trait EmbeddingClient {
	fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>>;
}

pub fn build_client(config: &AppConfig) -> Result<Box<dyn EmbeddingClient>, Box<dyn std::error::Error>> {
	match config.embedding_backend {
		EmbeddingBackend::Ollama => {
			Ok(Box::new(ollama::OllamaClient::new(
				&config.ollama_url,
				&config.embedding_model,
				config.max_embed_chars,
			)))
		}
		EmbeddingBackend::OpenAi => {
			Ok(Box::new(openai::OpenAiClient::new(
				&config.openai.url,
				&config.embedding_model,
				config.max_embed_chars,
				&config.openai.oauth2_token_url,
				&config.openai.oauth2_scope,
			)?))
		}
	}
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
	embedding.iter()
		.flat_map(|f| f.to_le_bytes())
		.collect()
}

pub fn bytes_to_embedding(bytes: &[u8]) -> Vec<f32> {
	bytes.chunks_exact(4)
		.map(|chunk| {
			let arr: [u8; 4] = chunk.try_into().unwrap();
			f32::from_le_bytes(arr)
		})
		.collect()
}

pub fn truncate_text(text: &str, max: usize) -> &str {
	if text.len() <= max {
		return text;
	}
	let mut end = max;
	while end > 0 && !text.is_char_boundary(end) {
		end -= 1;
	}
	&text[..end]
}
