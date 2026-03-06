use crate::config::AppConfig;
use crate::db::Database;
use crate::picker::{self, PickerItem};

pub fn run(
	db: &Database,
	config: &AppConfig,
	query: Option<String>,
	tags: Option<Vec<String>>,
	author: Option<String>,
	since: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
	let results = db.search_directories(
		author.as_deref(),
		since.as_deref(),
		tags.as_deref(),
	)?;

	if results.is_empty() {
		eprintln!("no matching directories");
		return Ok(());
	}

	let items: Vec<PickerItem> = results.iter()
		.map(|row| PickerItem {
			key: row.key.clone(),
			display: row.purpose.clone(),
		})
		.collect();

	if let Some(selected_key) = picker::run_picker(&items, "search") {
		let dir_path = config.contents_path.join(&selected_key);
		println!("{}", dir_path.display());
		db.record_visit(&selected_key)?;
	}

	Ok(())
}
