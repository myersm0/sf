use crate::db::Database;

pub fn run(db: &Database, key: &str) -> Result<(), Box<dyn std::error::Error>> {
	let row = db.get_directory(key)?
		.ok_or_else(|| format!("key not found: {}", key))?;
	let locations = db.get_locations(key)?;
	let tags = row.tags();

	eprintln!("  key:      {}", row.key);
	eprintln!("  created:  {}", row.created);
	eprintln!("  purpose:  {}", row.purpose);
	eprintln!("  author:   {}", row.author);
	if !tags.is_empty() {
		eprintln!("  tags:     {}", tags.join(", "));
	}
	if !locations.is_empty() {
		eprintln!("  locations:");
		for location in &locations {
			eprintln!("    - {}", location);
		}
	}
	let count = locations.len();
	if count < 2 {
		eprintln!("  warning: only {} backup location(s)", count);
	}

	Ok(())
}
