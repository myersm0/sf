use std::path::{Path, PathBuf};

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
	#[serde(flatten)]
	pub extra: serde_json::Map<String, serde_json::Value>,
}

impl DirMeta {
	pub fn find_meta_path(dir: &Path, filenames: &[String]) -> Option<PathBuf> {
		for name in filenames {
			let path = dir.join(name);
			if path.exists() {
				return Some(path);
			}
		}
		None
	}

	pub fn load(dir: &Path, filenames: &[String]) -> Result<Self, Box<dyn std::error::Error>> {
		let path = Self::find_meta_path(dir, filenames)
			.ok_or_else(|| {
				let tried: Vec<&str> = filenames.iter().map(|s| s.as_str()).collect();
				format!(
					"no metadata file found in {} (tried: {})",
					dir.display(),
					tried.join(", "),
				)
			})?;
		let contents = std::fs::read_to_string(&path)?;
		let meta: Self = serde_json::from_str(&contents)?;
		Ok(meta)
	}

	pub fn save(&self, dir: &Path, filenames: &[String]) -> Result<(), Box<dyn std::error::Error>> {
		let path = Self::find_meta_path(dir, filenames)
			.unwrap_or_else(|| dir.join(&filenames[0]));
		let json = serde_json::to_string_pretty(self)?;
		std::fs::write(&path, json)?;
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn unknown_fields_survive_roundtrip() {
		let source = r#"{
			"created": "2026-03-06",
			"purpose": "sf project",
			"author": "m",
			"key": "301018",
			"class": "A"
		}"#;
		let meta: DirMeta = serde_json::from_str(source).unwrap();
		assert_eq!(meta.extra.get("key").unwrap(), "301018");
		assert_eq!(meta.extra.get("class").unwrap(), "A");

		let serialized = serde_json::to_string_pretty(&meta).unwrap();
		let reparsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
		assert_eq!(reparsed["key"], "301018");
		assert_eq!(reparsed["class"], "A");
		assert_eq!(reparsed["purpose"], "sf project");
	}

	#[test]
	fn plain_meta_roundtrip_adds_nothing() {
		let source = r#"{"created": "2026-01-01", "purpose": "p", "author": "m", "tags": ["x"]}"#;
		let meta: DirMeta = serde_json::from_str(source).unwrap();
		assert!(meta.extra.is_empty());
		let serialized = serde_json::to_string(&meta).unwrap();
		let reparsed: serde_json::Value = serde_json::from_str(&serialized).unwrap();
		let object = reparsed.as_object().unwrap();
		assert_eq!(object.len(), 4);
	}
}
