use crate::config::AppConfig;
use crate::db::Database;
use crate::embed::{self, OllamaClient};
use crate::picker::{self, PickerItem};

struct ScoredResult {
	key: String,
	purpose: String,
	score: f32,
}

pub fn run(
	db: &Database,
	config: &AppConfig,
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
			let client = OllamaClient::new(&config.ollama_url, &config.embedding_model);
			let query_embedding = client.embed(query_text)?;

			let mut scored: Vec<ScoredResult> = Vec::new();
			for row in &candidates {
				let embedding_bytes = match db.get_embedding(&row.key)? {
					Some(bytes) => bytes,
					None => continue,
				};
				let stored_embedding = embed::bytes_to_embedding(&embedding_bytes);
				let score = embed::cosine_similarity(&query_embedding, &stored_embedding);
				scored.push(ScoredResult {
					key: row.key.clone(),
					purpose: row.purpose.clone(),
					score,
				});
			}

			scored.sort_by(|a, b| {
				b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
			});

			let scores: Vec<f32> = scored.iter().map(|s| s.score).collect();
			let cutoff = embed::elbow_cutoff(&scores, 1, 15);
			scored.truncate(cutoff);
			scored
		}
		None => {
			candidates.iter()
				.map(|row| ScoredResult {
					key: row.key.clone(),
					purpose: row.purpose.clone(),
					score: 0.0,
				})
				.collect()
		}
	};

	if results.is_empty() {
		eprintln!("no results (do you need to run `clew sync`?)");
		return Ok(());
	}

	let has_scores = query.is_some();
	let items: Vec<PickerItem> = results.iter()
		.map(|r| PickerItem {
			key: r.key.clone(),
			display: r.purpose.clone(),
			score: if has_scores { Some(r.score) } else { None },
		})
		.collect();

	if let Some(selected_key) = picker::run_picker(&items, "search") {
		let dir_path = config.contents_path.join(&selected_key);
		println!("{}", dir_path.display());
		db.record_visit(&selected_key)?;
	}

	Ok(())
}
