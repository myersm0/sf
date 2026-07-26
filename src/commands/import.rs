use std::path::{Path, PathBuf};

use chrono::Local;

use crate::config::{AppConfig, expand_tilde};
use crate::db::Database;
use crate::embed;
use crate::keys;
use crate::meta::{validate, DirMeta};
use crate::prompt;

fn locate(
	config: &AppConfig,
	target: &str,
	path: Option<PathBuf>,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
	let as_given = expand_tilde(Path::new(target));
	if as_given.is_dir() {
		return Ok(std::fs::canonicalize(as_given)?);
	}
	let root = expand_tilde(&path.unwrap_or_else(|| config.contents_path.clone()));
	let in_root = root.join(target);
	if in_root.is_dir() {
		return Ok(std::fs::canonicalize(in_root)?);
	}
	Err(format!("no directory at {} or {}", as_given.display(), in_root.display()).into())
}

fn first_paragraph(text: &str) -> Option<String> {
	for block in text.split("\n\n") {
		let prose: Vec<&str> = block.lines()
			.map(|line| line.trim())
			.filter(|line| {
				!line.is_empty()
					&& !line.starts_with('#')
					&& !line.starts_with("![")
					&& !line.starts_with("[!")
					&& !line.starts_with("```")
			})
			.collect();
		if !prose.is_empty() {
			return Some(prose.join(" "));
		}
	}
	None
}

fn describe(dir: &Path) -> Option<String> {
	let readme = embed::find_readme(dir)?;
	let text = std::fs::read_to_string(readme).ok()?;
	first_paragraph(&text)
}

fn read_metadata(
	dir: &Path,
	config: &AppConfig,
) -> Result<Option<DirMeta>, Box<dyn std::error::Error>> {
	if DirMeta::find_meta_path(dir, &config.meta_filenames).is_none() {
		return Ok(None);
	}
	let meta = DirMeta::load(dir, &config.meta_filenames)
		.map_err(|error| format!("{}: {}", dir.display(), error))?;
	Ok(Some(meta))
}

fn report(findings: &[validate::Finding]) -> Result<(), Box<dyn std::error::Error>> {
	for finding in findings {
		eprintln!("  {}: {}", finding.severity.label(), finding.message);
	}
	if validate::has_errors(findings) {
		return Err("metadata has errors; fix them and try again".into());
	}
	Ok(())
}

fn compose_metadata(
	config: &AppConfig,
	dir: &Path,
	name: &str,
) -> Result<DirMeta, Box<dyn std::error::Error>> {
	if !prompt::is_interactive() {
		return Err(format!(
			"no metadata file in {}, and stdin is not a terminal to prompt for one",
			dir.display(),
		).into());
	}

	let suggestion = describe(dir).unwrap_or_else(|| name.to_string());
	let purpose = prompt::ask_with_default("purpose", embed::truncate_text(&suggestion, 200))?;
	if purpose.trim().is_empty() {
		return Err("purpose is required".into());
	}

	let author = match config.resolve_author() {
		Some(author) => author,
		None => prompt::ask("author")?,
	};

	let entered = prompt::ask("tags (comma-separated)")?;
	let tags = if entered.is_empty() {
		Vec::new()
	} else {
		entered.split(',').map(|tag| tag.trim().to_string()).collect()
	};

	Ok(DirMeta {
		created: Local::now().format("%Y-%m-%d").to_string(),
		purpose,
		author,
		tags,
		index: Vec::new(),
		source_name: None,
		extra: serde_json::Map::new(),
	})
}

fn register_location(
	db: &Database,
	key: &str,
	root: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
	let location = root.to_string_lossy().to_string();
	if db.get_locations(key)?.contains(&location) {
		eprintln!("{} is already registered at {}", key, location);
		return Ok(());
	}
	db.add_location(key, &location)?;
	eprintln!("registered another location for {}: {}", key, location);
	Ok(())
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	target: &str,
	path: Option<PathBuf>,
	force: bool,
) -> Result<(), Box<dyn std::error::Error>> {
	let dir_path = locate(config, target, path)?;
	let root = dir_path.parent()
		.ok_or_else(|| format!("{} has no parent directory", dir_path.display()))?
		.to_path_buf();
	let name = dir_path.file_name()
		.map(|name| name.to_string_lossy().to_string())
		.ok_or_else(|| format!("{} has no directory name", dir_path.display()))?;

	if keys::is_valid(&name) && db.key_exists(&name)? {
		return register_location(db, &name, &root);
	}

	let found = read_metadata(&dir_path, config)?;
	let was_absent = found.is_none();
	if let Some(meta) = &found {
		report(&validate::check_in_directory(meta, &dir_path))?;
	}

	let contents_root = std::fs::canonicalize(&config.contents_path)
		.unwrap_or_else(|_| config.contents_path.clone());

	let fresh_key = if keys::is_valid(&name) {
		None
	} else {
		if root != contents_root {
			return Err(format!(
				"{} is not named with a key, and sf only renames inside the contents root ({}); move it there first",
				dir_path.display(), contents_root.display(),
			).into());
		}
		let key = keys::generate_unique(db, &root)?;
		let question = format!("rename {} to {} and register it?", name, key);
		if !force && !prompt::confirm_default_no(&question)? {
			eprintln!("cancelled");
			return Ok(());
		}
		Some(key)
	};

	let mut meta = match found {
		Some(meta) => meta,
		None => compose_metadata(config, &dir_path, &name)?,
	};

	let (key, final_path) = match &fresh_key {
		Some(key) => {
			let renamed_path = root.join(key);
			std::fs::rename(&dir_path, &renamed_path)?;
			meta.source_name = Some(name.clone());
			(key.clone(), renamed_path)
		}
		None => (name.clone(), dir_path.clone()),
	};

	if fresh_key.is_some() || was_absent {
		meta.save(&final_path, &config.meta_filenames)?;
	}

	db.insert_directory(&key, &meta)?;
	db.add_location(&key, &root.to_string_lossy())?;

	match &fresh_key {
		Some(_) => eprintln!("imported {} as {} ({})", name, key, meta.purpose),
		None => eprintln!("imported {} ({})", key, meta.purpose),
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn headings_and_badges_are_skipped() {
		let readme = "# sf\n[![CI](https://example.com/badge.svg)](https://example.com)\n\nA CLI tool for managing directories.\n\nMore detail here.\n";
		assert_eq!(
			first_paragraph(readme).as_deref(),
			Some("A CLI tool for managing directories."),
		);
	}

	#[test]
	fn wrapped_lines_join_into_one_paragraph() {
		let readme = "Managing a flat hierarchy\nof directories, each with a key.\n\nSecond paragraph.\n";
		assert_eq!(
			first_paragraph(readme).as_deref(),
			Some("Managing a flat hierarchy of directories, each with a key."),
		);
	}

	#[test]
	fn a_readme_with_only_a_heading_yields_nothing() {
		assert_eq!(first_paragraph("# title\n"), None);
		assert_eq!(first_paragraph(""), None);
	}
}
