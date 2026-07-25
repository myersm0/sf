use std::path::PathBuf;

use crate::config::{AppConfig, expand_tilde};
use crate::db::Database;
use crate::meta::DirMeta;

pub fn run(
	db: &Database,
	config: &AppConfig,
	key: &str,
	path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
	let root = expand_tilde(&path.unwrap_or_else(|| config.contents_path.clone()));
	let root = std::fs::canonicalize(&root).map_err(|error| {
		format!("cannot resolve root {}: {}", root.display(), error)
	})?;
	let dir_path = root.join(key);
	if !dir_path.exists() {
		return Err(format!("directory not found: {}", dir_path.display()).into());
	}

	if db.key_exists(key)? {
		return Err(format!("key already registered: {}", key).into());
	}

	let meta = DirMeta::load(&dir_path, &config.meta_filenames).map_err(|e| {
		format!("{}: {}", dir_path.display(), e)
	})?;

	db.insert_directory(key, &meta.created, &meta.purpose, &meta.author, &meta.tags)?;
	let location = root.to_string_lossy().to_string();
	db.add_location(key, &location)?;

	eprintln!("imported {} ({})", key, meta.purpose);

	Ok(())
}
