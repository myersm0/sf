use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json;

use crate::embed::EmbeddingIdentity;
use crate::meta::DirMeta;

#[allow(non_upper_case_globals)]
const current_schema_version: i32 = 4;

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
		if version < 3 {
			self.migrate_to_version_3()?;
		}
		if version < 4 {
			self.migrate_to_version_4()?;
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

	fn migrate_to_version_3(&self) -> Result<(), Box<dyn std::error::Error>> {
		if !self.column_exists("directories", "embedding_backend")? {
			self.connection.execute("ALTER TABLE directories ADD COLUMN embedding_backend TEXT", [])?;
		}
		self.connection.pragma_update(None, "user_version", 3)?;
		Ok(())
	}

	fn migrate_to_version_4(&self) -> Result<(), Box<dyn std::error::Error>> {
		if !self.column_exists("directories", "source_name")? {
			self.connection.execute("ALTER TABLE directories ADD COLUMN source_name TEXT", [])?;
		}
		self.connection.pragma_update(None, "user_version", 4)?;
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
		meta: &DirMeta,
	) -> Result<(), Box<dyn std::error::Error>> {
		let tags_json = serde_json::to_string(&meta.tags)?;
		self.connection.execute(
			"INSERT INTO directories (key, created, purpose, author, tags, source_name)
				VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
			params![key, meta.created, meta.purpose, meta.author, tags_json, meta.source_name],
		)?;
		Ok(())
	}

	pub fn update_directory(
		&self,
		key: &str,
		meta: &DirMeta,
	) -> Result<(), Box<dyn std::error::Error>> {
		let tags_json = serde_json::to_string(&meta.tags)?;
		self.connection.execute(
			"UPDATE directories SET created = ?1, purpose = ?2, author = ?3, tags = ?4, source_name = ?5
				WHERE key = ?6",
			params![meta.created, meta.purpose, meta.author, tags_json, meta.source_name, key],
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
			"SELECT key, created, purpose, author, tags, source_name, has_docs
				FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], directory_row)?;
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
		source: Option<&str>,
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
		if let Some(source) = source {
			clauses.push(format!("source_name = ?{} COLLATE NOCASE", param_values.len() + 1));
			param_values.push(Box::new(source.to_string()));
		}

		let mut sql = "SELECT key, created, purpose, author, tags, source_name, has_docs FROM directories".to_string();
		if !clauses.is_empty() {
			sql.push_str(" WHERE ");
			sql.push_str(&clauses.join(" AND "));
		}
		sql.push_str(" ORDER BY created DESC");

		let mut statement = self.connection.prepare(&sql)?;
		let params: Vec<&dyn rusqlite::types::ToSql> = param_values.iter()
			.map(|p| p.as_ref())
			.collect();
		let rows = statement.query_map(params.as_slice(), directory_row)?;

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
		self.search_directories(None, None, None, None)
	}

	pub fn get_embedding(&self, key: &str) -> Result<Option<StoredEmbedding>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT embedding, embedding_backend, embedding_model FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			Ok((
				row.get::<_, Option<Vec<u8>>>(0)?,
				row.get::<_, Option<String>>(1)?,
				row.get::<_, Option<String>>(2)?,
			))
		})?;
		match rows.next() {
			Some(row) => {
				let (bytes, backend, model) = row?;
				Ok(bytes.map(|bytes| StoredEmbedding { bytes, backend, model }))
			}
			None => Ok(None),
		}
	}

	pub fn set_embedding(
		&self,
		key: &str,
		embedding: &[u8],
		identity: &EmbeddingIdentity,
		content_hash: &str,
		has_docs: bool,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"UPDATE directories SET embedding = ?1, embedding_backend = ?2, embedding_model = ?3, content_hash = ?4, has_docs = ?5 WHERE key = ?6",
			params![embedding, identity.backend, identity.model, content_hash, has_docs as i32, key],
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
			"SELECT content_hash, embedding_backend, embedding_model FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			Ok(EmbeddingState {
				content_hash: row.get(0)?,
				backend: row.get(1)?,
				model: row.get(2)?,
			})
		})?;
		match rows.next() {
			Some(row) => Ok(row?),
			None => Ok(EmbeddingState { content_hash: None, backend: None, model: None }),
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

}

pub struct StoredEmbedding {
	pub bytes: Vec<u8>,
	pub backend: Option<String>,
	pub model: Option<String>,
}

pub struct EmbeddingState {
	pub content_hash: Option<String>,
	pub backend: Option<String>,
	pub model: Option<String>,
}

impl EmbeddingState {
	pub fn is_current(&self, content_hash: &str, identity: &EmbeddingIdentity) -> bool {
		self.content_hash.as_deref() == Some(content_hash)
			&& identity.matches_stored(self.backend.as_deref(), self.model.as_deref())
	}
}

pub struct DirectoryRow {
	pub key: String,
	pub created: String,
	pub purpose: String,
	pub author: String,
	pub tags_json: String,
	pub source_name: Option<String>,
	pub has_docs: bool,
}

fn directory_row(row: &rusqlite::Row) -> rusqlite::Result<DirectoryRow> {
	Ok(DirectoryRow {
		key: row.get(0)?,
		created: row.get(1)?,
		purpose: row.get(2)?,
		author: row.get(3)?,
		tags_json: row.get(4)?,
		source_name: row.get(5)?,
		has_docs: row.get::<_, i32>(6)? != 0,
	})
}

impl DirectoryRow {
	pub fn tags(&self) -> Vec<String> {
		serde_json::from_str(&self.tags_json).unwrap_or_default()
	}

	pub fn mirrors(&self, meta: &DirMeta) -> bool {
		self.created == meta.created
			&& self.purpose == meta.purpose
			&& self.author == meta.author
			&& self.source_name == meta.source_name
			&& self.tags() == meta.tags
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use rusqlite::Connection;

	fn meta(created: &str, purpose: &str, author: &str, tags: &[&str]) -> DirMeta {
		DirMeta {
			created: created.to_string(),
			purpose: purpose.to_string(),
			author: author.to_string(),
			tags: tags.iter().map(|tag| tag.to_string()).collect(),
			index: Vec::new(),
			source_name: None,
			extra: serde_json::Map::new(),
		}
	}

	fn identity(backend: &str, model: &str) -> EmbeddingIdentity {
		EmbeddingIdentity {
			backend: backend.to_string(),
			model: model.to_string(),
		}
	}

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
		db.insert_directory("abc123", &meta("2026-01-01", "purpose", "author", &["x", "y"])).unwrap();
		let row = db.get_directory("abc123").unwrap().unwrap();
		assert_eq!(row.purpose, "purpose");
		assert_eq!(row.tags(), vec!["x".to_string(), "y".to_string()]);
		assert!(db.get_directory("zzz999").unwrap().is_none());
	}

	#[test]
	fn update_directory_rewrites_every_mirrored_field() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "old", "alice", &["x"])).unwrap();
		db.update_directory("abc123", &meta("2026-06-01", "new", "bob", &["y"])).unwrap();
		let row = db.get_directory("abc123").unwrap().unwrap();
		assert_eq!(row.created, "2026-06-01");
		assert_eq!(row.purpose, "new");
		assert_eq!(row.author, "bob");
		assert_eq!(row.tags(), vec!["y".to_string()]);
	}

	#[test]
	fn source_name_round_trips_and_filters_case_insensitively() {
		let db = test_db();
		let mut cloned = meta("2026-01-01", "a checkout", "m", &[]);
		cloned.source_name = Some("sf".to_string());
		db.insert_directory("aaa", &cloned).unwrap();
		db.insert_directory("bbb", &meta("2026-01-01", "unrelated", "m", &[])).unwrap();

		let row = db.get_directory("aaa").unwrap().unwrap();
		assert_eq!(row.source_name.as_deref(), Some("sf"));
		assert!(db.get_directory("bbb").unwrap().unwrap().source_name.is_none());

		let matched = db.search_directories(None, None, Some("SF"), None).unwrap();
		assert_eq!(matched.len(), 1);
		assert_eq!(matched[0].key, "aaa");
		assert!(db.search_directories(None, None, Some("other"), None).unwrap().is_empty());
	}

	#[test]
	fn mirrors_tracks_every_field_the_row_carries() {
		let db = test_db();
		let original = meta("2026-01-01", "p", "m", &["x"]);
		db.insert_directory("abc123", &original).unwrap();
		let row = db.get_directory("abc123").unwrap().unwrap();
		assert!(row.mirrors(&original));

		for changed in [
			meta("2026-02-01", "p", "m", &["x"]),
			meta("2026-01-01", "q", "m", &["x"]),
			meta("2026-01-01", "p", "n", &["x"]),
			meta("2026-01-01", "p", "m", &["y"]),
		] {
			assert!(!row.mirrors(&changed));
		}

		let mut with_source = original.clone();
		with_source.source_name = Some("myproject".to_string());
		assert!(!row.mirrors(&with_source));
	}

	#[test]
	fn search_filters_by_author_since_and_tags() {
		let db = test_db();
		db.insert_directory("aaa", &meta("2025-01-01", "p", "alice", &["julia"])).unwrap();
		db.insert_directory("bbb", &meta("2026-01-01", "p", "bob", &["rust"])).unwrap();

		let by_author = db.search_directories(Some("alice"), None, None, None).unwrap();
		assert_eq!(by_author.len(), 1);
		assert_eq!(by_author[0].key, "aaa");

		let recent = db.search_directories(None, Some("2025-06-01"), None, None).unwrap();
		assert_eq!(recent.len(), 1);
		assert_eq!(recent[0].key, "bbb");

		let tagged = db.search_directories(None, None, None, Some(&["rust".to_string()])).unwrap();
		assert_eq!(tagged.len(), 1);
		assert_eq!(tagged[0].key, "bbb");
	}

	#[test]
	fn foreign_keys_are_enforced() {
		let db = test_db();
		assert!(db.record_visit("nonexistent").is_err());
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		assert!(db.record_visit("abc123").is_ok());
	}

	#[test]
	fn visits_carry_utc_timestamps() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
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
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		db.set_embedding("abc123", &[0u8; 8], &identity("ollama", "model-one"), "hash-one", true).unwrap();

		let state = db.get_embedding_state("abc123").unwrap();
		assert!(state.is_current("hash-one", &identity("ollama", "model-one")));
		assert!(!state.is_current("hash-two", &identity("ollama", "model-one")));
		assert!(!state.is_current("hash-one", &identity("ollama", "model-two")));
	}

	#[test]
	fn same_model_name_on_another_backend_is_stale() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		db.set_embedding("abc123", &[0u8; 8], &identity("ollama", "shared-name"), "hash-one", true).unwrap();
		let state = db.get_embedding_state("abc123").unwrap();
		assert!(state.is_current("hash-one", &identity("ollama", "shared-name")));
		assert!(!state.is_current("hash-one", &identity("openai", "shared-name")));
	}

	#[test]
	fn colons_in_model_names_survive_storage() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		db.set_embedding("abc123", &[0u8; 8], &identity("ollama", "qwen3-embedding:8b"), "h", true).unwrap();
		let stored = db.get_embedding("abc123").unwrap().unwrap();
		assert_eq!(stored.model.as_deref(), Some("qwen3-embedding:8b"));
		assert_eq!(stored.backend.as_deref(), Some("ollama"));
	}

	#[test]
	fn rows_predating_a_column_are_grandfathered() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		db.set_embedding("abc123", &[0u8; 8], &identity("ollama", "model-one"), "hash-one", true).unwrap();

		db.connection.execute("UPDATE directories SET embedding_backend = NULL", []).unwrap();
		let without_backend = db.get_embedding_state("abc123").unwrap();
		assert!(without_backend.is_current("hash-one", &identity("openai", "model-one")));
		assert!(!without_backend.is_current("hash-one", &identity("openai", "model-two")));

		db.connection.execute("UPDATE directories SET embedding_model = NULL", []).unwrap();
		let legacy = db.get_embedding_state("abc123").unwrap();
		assert!(legacy.is_current("hash-one", &identity("anything", "any-model")));
		assert!(!legacy.is_current("hash-two", &identity("anything", "any-model")));
	}

	#[test]
	fn stored_embedding_carries_backend_and_model() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
		assert!(db.get_embedding("abc123").unwrap().is_none());
		db.set_embedding("abc123", &[1, 2, 3, 4], &identity("openai", "m"), "h", true).unwrap();
		let stored = db.get_embedding("abc123").unwrap().unwrap();
		assert_eq!(stored.bytes, vec![1, 2, 3, 4]);
		assert_eq!(stored.backend.as_deref(), Some("openai"));
		assert_eq!(stored.model.as_deref(), Some("m"));
	}

	#[test]
	fn resolve_key_finds_a_reachable_location() {
		let db = test_db();
		db.insert_directory("abc123", &meta("2026-01-01", "p", "a", &[])).unwrap();
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
