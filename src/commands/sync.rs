use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, OllamaClient};
use crate::meta::DirMeta;

pub fn run(db: &Database, config: &AppConfig, force: bool) -> Result<(), Box<dyn std::error::Error>> {
	let client = OllamaClient::new(&config.ollama_url, &config.embedding_model, config.max_embed_chars);
	let rows = db.get_all_directories()?;

	let mut updated = 0;
	let mut skipped = 0;
	let mut errors = 0;
	let mut no_docs = Vec::new();

	for row in &rows {
		let dir_path = config.contents_path.join(&row.key);
		if !dir_path.exists() {
			eprintln!("  skip {}: directory not found", row.key);
			skipped += 1;
			continue;
		}

		let meta = match DirMeta::load(&dir_path, &config.meta_filenames) {
			Ok(m) => m,
			Err(e) => {
				eprintln!("  skip {}: {}", row.key, e);
				errors += 1;
				continue;
			}
		};

		let content = embed::gather_text(&dir_path, &meta);
		let hash = embed::compute_content_hash(&content.text);

		if !content.has_docs {
			no_docs.push(row.key.clone());
			if !force && config.warn_no_docs {
				eprintln!("  skip {}: no docs (use --force to embed anyway)", row.key);
				skipped += 1;
				continue;
			}
		}

		let stored_hash = db.get_content_hash(&row.key)?;
		if stored_hash.as_deref() == Some(&hash) {
			skipped += 1;
			continue;
		}

		eprint!("  embed {} ({} chars)...", row.key, content.text.len());
		match client.embed(&content.text) {
			Ok(embedding) => {
				let bytes = embed::embedding_to_bytes(&embedding);
				db.set_embedding(&row.key, &bytes, &hash, content.has_docs)?;
				eprintln!(" ok");
				updated += 1;
			}
			Err(e) => {
				eprintln!(" error: {}", e);
				errors += 1;
			}
		}
	}

	if !no_docs.is_empty() && !force && config.warn_no_docs {
		eprintln!(
			"\n  {} director{} without docs: {}",
			no_docs.len(),
			if no_docs.len() == 1 { "y" } else { "ies" },
			no_docs.join(", "),
		);
	}

	eprintln!(
		"sync complete: {} updated, {} unchanged, {} errors",
		updated, skipped, errors,
	);

	Ok(())
}
