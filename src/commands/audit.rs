use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::config::AppConfig;
use crate::db::Database;

fn scan_hex_dirs(root: &Path) -> Vec<String> {
	let hex_pattern = Regex::new(r"^[0-9a-f]{6}$").unwrap();
	let mut keys = Vec::new();
	let entries = match std::fs::read_dir(root) {
		Ok(e) => e,
		Err(_) => return keys,
	};
	for entry in entries.flatten() {
		if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
			continue;
		}
		let name = entry.file_name().to_string_lossy().to_string();
		if hex_pattern.is_match(&name) {
			keys.push(name);
		}
	}
	keys
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	mount_path: Option<PathBuf>,
) -> Result<(), Box<dyn std::error::Error>> {
	let scan_roots: Vec<PathBuf> = match mount_path {
		Some(p) => vec![p],
		None => {
			let mut roots = vec![config.contents_path.clone()];
			roots.extend(config.backup_locations.iter().cloned());
			roots
		}
	};

	let registered_keys: HashSet<String> = db.get_all_keys()?.into_iter().collect();
	let all_locations = db.get_all_locations()?;
	let mut locations_per_key: HashMap<String, Vec<String>> = HashMap::new();
	for (key, mount) in &all_locations {
		locations_per_key.entry(key.clone()).or_default().push(mount.clone());
	}

	let mut disk_presence: HashMap<String, Vec<PathBuf>> = HashMap::new();
	let mut scanned_roots = Vec::new();
	let mut skipped_roots = Vec::new();

	for root in &scan_roots {
		if !root.exists() {
			skipped_roots.push(root.clone());
			continue;
		}
		scanned_roots.push(root.clone());
		let keys_on_disk = scan_hex_dirs(root);
		for key in keys_on_disk {
			disk_presence.entry(key).or_default().push(root.clone());
		}
	}

	if !skipped_roots.is_empty() {
		eprintln!("  unmounted/missing:");
		for root in &skipped_roots {
			eprintln!("    {}", root.display());
		}
		eprintln!();
	}

	let mut strays: Vec<(String, PathBuf)> = Vec::new();
	for (key, roots) in &disk_presence {
		if !registered_keys.contains(key) {
			for root in roots {
				strays.push((key.clone(), root.clone()));
			}
		}
	}

	let mut newly_registered = Vec::new();
	for (key, roots) in &disk_presence {
		if registered_keys.contains(key) {
			for root in roots {
				let mount_str = root.to_string_lossy().to_string();
				let existing = locations_per_key.get(key).cloned().unwrap_or_default();
				if !existing.contains(&mount_str) {
					db.add_location(key, &mount_str)?;
					newly_registered.push((key.clone(), mount_str));
				}
			}
		}
	}

	let mut lost: Vec<String> = Vec::new();
	let mut underprotected: Vec<(String, usize)> = Vec::new();
	for key in &registered_keys {
		let registered_locations = locations_per_key.get(key).cloned().unwrap_or_default();
		let accessible_count = registered_locations.iter()
			.filter(|mount| {
				let dir = Path::new(mount.as_str()).join(key);
				dir.exists()
			})
			.count();

		let total_known = registered_locations.len();
		if accessible_count == 0 && total_known == 0 {
			lost.push(key.clone());
		} else if total_known < 2 {
			underprotected.push((key.clone(), total_known));
		}
	}

	underprotected.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));

	let mut found_issues = false;

	if !lost.is_empty() {
		found_issues = true;
		eprintln!("  lost (no known locations):");
		for key in &lost {
			let purpose = db.get_directory(key)?
				.map(|r| r.purpose)
				.unwrap_or_default();
			eprintln!("    {}: {}", key, purpose);
		}
		eprintln!();
	}

	if !underprotected.is_empty() {
		found_issues = true;
		eprintln!("  underprotected (fewer than 2 locations):");
		for (key, count) in &underprotected {
			let purpose = db.get_directory(key)?
				.map(|r| r.purpose)
				.unwrap_or_default();
			eprintln!("    {}: {} ({} location{})", key, purpose, count, if *count == 1 { "" } else { "s" });
		}
		eprintln!();
	}

	if !strays.is_empty() {
		found_issues = true;
		eprintln!("  strays (on disk but not registered):");
		for (key, root) in &strays {
			eprintln!("    {} at {}", key, root.display());
		}
		eprintln!();
	}

	if !newly_registered.is_empty() {
		eprintln!("  newly registered locations:");
		for (key, mount) in &newly_registered {
			eprintln!("    {} -> {}", key, mount);
		}
		eprintln!();
	}

	eprintln!(
		"audit: {} registered, {} scanned, {} lost, {} underprotected, {} strays, {} new locations",
		registered_keys.len(),
		scanned_roots.len(),
		lost.len(),
		underprotected.len(),
		strays.len(),
		newly_registered.len(),
	);

	if !found_issues && newly_registered.is_empty() {
		eprintln!("  all clear");
	}

	Ok(())
}
