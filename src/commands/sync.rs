use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, OllamaClient};
use crate::meta::DirMeta;

pub fn run(db: &Database, config: &AppConfig) -> Result<(), Box<dyn std::error::Error>> {
	let client = OllamaClient::new(&config.ollama_url, &config.embedding_model);
	let rows = db.get_all_directories()?;

	let mut updated = 0;
	let mut skipped = 0;
	let mut errors = 0;

	for row in &rows {
		let dir_path = config.contents_path.join(&row.key);
		if !dir_path.exists() {
			eprintln!("  skip {}: directory not found", row.key);
			skipped += 1;
			continue;
		}

		let meta = match DirMeta::load(&dir_path) {
			Ok(m) => m,
			Err(e) => {
				eprintln!("  skip {}: {}", row.key, e);
				errors += 1;
				continue;
			}
		};

		let text = embed::gather_text(&dir_path, &meta);
		let hash = embed::compute_content_hash(&text);

		let stored_hash = db.get_content_hash(&row.key)?;
		if stored_hash.as_deref() == Some(&hash) {
			skipped += 1;
			continue;
		}

		eprint!("  embed {}...", row.key);
		match client.embed(&text) {
			Ok(embedding) => {
				let bytes = embed::embedding_to_bytes(&embedding);
				db.set_embedding(&row.key, &bytes, &hash)?;
				eprintln!(" ok");
				updated += 1;
			}
			Err(e) => {
				eprintln!(" error: {}", e);
				errors += 1;
			}
		}
	}

	eprintln!(
		"sync complete: {} updated, {} unchanged, {} errors",
		updated, skipped, errors,
	);

	Ok(())
}
