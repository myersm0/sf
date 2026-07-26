use std::path::Path;

use rand::Rng;

use crate::db::Database;

#[allow(non_upper_case_globals)]
pub const key_length: usize = 6;

pub fn is_valid(name: &str) -> bool {
	name.len() == key_length
		&& name.bytes().all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn generate() -> String {
	let ceiling = 16u32.pow(key_length as u32);
	let value: u32 = rand::thread_rng().gen_range(0..ceiling);
	format!("{:0width$x}", value, width = key_length)
}

pub fn generate_unique(db: &Database, root: &Path) -> Result<String, Box<dyn std::error::Error>> {
	for _ in 0..100 {
		let key = generate();
		if !db.key_exists(&key)? && !root.join(&key).exists() {
			return Ok(key);
		}
	}
	Err("failed to generate a unique key after 100 attempts".into())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn well_formed_keys_are_accepted() {
		assert!(is_valid("a3f1c2"));
		assert!(is_valid("000000"));
		assert!(is_valid("301018"));
	}

	#[test]
	fn english_words_can_be_well_formed_keys() {
		assert!(is_valid("decade"));
		assert!(is_valid("facade"));
		assert!(is_valid("deface"));
	}

	#[test]
	fn malformed_keys_are_rejected() {
		assert!(!is_valid("A3F1C2"));
		assert!(!is_valid("a3f1c"));
		assert!(!is_valid("a3f1c22"));
		assert!(!is_valid("a3f1g2"));
		assert!(!is_valid(""));
		assert!(!is_valid("a3f1c\u{e9}"));
	}

	#[test]
	fn generated_keys_are_well_formed() {
		for _ in 0..1000 {
			assert!(is_valid(&generate()));
		}
	}
}
