use std::time::Duration;

use serde::{Deserialize, Serialize};
use ureq::{Agent, AgentBuilder};

use crate::config::AppConfig;

#[derive(Serialize)]
struct EmbedRequest<'a> {
	model: &'a str,
	input: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
	embeddings: Vec<Vec<f32>>,
}

pub struct OllamaClient {
	agent: Agent,
	base_url: String,
	model: String,
	max_chars: usize,
}

impl OllamaClient {
	pub fn from_config(config: &AppConfig) -> Self {
		let agent = AgentBuilder::new()
			.timeout_connect(Duration::from_secs(5))
			.timeout(Duration::from_secs(config.ollama_timeout_seconds))
			.build();
		Self {
			agent,
			base_url: config.ollama_url.trim_end_matches('/').to_string(),
			model: config.embedding_model.clone(),
			max_chars: config.max_embed_chars,
		}
	}

	pub fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
		let truncated = truncate_text(text, self.max_chars);
		let url = format!("{}/api/embed", self.base_url);
		let request = EmbedRequest {
			model: &self.model,
			input: truncated,
		};
		let result = self.agent.post(&url)
			.send_json(serde_json::to_value(&request)?);
		match result {
			Ok(response) => {
				let body: EmbedResponse = response.into_json()?;
				body.embeddings.into_iter().next()
					.ok_or_else(|| "empty embedding response".into())
			}
			Err(ureq::Error::Status(code, response)) => {
				let body = response.into_string().unwrap_or_default();
				Err(format!("ollama returned {}: {}", code, body).into())
			}
			Err(error) => Err(format!("ollama request to {} failed: {}", url, error).into()),
		}
	}
}

fn truncate_text(text: &str, max: usize) -> &str {
	if text.len() <= max {
		return text;
	}
	let mut end = max;
	while end > 0 && !text.is_char_boundary(end) {
		end -= 1;
	}
	&text[..end]
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
