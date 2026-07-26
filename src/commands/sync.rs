use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, EmbeddingClient, EmbeddingIdentity};
use crate::meta::DirMeta;

pub fn run(
	db: &Database,
	config: &AppConfig,
	client: &dyn EmbeddingClient,
	force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
	let rows = db.get_all_directories()?;
	let identity = EmbeddingIdentity::from_config(config);

	let mut updated = 0;
	let mut skipped = 0;
	let mut errors = 0;
	let mut reconciled = 0;
	let mut no_docs = Vec::new();

	for row in &rows {
		let dir_path = match db.resolve_key(&row.key)? {
			Some(p) => p,
			None => {
				eprintln!("  skip {}: no accessible location", row.key);
				skipped += 1;
				continue;
			}
		};

		let meta = match DirMeta::load(&dir_path, &config.meta_filenames) {
			Ok(m) => m,
			Err(e) => {
				eprintln!("  skip {}: {}", row.key, e);
				errors += 1;
				continue;
			}
		};

		let stale_metadata = meta.created != row.created
			|| meta.purpose != row.purpose
			|| meta.author != row.author
			|| meta.tags != row.tags();
		if stale_metadata {
			db.update_directory(&row.key, &meta.created, &meta.purpose, &meta.author, &meta.tags)?;
			eprintln!("  reconcile {}: metadata changed on disk", row.key);
			reconciled += 1;
		}

		let content = embed::gather_text(&dir_path, &meta);
		let hash = embed::compute_content_hash(&content.text);

		if !content.has_docs {
			no_docs.push(row.key.clone());
			if row.has_docs {
				db.set_has_docs(&row.key, false)?;
			}
			if !force && config.warn_no_docs {
				eprintln!("  skip {}: no docs (use --force to embed anyway)", row.key);
				skipped += 1;
				continue;
			}
		}

		let state = db.get_embedding_state(&row.key)?;
		if state.is_current(&hash, &identity) {
			skipped += 1;
			continue;
		}

		eprint!("  embed {} ({} chars)...", row.key, content.text.len());
		match client.embed(&content.text) {
			Ok(embedding) => {
				let bytes = embed::embedding_to_bytes(&embedding);
				db.set_embedding(&row.key, &bytes, &identity, &hash, content.has_docs)?;
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
		"sync complete: {} embedded, {} unchanged, {} reconciled, {} errors",
		updated, skipped, reconciled, errors,
	);

	Ok(())
}
