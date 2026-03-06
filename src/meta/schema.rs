use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirMeta {
	pub created: String,
	pub purpose: String,
	pub author: String,
	#[serde(default)]
	pub tags: Vec<String>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub index: Vec<String>,
}

impl DirMeta {
	pub fn meta_path(dir: &Path) -> std::path::PathBuf {
		dir.join(".meta.json")
	}

	pub fn load(dir: &Path) -> Result<Self, Box<dyn std::error::Error>> {
		let path = Self::meta_path(dir);
		let contents = std::fs::read_to_string(&path)?;
		let meta: Self = serde_json::from_str(&contents)?;
		Ok(meta)
	}

	pub fn save(&self, dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
		let path = Self::meta_path(dir);
		let json = serde_json::to_string_pretty(self)?;
		std::fs::write(&path, json)?;
		Ok(())
	}
}
