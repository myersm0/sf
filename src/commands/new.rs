use std::path::PathBuf;

use chrono::Local;

use crate::config::{AppConfig, expand_tilde};
use crate::db::Database;
use crate::keys;
use crate::meta::DirMeta;
use crate::prompt;

pub fn run(
	db: &Database,
	config: &AppConfig,
	purpose: Option<String>,
	author: Option<String>,
	tags: Option<Vec<String>>,
	path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
	let root = expand_tilde(&path.unwrap_or_else(|| config.contents_path.clone()));
	std::fs::create_dir_all(&root)?;
	let root = std::fs::canonicalize(&root)?;
	let key = keys::generate_unique(db, &root)?;
	let created = Local::now().format("%Y-%m-%d").to_string();

	let purpose = match purpose {
		Some(p) => p,
		None => {
			let p = prompt::ask("purpose")?;
			if p.is_empty() {
				return Err("purpose is required".into());
			}
			p
		}
	};

	let author = match author.or_else(|| config.resolve_author()) {
		Some(author) => author,
		None => prompt::ask("author")?,
	};

	let tags = match tags {
		Some(tags) => tags,
		None => {
			let input = prompt::ask("tags (comma-separated)")?;
			if input.is_empty() {
				Vec::new()
			} else {
				input.split(',').map(|s| s.trim().to_string()).collect()
			}
		}
	};

	let dir_path = root.join(&key);
	std::fs::create_dir_all(&dir_path)?;

	let meta = DirMeta {
		created: created.clone(),
		purpose: purpose.clone(),
		author: author.clone(),
		tags: tags.clone(),
		index: Vec::new(),
		source_name: None,
		extra: serde_json::Map::new(),
	};
	meta.save(&dir_path, &config.meta_filenames)?;

	db.insert_directory(&key, &created, &purpose, &author, &tags)?;
	let location = root.to_string_lossy().to_string();
	db.add_location(&key, &location)?;

	println!("{}", dir_path.display());
	eprintln!("created {} ({})", key, purpose);

	Ok(())
}
