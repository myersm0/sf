mod content;
mod ollama;
mod openai;
mod similarity;

pub use content::{find_readme, gather_text, compute_content_hash, GatheredContent};
pub use similarity::cosine_similarity;

use crate::config::{AppConfig, EmbeddingBackend};

pub struct EmbeddingIdentity {
	pub backend: String,
	pub model: String,
}

impl EmbeddingIdentity {
	pub fn from_config(config: &AppConfig) -> Self {
		Self {
			backend: config.embedding_backend.name().to_string(),
			model: config.embedding_model.clone(),
		}
	}

	pub fn matches_stored(&self, backend: Option<&str>, model: Option<&str>) -> bool {
		backend.map_or(true, |stored| stored == self.backend)
			&& model.map_or(true, |stored| stored == self.model)
	}
}

impl std::fmt::Display for EmbeddingIdentity {
	fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(formatter, "{}/{}", self.backend, self.model)
	}
}

pub trait EmbeddingClient {
	fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>>;
}

pub fn build_client(config: &AppConfig) -> Result<Box<dyn EmbeddingClient>, Box<dyn std::error::Error>> {
	match config.embedding_backend {
		EmbeddingBackend::Ollama => {
			Ok(Box::new(ollama::OllamaClient::from_config(config)))
		}
		EmbeddingBackend::OpenAi => {
			Ok(Box::new(openai::OpenAiClient::from_config(config)?))
		}
	}
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
	embedding.iter()
		.flat_map(|f| f.to_le_bytes())
		.collect()
}

pub fn bytes_to_embedding(bytes: &[u8]) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
	if bytes.len() % 4 != 0 {
		return Err(format!(
			"corrupt embedding blob: {} bytes is not a multiple of 4", bytes.len(),
		).into());
	}
	Ok(bytes.chunks_exact(4)
		.map(|chunk| {
			let arr: [u8; 4] = chunk.try_into().unwrap();
			f32::from_le_bytes(arr)
		})
		.collect())
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

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn truncate_under_limit_is_untouched() {
		assert_eq!(truncate_text("hello", 10), "hello");
	}

	#[test]
	fn truncate_cuts_at_limit() {
		assert_eq!(truncate_text("hello world", 5), "hello");
	}

	#[test]
	fn truncate_respects_char_boundaries() {
		assert_eq!(truncate_text("a\u{e9}b", 2), "a");
	}

	#[test]
	fn embedding_bytes_roundtrip() {
		let embedding = vec![0.0f32, -1.5, 3.25];
		let bytes = embedding_to_bytes(&embedding);
		assert_eq!(bytes_to_embedding(&bytes).unwrap(), embedding);
	}

	#[test]
	fn corrupt_blob_is_rejected() {
		assert!(bytes_to_embedding(&[0, 0, 0]).is_err());
	}
}
