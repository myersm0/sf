use std::path::Path;

use chrono::{SecondsFormat, Utc};
use rusqlite::{Connection, params};
use serde_json;

#[allow(non_upper_case_globals)]
const current_schema_version: i32 = 1;

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

	pub fn get_embedding(&self, key: &str) -> Result<Option<Vec<u8>>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT embedding FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			row.get::<_, Option<Vec<u8>>>(0)
		})?;
		match rows.next() {
			Some(row) => Ok(row?),
			None => Ok(None),
		}
	}

	pub fn set_embedding(
		&self,
		key: &str,
		embedding: &[u8],
		content_hash: &str,
		has_docs: bool,
	) -> Result<(), Box<dyn std::error::Error>> {
		self.connection.execute(
			"UPDATE directories SET embedding = ?1, content_hash = ?2, has_docs = ?3 WHERE key = ?4",
			params![embedding, content_hash, has_docs as i32, key],
		)?;
		Ok(())
	}

	pub fn get_content_hash(&self, key: &str) -> Result<Option<String>, Box<dyn std::error::Error>> {
		let mut statement = self.connection.prepare(
			"SELECT content_hash FROM directories WHERE key = ?1"
		)?;
		let mut rows = statement.query_map(params![key], |row| {
			row.get::<_, Option<String>>(0)
		})?;
		match rows.next() {
			Some(row) => Ok(row?),
			None => Ok(None),
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
