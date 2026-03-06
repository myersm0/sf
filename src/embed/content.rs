use std::path::Path;

use sha2::{Digest, Sha256};

use crate::meta::DirMeta;

pub fn gather_text(dir: &Path, meta: &DirMeta) -> String {
	let mut parts = vec![meta.purpose.clone()];

	let readme_path = dir.join("README.md");
	if readme_path.exists() {
		if let Ok(contents) = std::fs::read_to_string(&readme_path) {
			parts.push(contents);
		}
	}

	for filename in &meta.index {
		let file_path = dir.join(filename);
		if file_path.exists() {
			if let Ok(contents) = std::fs::read_to_string(&file_path) {
				parts.push(contents);
			}
		}
	}

	parts.join("\n\n")
}

pub fn compute_content_hash(text: &str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(text.as_bytes());
	format!("{:x}", hasher.finalize())
}
