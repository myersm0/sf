use std::path::{Path, PathBuf};

use crate::config::{AppConfig, expand_tilde};
use crate::db::Database;
use crate::meta::{validate, DirMeta};

fn resolve_target(
	config: &AppConfig,
	target: &str,
) -> Result<(String, PathBuf), Box<dyn std::error::Error>> {
	let as_given = expand_tilde(Path::new(target));
	let path = if as_given.is_dir() {
		as_given
	} else {
		let in_contents = config.contents_path.join(target);
		if !in_contents.is_dir() {
			return Err(format!(
				"no directory at {} or {}", as_given.display(), in_contents.display(),
			).into());
		}
		in_contents
	};
	let label = path.file_name()
		.map(|name| name.to_string_lossy().to_string())
		.unwrap_or_else(|| path.display().to_string());
	Ok((label, path))
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	target: Option<String>,
) -> Result<bool, Box<dyn std::error::Error>> {
	let targets: Vec<(String, PathBuf)> = match target {
		Some(target) => vec![resolve_target(config, &target)?],
		None => {
			let mut keys = db.get_all_keys()?;
			keys.sort();
			let mut found = Vec::new();
			for key in keys {
				match db.resolve_key(&key)? {
					Some(path) => found.push((key, path)),
					None => eprintln!("  skip {}: no accessible location", key),
				}
			}
			found
		}
	};

	let mut errors = 0;
	let mut warnings = 0;

	for (label, path) in &targets {
		let findings = match DirMeta::load(path, &config.meta_filenames) {
			Ok(meta) => validate::check_in_directory(&meta, path),
			Err(error) => vec![validate::Finding::error(error.to_string())],
		};
		if findings.is_empty() {
			continue;
		}
		eprintln!("  {}", label);
		for finding in &findings {
			eprintln!("    {}: {}", finding.severity.label(), finding.message);
			match finding.severity {
				validate::Severity::Error => errors += 1,
				validate::Severity::Warning => warnings += 1,
				validate::Severity::Note => {}
			}
		}
		eprintln!();
	}

	eprintln!(
		"validate: {} checked, {} error{}, {} warning{}",
		targets.len(),
		errors,
		if errors == 1 { "" } else { "s" },
		warnings,
		if warnings == 1 { "" } else { "s" },
	);

	Ok(errors == 0)
}
