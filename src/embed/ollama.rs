use serde::{Deserialize, Serialize};

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
	base_url: String,
	model: String,
	max_chars: usize,
}

impl OllamaClient {
	pub fn new(base_url: &str, model: &str, max_chars: usize) -> Self {
		Self {
			base_url: base_url.trim_end_matches('/').to_string(),
			model: model.to_string(),
			max_chars,
		}
	}

	pub fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
		let truncated = truncate_text(text, self.max_chars);
		let url = format!("{}/api/embed", self.base_url);
		let request = EmbedRequest {
			model: &self.model,
			input: truncated,
		};
		let result = ureq::post(&url)
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
			Err(e) => Err(e.into()),
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
