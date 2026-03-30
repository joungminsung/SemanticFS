/// Reciprocal Rank Fusion: merge multiple ranked lists
/// RRF_score(d) = Sigma 1 / (k + rank_i(d))
pub fn reciprocal_rank_fusion(
    ranked_lists: &[Vec<(i64, f32)>],  // Each list: (file_id, score)
    k: f32,
) -> Vec<(i64, f32)> {
    use std::collections::HashMap;

    let mut scores: HashMap<i64, f32> = HashMap::new();

    for list in ranked_lists {
        for (rank, (file_id, _original_score)) in list.iter().enumerate() {
            let rrf_score = 1.0 / (k + rank as f32 + 1.0);
            *scores.entry(*file_id).or_insert(0.0) += rrf_score;
        }
    }

    let mut results: Vec<(i64, f32)> = scores.into_iter().collect();
    results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_basic() {
        let list1 = vec![(1, 0.9), (2, 0.7), (3, 0.5)];
        let list2 = vec![(2, 0.95), (1, 0.6), (4, 0.3)];
        let results = reciprocal_rank_fusion(&[list1, list2], 60.0);

        // File 1 and 2 should be top results (appear in both lists)
        assert!(results.len() >= 2);
        let top_ids: Vec<i64> = results.iter().take(2).map(|(id, _)| *id).collect();
        assert!(top_ids.contains(&1));
        assert!(top_ids.contains(&2));
    }

    #[test]
    fn test_rrf_empty() {
        let results = reciprocal_rank_fusion(&[], 60.0);
        assert!(results.is_empty());
    }

    #[test]
    fn test_rrf_single_list() {
        let list = vec![(1, 0.9), (2, 0.5)];
        let results = reciprocal_rank_fusion(&[list], 60.0);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].0, 1); // Higher rank should have higher RRF score
    }
}
