use std::process::Command;

use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, OllamaClient};
use crate::meta::DirMeta;

pub fn run(
	db: &Database,
	config: &AppConfig,
	key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
	if !db.key_exists(key)? {
		return Err(format!("key not found: {}", key).into());
	}

	let dir_path = db.resolve_key(key)?
		.ok_or_else(|| format!("no accessible location for {}", key))?;
	let meta_path = DirMeta::find_meta_path(&dir_path, &config.meta_filenames)
		.ok_or_else(|| format!("no metadata file found in {}", dir_path.display()))?;

	let before = std::fs::read_to_string(&meta_path)?;

	let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vi".to_string());
	let status = Command::new(&editor)
		.arg(&meta_path)
		.status()?;

	if !status.success() {
		return Err("editor exited with non-zero status".into());
	}

	let after = std::fs::read_to_string(&meta_path)?;
	if before == after {
		eprintln!("no changes");
		return Ok(());
	}

	let meta: DirMeta = serde_json::from_str(&after).map_err(|e| {
		format!("invalid json after edit: {}", e)
	})?;

	db.update_directory(key, &meta.purpose, &meta.author, &meta.tags)?;
	eprintln!("updated metadata for {}", key);

	let content = embed::gather_text(&dir_path, &meta);
	let hash = embed::compute_content_hash(&content.text);
	let stored_hash = db.get_content_hash(key)?;

	if stored_hash.as_deref() != Some(&hash) {
		eprint!("  re-embedding...");
		let client = OllamaClient::new(&config.ollama_url, &config.embedding_model, config.max_embed_chars);
		let embedding = client.embed(&content.text)?;
		let bytes = embed::embedding_to_bytes(&embedding);
		db.set_embedding(key, &bytes, &hash, content.has_docs)?;
		eprintln!(" ok");
	}

	Ok(())
}
