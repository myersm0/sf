use std::io::{self, Write};
use std::path::PathBuf;

use chrono::Local;
use rand::Rng;

use crate::config::{AppConfig, expand_tilde};
use crate::db::Database;
use crate::meta::DirMeta;

fn generate_key() -> String {
	let mut rng = rand::thread_rng();
	let value: u32 = rng.gen_range(0..0x1000000);
	format!("{:06x}", value)
}

fn generate_unique_key(db: &Database, root: &PathBuf) -> Result<String, Box<dyn std::error::Error>> {
	for _ in 0..100 {
		let key = generate_key();
		let dir_path = root.join(&key);
		if !db.key_exists(&key)? && !dir_path.exists() {
			return Ok(key);
		}
	}
	Err("failed to generate a unique key after 100 attempts".into())
}

fn prompt(label: &str) -> io::Result<String> {
	eprint!("{}: ", label);
	io::stderr().flush()?;
	let mut input = String::new();
	io::stdin().read_line(&mut input)?;
	Ok(input.trim().to_string())
}

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
	let key = generate_unique_key(db, &root)?;
	let created = Local::now().format("%Y-%m-%d").to_string();

	let purpose = match purpose {
		Some(p) => p,
		None => {
			let p = prompt("purpose")?;
			if p.is_empty() {
				return Err("purpose is required".into());
			}
			p
		}
	};

	let author = author
		.or_else(|| {
			if !config.default_author.is_empty() {
				Some(config.default_author.clone())
			} else {
				None
			}
		})
		.unwrap_or_else(|| {
			prompt("author").unwrap_or_default()
		});

	let tags = tags.unwrap_or_else(|| {
		let input = prompt("tags (comma-separated)").unwrap_or_default();
		if input.is_empty() {
			Vec::new()
		} else {
			input.split(',').map(|s| s.trim().to_string()).collect()
		}
	});

	let dir_path = root.join(&key);
	std::fs::create_dir_all(&dir_path)?;

	let meta = DirMeta {
		created: created.clone(),
		purpose: purpose.clone(),
		author: author.clone(),
		tags: tags.clone(),
		index: Vec::new(),
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
