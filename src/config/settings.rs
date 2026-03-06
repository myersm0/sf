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
	pub default_author: String,
	pub backup_locations: Vec<PathBuf>,
}

impl Default for AppConfig {
	fn default() -> Self {
		let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
		Self {
			contents_path: home.join("contents"),
			db_path: home.join(".clew").join("clew.db"),
			ollama_url: "http://localhost:11434".to_string(),
			embedding_model: "nomic-embed-text".to_string(),
			coaccess_window: 3,
			default_author: String::new(),
			backup_locations: Vec::new(),
		}
	}
}

impl AppConfig {
	pub fn config_path() -> PathBuf {
		dirs::config_dir()
			.unwrap_or_else(|| PathBuf::from("."))
			.join("clew")
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
