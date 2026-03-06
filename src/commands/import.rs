use crate::config::AppConfig;
use crate::db::Database;
use crate::meta::DirMeta;

pub fn run(
	db: &Database,
	config: &AppConfig,
	key: &str,
) -> Result<(), Box<dyn std::error::Error>> {
	let dir_path = config.contents_path.join(key);
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
	let location = config.contents_path.to_string_lossy().to_string();
	db.add_location(key, &location)?;

	eprintln!("imported {} ({})", key, meta.purpose);

	Ok(())
}
