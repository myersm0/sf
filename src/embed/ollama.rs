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
}

impl OllamaClient {
	pub fn new(base_url: &str, model: &str) -> Self {
		Self {
			base_url: base_url.trim_end_matches('/').to_string(),
			model: model.to_string(),
		}
	}

	pub fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
		let url = format!("{}/api/embed", self.base_url);
		let request = EmbedRequest {
			model: &self.model,
			input: text,
		};
		let response: EmbedResponse = ureq::post(&url)
			.send_json(serde_json::to_value(&request)?)?
			.into_json()?;
		response.embeddings.into_iter().next()
			.ok_or_else(|| "empty embedding response".into())
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
