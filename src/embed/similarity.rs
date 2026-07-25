pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
	if a.len() != b.len() {
		return 0.0;
	}
	let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
	let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
	let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
	if norm_a == 0.0 || norm_b == 0.0 {
		return 0.0;
	}
	dot / (norm_a * norm_b)
}

pub fn elbow_cutoff(scores: &[f32], min_results: usize, max_results: usize) -> usize {
	if scores.len() <= min_results {
		return scores.len();
	}
	let n = scores.len().min(max_results);
	if n <= 2 {
		return n;
	}

	let first = scores[0];
	let last = scores[n - 1];
	let range = first - last;
	if range.abs() < f32::EPSILON {
		return n;
	}

	let mut max_distance = 0.0f32;
	let mut elbow_index = min_results;

	for i in 1..n - 1 {
		let expected = first - range * (i as f32) / ((n - 1) as f32);
		let distance = (scores[i] - expected).abs();
		if distance > max_distance {
			max_distance = distance;
			elbow_index = i + 1;
		}
	}

	elbow_index.max(min_results)
}
