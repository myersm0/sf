use std::cell::RefCell;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use ureq::{Agent, AgentBuilder};

use crate::config::AppConfig;

use super::{EmbeddingClient, truncate_text};

#[derive(Serialize)]
struct EmbedRequest<'a> {
	model: &'a str,
	input: &'a str,
}

#[derive(Deserialize)]
struct EmbedResponse {
	data: Vec<EmbedData>,
}

#[derive(Deserialize)]
struct EmbedData {
	embedding: Vec<f32>,
}

#[derive(Deserialize)]
struct TokenResponse {
	access_token: String,
	expires_in: u64,
}

struct CachedToken {
	access_token: String,
	expires_at: Instant,
}

pub struct OpenAiClient {
	agent: Agent,
	base_url: String,
	model: String,
	max_chars: usize,
	token_url: String,
	client_id: String,
	client_secret: String,
	scope: String,
	cached_token: RefCell<Option<CachedToken>>,
}

impl OpenAiClient {
	pub fn from_config(config: &AppConfig) -> Result<Self, Box<dyn std::error::Error>> {
		let client_id = std::env::var("SF_OAUTH2_CLIENT_ID")
			.map_err(|_| "SF_OAUTH2_CLIENT_ID env var not set")?;
		let client_secret = std::env::var("SF_OAUTH2_CLIENT_SECRET")
			.map_err(|_| "SF_OAUTH2_CLIENT_SECRET env var not set")?;
		let agent = AgentBuilder::new()
			.timeout_connect(Duration::from_secs(5))
			.timeout(Duration::from_secs(config.openai.timeout_seconds))
			.build();
		Ok(Self {
			agent,
			base_url: config.openai.url.trim_end_matches('/').to_string(),
			model: config.embedding_model.clone(),
			max_chars: config.max_embed_chars,
			token_url: config.openai.oauth2_token_url.clone(),
			client_id,
			client_secret,
			scope: config.openai.oauth2_scope.clone(),
			cached_token: RefCell::new(None),
		})
	}

	fn get_token(&self) -> Result<String, Box<dyn std::error::Error>> {
		if let Some(ref cached) = *self.cached_token.borrow() {
			if Instant::now() < cached.expires_at {
				return Ok(cached.access_token.clone());
			}
		}

		let form_body = format!(
			"client_id={}&client_secret={}&scope={}&grant_type=client_credentials",
			urlencoding::encode(&self.client_id),
			urlencoding::encode(&self.client_secret),
			urlencoding::encode(&self.scope),
		);
		let response = self.agent.post(&self.token_url)
			.set("Content-Type", "application/x-www-form-urlencoded")
			.send_string(&form_body)
			.map_err(|error| format!("token request to {} failed: {}", self.token_url, error))?;
		let token_response: TokenResponse = response.into_json()?;
		let margin = Duration::from_secs(60);
		let expires_at = Instant::now() + Duration::from_secs(token_response.expires_in) - margin;

		let access_token = token_response.access_token.clone();
		*self.cached_token.borrow_mut() = Some(CachedToken {
			access_token: token_response.access_token,
			expires_at,
		});
		Ok(access_token)
	}
}

impl EmbeddingClient for OpenAiClient {
	fn embed(&self, text: &str) -> Result<Vec<f32>, Box<dyn std::error::Error>> {
		let truncated = truncate_text(text, self.max_chars);
		let token = self.get_token()?;
		let url = &self.base_url;
		let request = EmbedRequest {
			model: &self.model,
			input: truncated,
		};
		let result = self.agent.post(url)
			.set("Authorization", &format!("Bearer {}", token))
			.send_json(serde_json::to_value(&request)?);
		match result {
			Ok(response) => {
				let body: EmbedResponse = response.into_json()?;
				body.data.into_iter().next()
					.map(|d| d.embedding)
					.ok_or_else(|| "empty embedding response".into())
			}
			Err(ureq::Error::Status(code, response)) => {
				let body = response.into_string().unwrap_or_default();
				Err(format!("openai returned {}: {}", code, body).into())
			}
			Err(error) => Err(format!("openai request to {} failed: {}", url, error).into()),
		}
	}
}
