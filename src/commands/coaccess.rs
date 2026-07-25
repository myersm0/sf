use std::collections::{HashMap, HashSet};

use crate::config::AppConfig;
use crate::db::Database;
use crate::picker::{self, PickerItem};

struct CoAccessEdge {
	neighbor: String,
	score: f64,
}

fn compute_npmi(visits: &[String], target: &str, window_size: usize) -> Vec<CoAccessEdge> {
	if visits.len() < window_size || window_size < 2 {
		return Vec::new();
	}

	let total_windows = visits.len() - window_size + 1;
	let total = total_windows as f64;

	let mut marginal_counts: HashMap<&str, u32> = HashMap::new();
	let mut joint_counts: HashMap<(&str, &str), u32> = HashMap::new();

	for start in 0..total_windows {
		let window: HashSet<&str> = visits[start..start + window_size]
			.iter()
			.map(|s| s.as_str())
			.collect();
		let unique: Vec<&str> = window.into_iter().collect();

		for key in &unique {
			*marginal_counts.entry(key).or_insert(0) += 1;
		}

		for i in 0..unique.len() {
			for j in (i + 1)..unique.len() {
				let pair = if unique[i] < unique[j] {
					(unique[i], unique[j])
				} else {
					(unique[j], unique[i])
				};
				*joint_counts.entry(pair).or_insert(0) += 1;
			}
		}
	}

	let mut edges: Vec<CoAccessEdge> = Vec::new();

	for ((key_a, key_b), joint) in &joint_counts {
		let is_relevant = *key_a == target || *key_b == target;
		if !is_relevant {
			continue;
		}

		let p_joint = *joint as f64 / total;
		let p_a = marginal_counts[key_a] as f64 / total;
		let p_b = marginal_counts[key_b] as f64 / total;

		let npmi = if (p_joint - 1.0).abs() < f64::EPSILON {
			1.0
		} else {
			let pmi = (p_joint / (p_a * p_b)).ln();
			pmi / -p_joint.ln()
		};

		if npmi <= 0.0 {
			continue;
		}

		let neighbor = if *key_a == target {
			key_b.to_string()
		} else {
			key_a.to_string()
		};

		edges.push(CoAccessEdge { neighbor, score: npmi });
	}

	edges.sort_by(|a, b| {
		b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal)
	});

	edges
}

pub fn run(
	db: &Database,
	config: &AppConfig,
	key: &str,
	number: usize,
) -> Result<(), Box<dyn std::error::Error>> {
	let visits = db.get_visits()?;
	let edges = compute_npmi(&visits, key, config.coaccess_window);

	if edges.is_empty() {
		eprintln!("no co-access neighbors for {}", key);
		return Ok(());
	}

	let limited: Vec<&CoAccessEdge> = edges.iter().take(number).collect();
	let items: Vec<PickerItem> = limited.iter()
		.filter_map(|edge| {
			let purpose = db.get_directory(&edge.neighbor).ok()??.purpose;
			Some(PickerItem {
				key: edge.neighbor.clone(),
				display: purpose,
				score: Some(edge.score as f32),
			})
		})
		.collect();

	if let Some(selected_key) = picker::run_picker(&items, "coaccess (npmi)") {
		let dir_path = db.resolve_key(&selected_key)?
			.ok_or_else(|| format!("no accessible location for {}", selected_key))?;
		println!("{}", dir_path.display());
		db.record_visit(&selected_key)?;
	}

	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn visits(keys: &[&str]) -> Vec<String> {
		keys.iter().map(|key| key.to_string()).collect()
	}

	#[test]
	fn short_history_yields_nothing() {
		assert!(compute_npmi(&visits(&["a"]), "a", 3).is_empty());
		assert!(compute_npmi(&visits(&["a", "b"]), "a", 1).is_empty());
	}

	#[test]
	fn constant_companions_score_one() {
		let edges = compute_npmi(&visits(&["a", "b", "a", "b"]), "a", 2);
		assert_eq!(edges.len(), 1);
		assert_eq!(edges[0].neighbor, "b");
		assert!((edges[0].score - 1.0).abs() < 1e-9);
	}

	#[test]
	fn independent_keys_are_excluded() {
		let edges = compute_npmi(&visits(&["a", "b", "c", "d"]), "a", 2);
		assert_eq!(edges.len(), 1);
		assert_eq!(edges[0].neighbor, "b");
		let expected = (1.5f64).ln() / (3.0f64).ln();
		assert!((edges[0].score - expected).abs() < 1e-9);
	}

	#[test]
	fn ubiquitous_target_has_no_informative_neighbors() {
		let edges = compute_npmi(&visits(&["a", "b", "a", "b", "a", "c"]), "a", 2);
		assert!(edges.is_empty());
	}

	#[test]
	fn duplicates_within_a_window_count_once() {
		let edges = compute_npmi(&visits(&["a", "a", "b"]), "a", 3);
		assert_eq!(edges.len(), 1);
		assert_eq!(edges[0].neighbor, "b");
		assert!((edges[0].score - 1.0).abs() < 1e-9);
	}

	#[test]
	fn absent_target_yields_nothing() {
		assert!(compute_npmi(&visits(&["a", "b", "a", "b"]), "z", 2).is_empty());
	}
}
