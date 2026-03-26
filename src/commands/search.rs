use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, EmbeddingClient};
use crate::picker::{self, PickerItem};

struct ScoredResult {
	key: String,
	purpose: String,
	score: f32,
	has_docs: bool,
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	client: Option<&dyn EmbeddingClient>,
	query: Option<String>,
	tags: Option<Vec<String>>,
	author: Option<String>,
	since: Option<String>,
) -> Result<(), Box<dyn std::error::Error>> {
	let candidates = db.search_directories(
		author.as_deref(),
		since.as_deref(),
		tags.as_deref(),
	)?;

	if candidates.is_empty() {
		eprintln!("no matching directories");
		return Ok(());
	}

	let results: Vec<ScoredResult> = match query {
		Some(ref query_text) => {
			let client = client.expect("embedding client required for semantic search");
			let query_embedding = client.embed(query_text)?;

			let mut scored: Vec<ScoredResult> = Vec::new();
			let mut skipped = 0;
			for row in &candidates {
				let embedding_bytes = match db.get_embedding(&row.key)? {
					Some(bytes) => bytes,
					None => {
						skipped += 1;
						continue;
					}
				};
				let stored_embedding = embed::bytes_to_embedding(&embedding_bytes);
				let score = embed::cosine_similarity(&query_embedding, &stored_embedding);
				scored.push(ScoredResult {
					key: row.key.clone(),
					purpose: row.purpose.clone(),
					score,
					has_docs: row.has_docs,
				});
			}

			if skipped > 0 {
				eprintln!(
					"  ({} director{} skipped: no embedding; run `sf sync`)",
					skipped,
					if skipped == 1 { "y" } else { "ies" },
				);
			}

			scored.sort_by(|a, b| {
				b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
			});

			scored.retain(|r| r.score >= config.min_similarity);
			scored.truncate(config.max_search_results);
			scored
		}
		None => {
			candidates.iter()
				.map(|row| ScoredResult {
					key: row.key.clone(),
					purpose: row.purpose.clone(),
					score: 0.0,
					has_docs: row.has_docs,
				})
				.collect()
		}
	};

	if results.is_empty() {
		eprintln!("no results (do you need to run `sf sync`?)");
		return Ok(());
	}

	let has_scores = query.is_some();
	let show_doc_warnings = has_scores && config.warn_no_docs;
	let items: Vec<PickerItem> = results.iter()
		.map(|r| {
			let display = if !r.has_docs && show_doc_warnings {
				format!("{} (!)", r.purpose)
			} else {
				r.purpose.clone()
			};
			PickerItem {
				key: r.key.clone(),
				display,
				score: if has_scores { Some(r.score) } else { None },
			}
		})
		.collect();

	let any_no_docs = show_doc_warnings && results.iter().any(|r| !r.has_docs);
	if any_no_docs {
		eprintln!("  (!) = no docs; score may be unreliable");
	}

	if let Some(selected_key) = picker::run_picker(&items, "search") {
		let dir_path = db.resolve_key(&selected_key)?
			.ok_or_else(|| format!("no accessible location for {}", selected_key))?;
		println!("{}", dir_path.display());
		db.record_visit(&selected_key)?;
	}

	Ok(())
}
