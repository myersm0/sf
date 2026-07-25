use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EmbeddingBackend {
	#[default]
	Ollama,
	#[serde(alias = "openai")]
	OpenAi,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct OpenAiConfig {
	pub url: String,
	pub oauth2_token_url: String,
	pub oauth2_scope: String,
	pub timeout_seconds: u64,
}

impl Default for OpenAiConfig {
	fn default() -> Self {
		Self {
			url: String::new(),
			oauth2_token_url: String::new(),
			oauth2_scope: String::new(),
			timeout_seconds: 120,
		}
	}
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
	pub contents_path: PathBuf,
	pub db_path: PathBuf,
	pub embedding_backend: EmbeddingBackend,
	pub ollama_url: String,
	pub ollama_timeout_seconds: u64,
	pub embedding_model: String,
	pub coaccess_window: usize,
	pub max_embed_chars: usize,
	pub min_similarity: f32,
	pub max_search_results: usize,
	pub warn_no_docs: bool,
	pub default_author: String,
	pub meta_filenames: Vec<String>,
	pub backup_locations: Vec<PathBuf>,
	pub openai: OpenAiConfig,
}

impl Default for AppConfig {
	fn default() -> Self {
		let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
		let data_dir = dirs::data_dir()
			.unwrap_or_else(|| home.join(".local").join("share"));
		Self {
			contents_path: home.join("contents"),
			db_path: data_dir.join("sf").join("sf.db"),
			embedding_backend: EmbeddingBackend::default(),
			ollama_url: "http://localhost:11434".to_string(),
			ollama_timeout_seconds: 120,
			embedding_model: "qwen3-embedding".to_string(),
			coaccess_window: 3,
			max_embed_chars: 6000,
			min_similarity: 0.5,
			max_search_results: 15,
			warn_no_docs: true,
			default_author: String::new(),
			meta_filenames: vec![
				".meta.json".to_string(),
				".meta".to_string(),
			],
			backup_locations: Vec::new(),
			openai: OpenAiConfig::default(),
		}
	}
}

pub fn expand_tilde(path: &Path) -> PathBuf {
	let Some(text) = path.to_str() else {
		return path.to_path_buf();
	};
	let Some(home) = dirs::home_dir() else {
		return path.to_path_buf();
	};
	if text == "~" {
		return home;
	}
	if let Some(rest) = text.strip_prefix("~/") {
		return home.join(rest);
	}
	path.to_path_buf()
}

impl AppConfig {
	pub fn config_path() -> PathBuf {
		dirs::config_dir()
			.unwrap_or_else(|| PathBuf::from("."))
			.join("sf")
			.join("config.toml")
	}

	pub fn load() -> Result<Self, Box<dyn std::error::Error>> {
		let path = Self::config_path();
		let mut config = match std::fs::read_to_string(&path) {
			Ok(contents) => toml::from_str::<Self>(&contents).map_err(|error| {
				format!("failed to parse {}: {}", path.display(), error)
			})?,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
			Err(error) => {
				return Err(format!("failed to read {}: {}", path.display(), error).into());
			}
		};
		config.contents_path = expand_tilde(&config.contents_path);
		config.db_path = expand_tilde(&config.db_path);
		config.backup_locations = config.backup_locations.iter()
			.map(|location| expand_tilde(location))
			.collect();
		if config.meta_filenames.is_empty() {
			return Err("meta_filenames must contain at least one filename".into());
		}
		Ok(config)
	}
}
