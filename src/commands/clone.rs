use std::path::{Path, PathBuf};
use std::process::Command;

use crate::commands::import;
use crate::config::AppConfig;
use crate::db::Database;

fn repository_name(url: &str) -> Option<String> {
	let trimmed = url.trim_end_matches('/');
	let segment = trimmed.rsplit(['/', ':']).next()?;
	let name = segment.strip_suffix(".git").unwrap_or(segment);
	if name.is_empty() {
		None
	} else {
		Some(name.to_string())
	}
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	url: &str,
) -> Result<(), Box<dyn std::error::Error>> {
	let name = repository_name(url)
		.ok_or_else(|| format!("cannot tell a repository name from {}", url))?;

	let root = &config.contents_path;
	std::fs::create_dir_all(root)?;
	let root = std::fs::canonicalize(root)?;
	let staging: PathBuf = root.join(format!(".sf-clone-{}", std::process::id()));
	if staging.exists() {
		return Err(format!("staging path {} already exists", staging.display()).into());
	}

	let status = Command::new("git")
		.arg("clone")
		.arg(url)
		.arg(&staging)
		.status()
		.map_err(|error| format!("could not run git: {}", error))?;
	if !status.success() {
		let _ = std::fs::remove_dir_all(&staging);
		return Err(format!("git clone failed for {}", url).into());
	}

	match import::adopt(db, config, &staging, Some(&name), true) {
		Ok(Some(_)) => Ok(()),
		Ok(None) => Ok(()),
		Err(error) => {
			let resting = park(&root, &name, &staging);
			eprintln!("the clone succeeded but could not be registered");
			eprintln!(
				"it is at {}; `sf import` it once the problem is fixed",
				resting.display(),
			);
			Err(error)
		}
	}
}

fn park(root: &Path, name: &str, staging: &Path) -> PathBuf {
	let resting = root.join(name);
	if !resting.exists() && std::fs::rename(staging, &resting).is_ok() {
		resting
	} else {
		staging.to_path_buf()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn names_come_from_the_last_segment() {
		assert_eq!(repository_name("https://github.com/myersm0/sf.git").as_deref(), Some("sf"));
		assert_eq!(repository_name("https://github.com/myersm0/sf").as_deref(), Some("sf"));
		assert_eq!(repository_name("https://github.com/myersm0/sf/").as_deref(), Some("sf"));
		assert_eq!(repository_name("git@github.com:myersm0/sf.git").as_deref(), Some("sf"));
		assert_eq!(repository_name("git@github.com:sf.git").as_deref(), Some("sf"));
		assert_eq!(repository_name("/srv/git/sf.git").as_deref(), Some("sf"));
	}

	#[test]
	fn dots_inside_a_name_are_kept() {
		assert_eq!(repository_name("https://example.com/x/sf.rs.git").as_deref(), Some("sf.rs"));
		assert_eq!(repository_name("https://example.com/x/sf.rs").as_deref(), Some("sf.rs"));
	}

	#[test]
	fn a_nameless_url_is_rejected() {
		assert_eq!(repository_name(""), None);
		assert_eq!(repository_name("/"), None);
	}
}
