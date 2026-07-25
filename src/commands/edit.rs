use std::io::{self, Write};
use std::process::Command;

use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, EmbeddingClient};
use crate::meta::DirMeta;

fn confirm(question: &str) -> io::Result<bool> {
	eprint!("{} [Y/n] ", question);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	let answer = input.trim().to_lowercase();
	Ok(answer.is_empty() || answer == "y" || answer == "yes")
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	client: &dyn EmbeddingClient,
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

	let meta = loop {
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

		match serde_json::from_str::<DirMeta>(&after) {
			Ok(meta) => break meta,
			Err(error) => {
				eprintln!("invalid json after edit: {}", error);
				if !confirm("fix in editor?")? {
					std::fs::write(&meta_path, &before)?;
					eprintln!("restored previous metadata");
					return Ok(());
				}
			}
		}
	};

	db.update_directory(key, &meta.purpose, &meta.author, &meta.tags)?;
	eprintln!("updated metadata for {}", key);

	let content = embed::gather_text(&dir_path, &meta);
	let hash = embed::compute_content_hash(&content.text);
	let state = db.get_embedding_state(key)?;

	if !state.is_current(&hash, &config.embedding_model) {
		eprint!("  re-embedding...");
		let embedding = client.embed(&content.text)?;
		let bytes = embed::embedding_to_bytes(&embedding);
		db.set_embedding(key, &bytes, &config.embedding_model, &hash, content.has_docs)?;
		eprintln!(" ok");
	}

	Ok(())
}
