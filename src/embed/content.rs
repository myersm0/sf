use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::meta::DirMeta;

fn find_readme(dir: &Path) -> Option<PathBuf> {
	let entries = std::fs::read_dir(dir).ok()?;
	for entry in entries.flatten() {
		let name = entry.file_name();
		let lower = name.to_string_lossy().to_lowercase();
		if lower == "readme.md" || lower == "readme" || lower == "readme.txt" {
			return Some(entry.path());
		}
	}
	None
}

pub struct GatheredContent {
	pub text: String,
	pub has_docs: bool,
}

pub fn gather_text(dir: &Path, meta: &DirMeta) -> GatheredContent {
	let mut parts = vec![meta.purpose.clone()];
	let mut has_docs = false;

	if meta.index.is_empty() {
		if let Some(readme) = find_readme(dir) {
			if let Ok(contents) = std::fs::read_to_string(&readme) {
				parts.push(contents);
				has_docs = true;
			}
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
