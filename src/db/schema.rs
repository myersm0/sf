use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json;

#[allow(non_upper_case_globals)]
const current_schema_version: i32 = 2;

pub struct Database {
	connection: Connection,
}

impl Database {
	pub fn open(path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
		if let Some(parent) = path.parent() {
			std::fs::create_dir_all(parent)?;
		}
		let connection = Connection::open(path)?;
		connection.execute_batch("PRAGMA foreign_keys = ON;")?;
		Ok(Self { connection })
	}

	pub fn initialize(&self) -> Result<(), Box<dyn std::error::Error>> {
		let version: i32 = self.connection.query_row(
			"PRAGMA user_version", [], |row| row.get(0),
		)?;
		if version > current_schema_version {
			return Err(format!(
				"database schema version {} is newer than this build of sf supports ({}); upgrade sf",
				version, current_schema_version,
			).into());
		}
		if version < 1 {
			self.migrate_to_version_1()?;
		}
		if version < 2 {
			self.migrate_to_version_2()?;
		}
		Ok(())
	}

	fn migrate_to_version_1(&self) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute_batch(
			"CREATE TABLE IF NOT EXISTS directories (
				key TEXT PRIMARY KEY,
				created TEXT NOT NULL,
				purpose TEXT NOT NULL,
				author TEXT NOT NULL,
				tags TEXT NOT NULL DEFAULT '[]',
				embedding BLOB,
				content_hash TEXT,
				has_docs INTEGER NOT NULL DEFAULT 0
			);

			CREATE TABLE IF NOT EXISTS locations (
				key TEXT NOT NULL,
				mount_path TEXT NOT NULL,
				PRIMARY KEY (key, mount_path),
				FOREIGN KEY (key) REFERENCES directories(key)
			);

			CREATE TABLE IF NOT EXISTS visits (
				id INTEGER PRIMARY KEY AUTOINCREMENT,
				key TEXT NOT NULL,
				FOREIGN KEY (key) REFERENCES directories(key)
			);

			CREATE TABLE IF NOT EXISTS metadata (
				key TEXT PRIMARY KEY,
				value TEXT NOT NULL
			);"
		)?;
		if !self.column_exists("visits", "visited_at")? {
			self.connection.execute("ALTER TABLE visits ADD COLUMN visited_at TEXT", [])?;
		}
		self.connection.pragma_update(None, "user_version", 1)?;
		Ok(())
	}

	fn migrate_to_version_2(&self) -> Result<(), Box<dyn std::error::Error>> {
		if !self.column_exists("directories", "embedding_model")? {
			self.connection.execute("ALTER TABLE directories ADD COLUMN embedding_model TEXT", [])?;
		}
		self.connection.pragma_update(None, "user_version", 2)?;
		Ok(())
	}

	fn column_exists(&self, table: &str, column: &str) -> Result<bool, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(&format!("PRAGMA table_info({})", table))?;
		let names = statement.query_map([], |row| row.get::<_, String>(1))?
			.collect::<Result<Vec<_>, _>>()?;
		Ok(names.iter().any(|name| name == column))
	}

	pub fn insert_directory(
		&self,
		key: &str,
		created: &str,
		purpose: &str,
		author: &str,
		tags: &[String],
	) -> Result<(), Box<dyn std::error::Error>> {
		let tags_json = serde_json::to_string(tags)?;
		self.connection.execute(
			"INSERT INTO directories (key, created, purpose, author, tags) VALUES (?1, ?2, ?3, ?4, ?5)",
			params![key, created, purpose, author, tags_json],
		)?;
		Ok(())
	}

	pub fn update_directory(
		&self,
		key: &str,
		purpose: &str,
		author: &str,
		tags: &[String],
	) -> Result<(), Box<dyn std::error::Error>> {
		let tags_json = serde_json::to_string(tags)?;
		self.connection.execute(
			"UPDATE directories SET purpose = ?1, author = ?2, tags = ?3 WHERE key = ?4",
			params![purpose, author, tags_json, key],
		)?;
		Ok(())
	}

	pub fn add_location(
		&self,
		key: &str,
		mount_path: &str,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"INSERT OR IGNORE INTO locations (key, mount_path) VALUES (?1, ?2)",
			params![key, mount_path],
		)?;
		Ok(())
	}

	pub fn record_visit(&self, key: &str) -> Result<(), Box<dyn std::error::Error>> {
		let visited_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
		self.connection.execute(
			"INSERT INTO visits (key, visited_at) VALUES (?1, ?2)",
			params![key, visited_at],
		)?;
		Ok(())
	}

	pub fn get_directory(&self, key: &str) -> Result<Option<DirectoryRow>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT key, created, purpose, author, tags, has_docs FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			Ok(DirectoryRow {
				key: row.get(0)?,
				created: row.get(1)?,
				purpose: row.get(2)?,
				author: row.get(3)?,
				tags_json: row.get(4)?,
				has_docs: row.get::<_, i32>(5)? != 0,
			})
		})?;
		match rows.next() {
			Some(row) => Ok(Some(row?)),
			None => Ok(None),
		}
	}

	pub fn get_locations(&self, key: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT mount_path FROM locations WHERE key = ?1"
		)?;
		let paths = statement.query_map(params![key], |row| {
			row.get::<_, String>(0)
		})?.collect::<Result<Vec<_>, _>>()?;
		Ok(paths)
	}

	pub fn resolve_key(&self, key: &str) -> Result<Option<std::path::PathBuf>, Box<dyn std::error::Error>> {
		let locations = self.get_locations(key)?;
		for mount_path in &locations {
			let dir_path = std::path::Path::new(mount_path).join(key);
			if dir_path.exists() {
				return Ok(Some(dir_path));
			}
		}
		Ok(None)
	}

	pub fn get_visits(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT key FROM visits ORDER BY id"
		)?;
		let keys = statement.query_map([], |row| {
			row.get::<_, String>(0)
		})?.collect::<Result<Vec<_>, _>>()?;
		Ok(keys)
	}

	pub fn search_directories(
		&self,
		author: Option<&str>,
		since: Option<&str>,
		tags: Option<&[String]>,
	) -> Result<Vec<DirectoryRow>, Box<dyn std::error::Error>> {
		let mut clauses = Vec::new();
		let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

		if let Some(author) = author {
			clauses.push(format!("author = ?{}", param_values.len() + 1));
			param_values.push(Box::new(author.to_string()));
		}
		if let Some(since) = since {
			clauses.push(format!("created >= ?{}", param_values.len() + 1));
			param_values.push(Box::new(since.to_string()));
		}

		let mut sql = "SELECT key, created, purpose, author, tags, has_docs FROM directories".to_string();
		if !clauses.is_empty() {
			sql.push_str(" WHERE ");
			sql.push_str(&clauses.join(" AND "));
		}
		sql.push_str(" ORDER BY created DESC");

		let mut statement = self.connection.prepare(&sql)?;
		let params: Vec<&dyn rusqlite::types::ToSql> = param_values.iter()
			.map(|p| p.as_ref())
			.collect();
		let rows = statement.query_map(params.as_slice(), |row| {
			Ok(DirectoryRow {
				key: row.get(0)?,
				created: row.get(1)?,
				purpose: row.get(2)?,
				author: row.get(3)?,
				tags_json: row.get(4)?,
				has_docs: row.get::<_, i32>(5)? != 0,
			})
		})?;

		let mut results = Vec::new();
		for row in rows {
			let row = row?;
			if let Some(filter_tags) = tags {
				let row_tags = row.tags();
				let has_match = filter_tags.iter().any(|t| row_tags.contains(t));
				if !has_match {
					continue;
				}
			}
			results.push(row);
		}
		Ok(results)
	}

	pub fn get_all_directories(&self) -> Result<Vec<DirectoryRow>, Box<dyn std::error::Error>> {
		self.search_directories(None, None, None)
	}

	pub fn get_embedding(&self, key: &str) -> Result<Option<StoredEmbedding>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT embedding, embedding_model FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			Ok((
				row.get::<_, Option<Vec<u8>>>(0)?,
				row.get::<_, Option<String>>(1)?,
			))
		})?;
		match rows.next() {
			Some(row) => {
				let (bytes, model) = row?;
				Ok(bytes.map(|bytes| StoredEmbedding { bytes, model }))
			}
			None => Ok(None),
		}
	}

	pub fn set_embedding(
		&self,
		key: &str,
		embedding: &[u8],
		model: &str,
		content_hash: &str,
		has_docs: bool,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"UPDATE directories SET embedding = ?1, embedding_model = ?2, content_hash = ?3, has_docs = ?4 WHERE key = ?5",
			params![embedding, model, content_hash, has_docs as i32, key],
		)?;
		Ok(())
	}

	pub fn set_has_docs(&self, key: &str, has_docs: bool) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"UPDATE directories SET has_docs = ?1 WHERE key = ?2",
			params![has_docs as i32, key],
		)?;
		Ok(())
	}

	pub fn get_embedding_state(&self, key: &str) -> Result<EmbeddingState, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT content_hash, embedding_model FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			Ok(EmbeddingState {
				content_hash: row.get(0)?,
				model: row.get(1)?,
			})
		})?;
		match rows.next() {
			Some(row) => Ok(row?),
			None => Ok(EmbeddingState { content_hash: None, model: None }),
		}
	}

	pub fn get_all_keys(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT key FROM directories"
		)?;
		let keys = statement.query_map([], |row| {
			row.get::<_, String>(0)
		})?.collect::<Result<Vec<_>, _>>()?;
		Ok(keys)
	}

	pub fn get_all_locations(&self) -> Result<Vec<(String, String)>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT key, mount_path FROM locations"
		)?;
		let pairs = statement.query_map([], |row| {
			Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
		})?.collect::<Result<Vec<_>, _>>()?;
		Ok(pairs)
	}

	pub fn remove_location(
		&self,
		key: &str,
		mount_path: &str,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"DELETE FROM locations WHERE key = ?1 AND mount_path = ?2",
			params![key, mount_path],
		)?;
		Ok(())
	}

	pub fn key_exists(&self, key: &str) -> Result<bool, Box<dyn std::error::Error>> {
		let count: i64 = self.connection.query_row(
			"SELECT COUNT(*) FROM directories WHERE key = ?1",
			params![key],
			|row| row.get(0),
		)?;
		Ok(count > 0)
	}

	pub fn get_metadata(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT value FROM metadata WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			row.get::<_, String>(0)
		})?;
		match rows.next() {
			Some(row) => Ok(Some(row?)),
			None => Ok(None),
		}
	}

	pub fn set_metadata(&self, key: &str, value: &str) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"INSERT OR REPLACE INTO metadata (key, value) VALUES (?1, ?2)",
			params![key, value],
		)?;
		Ok(())
	}

	pub fn clear_embeddings(&self) -> Result<usize, Box<dyn std::error::Error>> {
		let count = self.connection.execute(
			"UPDATE directories SET embedding = NULL, content_hash = NULL WHERE embedding IS NOT NULL",
			[],
		)?;
		Ok(count)
	}
}

pub struct StoredEmbedding {
	pub bytes: Vec<u8>,
	pub model: Option<String>,
}

pub struct EmbeddingState {
	pub content_hash: Option<String>,
	pub model: Option<String>,
}

impl EmbeddingState {
	pub fn is_current(&self, content_hash: &str, configured_model: &str) -> bool {
		let hash_matches = self.content_hash.as_deref() == Some(content_hash);
		let model_matches = self.model.as_deref().map_or(true, |model| model == configured_model);
		hash_matches && model_matches
	}
}

pub struct DirectoryRow {
	pub key: String,
	pub created: String,
	pub purpose: String,
	pub author: String,
	pub tags_json: String,
	pub has_docs: bool,
}

impl DirectoryRow {
	pub fn tags(&self) -> Vec<String> {
		serde_json::from_str(&self.tags_json).unwrap_or_default()
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rusqlite::Connection;

	fn test_db() -> Database {
		let connection = Connection::open_in_memory().unwrap();
		connection.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
		let db = Database { connection };
		db.initialize().unwrap();
		db
	}

	#[test]
	fn initialize_versions_and_is_idempotent() {
		let db = test_db();
		let version: i32 = db.connection
			.query_row("PRAGMA user_version", [], |row| row.get(0))
			.unwrap();
		assert_eq!(version, current_schema_version);
		db.initialize().unwrap();
	}

	#[test]
	fn newer_schema_versions_are_rejected() {
		let db = test_db();
		db.connection.pragma_update(None, "user_version", 99).unwrap();
		assert!(db.initialize().is_err());
	}

	#[test]
	fn directory_roundtrip_with_tags() {
		let db = test_db();
		db.insert_directory("abc123", "2026-01-01", "purpose", "author", &["x".to_string(), "y".to_string()]).unwrap();
		let row = db.get_directory("abc123").unwrap().unwrap();
		assert_eq!(row.purpose, "purpose");
		assert_eq!(row.tags(), vec!["x".to_string(), "y".to_string()]);
		assert!(db.get_directory("zzz999").unwrap().is_none());
	}

	#[test]
	fn search_filters_by_author_since_and_tags() {
		let db = test_db();
		db.insert_directory("aaa", "2025-01-01", "p", "alice", &["julia".to_string()]).unwrap();
		db.insert_directory("bbb", "2026-01-01", "p", "bob", &["rust".to_string()]).unwrap();

		let by_author = db.search_directories(Some("alice"), None, None).unwrap();
		assert_eq!(by_author.len(), 1);
		assert_eq!(by_author[0].key, "aaa");

		let recent = db.search_directories(None, Some("2025-06-01"), None).unwrap();
		assert_eq!(recent.len(), 1);
		assert_eq!(recent[0].key, "bbb");

		let tagged = db.search_directories(None, None, Some(&["rust".to_string()])).unwrap();
		assert_eq!(tagged.len(), 1);
		assert_eq!(tagged[0].key, "bbb");
	}

	#[test]
	fn foreign_keys_are_enforced() {
		let db = test_db();
		assert!(db.record_visit("nonexistent").is_err());
		db.insert_directory("abc123", "2026-01-01", "p", "a", &[]).unwrap();
		assert!(db.record_visit("abc123").is_ok());
	}

	#[test]
	fn visits_carry_utc_timestamps() {
		let db = test_db();
		db.insert_directory("abc123", "2026-01-01", "p", "a", &[]).unwrap();
		db.record_visit("abc123").unwrap();
		let visited_at: String = db.connection
			.query_row("SELECT visited_at FROM visits", [], |row| row.get(0))
			.unwrap();
		assert!(visited_at.contains('T'));
		assert!(visited_at.ends_with('Z'));
	}

	#[test]
	fn embedding_state_currency_rules() {
		let db = test_db();
		db.insert_directory("abc123", "2026-01-01", "p", "a", &[]).unwrap();
		db.set_embedding("abc123", &[0u8; 8], "model-one", "hash-one", true).unwrap();

		let state = db.get_embedding_state("abc123").unwrap();
		assert!(state.is_current("hash-one", "model-one"));
		assert!(!state.is_current("hash-two", "model-one"));
		assert!(!state.is_current("hash-one", "model-two"));

		db.connection.execute("UPDATE directories SET embedding_model = NULL", []).unwrap();
		let legacy = db.get_embedding_state("abc123").unwrap();
		assert!(legacy.is_current("hash-one", "any-model"));
		assert!(!legacy.is_current("hash-two", "any-model"));
	}

	#[test]
	fn stored_embedding_carries_model() {
		let db = test_db();
		db.insert_directory("abc123", "2026-01-01", "p", "a", &[]).unwrap();
		assert!(db.get_embedding("abc123").unwrap().is_none());
		db.set_embedding("abc123", &[1, 2, 3, 4], "m", "h", true).unwrap();
		let stored = db.get_embedding("abc123").unwrap().unwrap();
		assert_eq!(stored.bytes, vec![1, 2, 3, 4]);
		assert_eq!(stored.model.as_deref(), Some("m"));
	}

	#[test]
	fn resolve_key_finds_a_reachable_location() {
		let db = test_db();
		db.insert_directory("abc123", "2026-01-01", "p", "a", &[]).unwrap();
		let base = std::env::temp_dir().join(format!("sf_resolve_test_{}", std::process::id()));
		let mounted = base.join("mounted");
		std::fs::create_dir_all(mounted.join("abc123")).unwrap();
		db.add_location("abc123", base.join("absent").to_str().unwrap()).unwrap();
		db.add_location("abc123", mounted.to_str().unwrap()).unwrap();
		let resolved = db.resolve_key("abc123").unwrap().unwrap();
		assert_eq!(resolved, mounted.join("abc123"));
		std::fs::remove_dir_all(&base).unwrap();
	}
}
