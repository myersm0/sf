use std::path::PathBuf;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
	pub contents_path: PathBuf,
	pub db_path: PathBuf,
	pub ollama_url: String,
	pub embedding_model: String,
	pub coaccess_window: usize,
	pub max_embed_chars: usize,
	pub min_similarity: f32,
	pub max_search_results: usize,
	pub default_author: String,
	pub meta_filenames: Vec<String>,
	pub backup_locations: Vec<PathBuf>,
}

impl Default for AppConfig {
	fn default() -> Self {
		let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
		let data_dir = dirs::data_dir()
			.unwrap_or_else(|| home.join(".local").join("share"));
		Self {
			contents_path: home.join("contents"),
			db_path: data_dir.join("sf").join("sf.db"),
			ollama_url: "http://localhost:11434".to_string(),
			embedding_model: "qwen3-embedding".to_string(),
			coaccess_window: 3,
			max_embed_chars: 6000,
			min_similarity: 0.5,
			max_search_results: 15,
			default_author: String::new(),
			meta_filenames: vec![
				".meta.json".to_string(),
				".meta".to_string(),
			],
			backup_locations: Vec::new(),
		}
	}
}

impl AppConfig {
	pub fn config_path() -> PathBuf {
		dirs::config_dir()
			.unwrap_or_else(|| PathBuf::from("."))
			.join("sf")
			.join("config.toml")
	}

	pub fn load() -> Self {
		let path = Self::config_path();
		if let Ok(contents) = std::fs::read_to_string(&path) {
			toml::from_str(&contents).unwrap_or_default()
		} else {
			Self::default()
		}
	}
}
