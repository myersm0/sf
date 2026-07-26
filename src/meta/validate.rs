use std::path::Path;

use chrono::NaiveDate;

use super::DirMeta;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
	Error,
	Warning,
	Note,
}

impl Severity {
	pub fn label(&self) -> &'static str {
		match self {
			Self::Error => "error",
			Self::Warning => "warning",
			Self::Note => "note",
		}
	}
}

#[derive(Debug, Clone)]
pub struct Finding {
	pub severity: Severity,
	pub message: String,
}

impl Finding {
	pub fn error(message: impl Into<String>) -> Self {
		Self { severity: Severity::Error, message: message.into() }
	}

	pub fn warning(message: impl Into<String>) -> Self {
		Self { severity: Severity::Warning, message: message.into() }
	}

	pub fn note(message: impl Into<String>) -> Self {
		Self { severity: Severity::Note, message: message.into() }
	}
}

pub fn has_errors(findings: &[Finding]) -> bool {
	findings.iter().any(|finding| finding.severity == Severity::Error)
}

fn is_iso_date(value: &str) -> bool {
	NaiveDate::parse_from_str(value, "%Y-%m-%d")
		.map(|date| date.format("%Y-%m-%d").to_string() == value)
		.unwrap_or(false)
}

pub fn check(meta: &DirMeta) -> Vec<Finding> {
	let mut findings = Vec::new();

	if !is_iso_date(&meta.created) {
		findings.push(Finding::error(format!(
			"created is `{}`, not a YYYY-MM-DD date; `search --since` compares these as text",
			meta.created,
		)));
	}
	if meta.purpose.trim().is_empty() {
		findings.push(Finding::error("purpose is empty; it is the primary search signal"));
	}
	if meta.author.trim().is_empty() {
		findings.push(Finding::warning("author is empty"));
	}
	if !meta.extra.is_empty() {
		let mut names: Vec<&str> = meta.extra.keys().map(|name| name.as_str()).collect();
		names.sort();
		findings.push(Finding::note(format!(
			"fields not in the schema, preserved as written: {}",
			names.join(", "),
		)));
	}

	findings
}

pub fn check_in_directory(meta: &DirMeta, dir: &Path) -> Vec<Finding> {
	let mut findings = check(meta);

	for filename in &meta.index {
		if !dir.join(filename).exists() {
			findings.push(Finding::warning(format!(
				"index lists `{}`, which is not in the directory",
				filename,
			)));
		}
	}

	if let Some(declared) = meta.extra.get("key").and_then(|value| value.as_str()) {
		let name = dir.file_name().map(|name| name.to_string_lossy().to_string());
		if let Some(name) = name {
			if declared != name {
				findings.push(Finding::warning(format!(
					"metadata declares key `{}` but the directory is named `{}`",
					declared, name,
				)));
			}
		}
	}

	findings
}

#[cfg(test)]
mod tests {
	use super::*;

	fn meta_from(source: &str) -> DirMeta {
		serde_json::from_str(source).unwrap()
	}

	fn sound_meta() -> DirMeta {
		meta_from(r#"{"created": "2026-03-06", "purpose": "a project", "author": "m"}"#)
	}

	fn severities(findings: &[Finding]) -> Vec<Severity> {
		findings.iter().map(|finding| finding.severity).collect()
	}

	fn temp_dir(name: &str) -> std::path::PathBuf {
		let path = std::env::temp_dir()
			.join(format!("sf_validate_{}_{}", std::process::id(), name));
		std::fs::create_dir_all(&path).unwrap();
		path
	}

	#[test]
	fn sound_metadata_yields_nothing() {
		assert!(check(&sound_meta()).is_empty());
	}

	#[test]
	fn zero_padded_iso_dates_are_the_only_accepted_form() {
		assert!(is_iso_date("2026-03-06"));
		assert!(!is_iso_date("2026-3-6"));
		assert!(!is_iso_date("March 2026"));
		assert!(!is_iso_date("2026-13-01"));
		assert!(!is_iso_date("06/03/2026"));
		assert!(!is_iso_date("2026-03-06T00:00:00Z"));
		assert!(!is_iso_date(""));
	}

	#[test]
	fn unparseable_created_is_an_error() {
		let meta = meta_from(r#"{"created": "March 2026", "purpose": "p", "author": "m"}"#);
		assert_eq!(severities(&check(&meta)), vec![Severity::Error]);
	}

	#[test]
	fn empty_purpose_is_an_error() {
		let meta = meta_from(r#"{"created": "2026-03-06", "purpose": "   ", "author": "m"}"#);
		assert_eq!(severities(&check(&meta)), vec![Severity::Error]);
	}

	#[test]
	fn empty_author_is_only_a_warning() {
		let meta = meta_from(r#"{"created": "2026-03-06", "purpose": "p", "author": ""}"#);
		assert_eq!(severities(&check(&meta)), vec![Severity::Warning]);
	}

	#[test]
	fn fields_outside_the_schema_are_noted_not_faulted() {
		let meta = meta_from(
			r#"{"created": "2026-03-06", "purpose": "p", "author": "m", "class": "A", "key": "301018"}"#
		);
		let findings = check(&meta);
		assert_eq!(severities(&findings), vec![Severity::Note]);
		assert!(findings[0].message.contains("class, key"));
	}

	#[test]
	fn missing_index_files_warn() {
		let dir = temp_dir("index");
		std::fs::write(dir.join("present.md"), "x").unwrap();
		let meta = meta_from(
			r#"{"created": "2026-03-06", "purpose": "p", "author": "m", "index": ["present.md", "absent.md"]}"#
		);
		let findings = check_in_directory(&meta, &dir);
		assert_eq!(severities(&findings), vec![Severity::Warning]);
		assert!(findings[0].message.contains("absent.md"));
		std::fs::remove_dir_all(&dir).unwrap();
	}

	#[test]
	fn declared_key_disagreeing_with_the_directory_warns() {
		let dir = temp_dir("301018");
		let name = dir.file_name().unwrap().to_string_lossy().to_string();
		let agreeing = meta_from(&format!(
			r#"{{"created": "2026-03-06", "purpose": "p", "author": "m", "key": "{}"}}"#, name
		));
		let disagreeing = meta_from(
			r#"{"created": "2026-03-06", "purpose": "p", "author": "m", "key": "a3f1c2"}"#
		);

		assert_eq!(
			severities(&check_in_directory(&agreeing, &dir)),
			vec![Severity::Note],
		);
		assert_eq!(
			severities(&check_in_directory(&disagreeing, &dir)),
			vec![Severity::Note, Severity::Warning],
		);
		std::fs::remove_dir_all(&dir).unwrap();
	}
}
