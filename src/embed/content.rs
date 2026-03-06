use std::path::Path;

use sha2::{Digest, Sha256};

use crate::meta::DirMeta;

pub struct GatheredContent {
	pub text: String,
	pub has_docs: bool,
}

pub fn gather_text(dir: &Path, meta: &DirMeta) -> GatheredContent {
	let mut parts = vec![meta.purpose.clone()];
	let mut has_docs = false;

	if meta.index.is_empty() {
		let readme_path = dir.join("README.md");
		if let Ok(contents) = std::fs::read_to_string(&readme_path) {
			parts.push(contents);
			has_docs = true;
		}
	} else {
		for filename in &meta.index {
			let file_path = dir.join(filename);
			if let Ok(contents) = std::fs::read_to_string(&file_path) {
				parts.push(contents);
				has_docs = true;
			}
		}
	}

	GatheredContent {
		text: parts.join("\n\n"),
		has_docs,
	}
}

pub fn compute_content_hash(text: &str) -> String {
	let mut hasher = Sha256::new();
	hasher.update(text.as_bytes());
	format!("{:x}", hasher.finalize())
}
