use crate::spatial::{query_radius, DistanceTable};
use crate::store::CellStore;
use crate::types::*;

/// Evaluate how likely `candidate_id` is the password value for `header_id`.
///
/// Returns `(score, reason)` where reason is a semicolon-separated
/// breakdown of contributing factors. All bonuses use precomputed
/// feature_flags — no regex runs during scoring.
pub fn score_candidate(
    store: &CellStore,
    header_id: u32,
    candidate_id: u32,
    dist_table: &DistanceTable,
) -> (f32, String) {
    let h = header_id as usize;
    let c = candidate_id as usize;
    let flags = store.feature_flags[c];
    let value = store.get_value(c);

    let mut score = 0.0_f32;
    let mut parts: Vec<String> = Vec::new();

    // DISTANCE: from precomputed table
    let dr = store.get_row(c) as i32 - store.get_row(h) as i32;
    let dc = store.get_col(c) as i32 - store.get_col(h) as i32;
    let dist_score = dist_table.score(dr, dc);
    score += dist_score;
    parts.push(format!("distance={dist_score}"));

    // LENGTH: +20 if len >= 8
    if value.len() >= 8 {
        score += 20.0;
        parts.push("length>=8".into());
    }

    // CHAR CLASSES: +10 each
    if flags & FLAG_HAS_UPPER != 0 {
        score += 10.0;
        parts.push("upper".into());
    }
    if flags & FLAG_HAS_LOWER != 0 {
        score += 10.0;
        parts.push("lower".into());
    }
    if flags & FLAG_HAS_DIGIT != 0 {
        score += 10.0;
        parts.push("digit".into());
    }
    if flags & FLAG_HAS_SPECIAL != 0 {
        score += 10.0;
        parts.push("special".into());
    }

    // ENTROPY: +25 if above threshold
    if store.entropy[c] > ENTROPY_THRESHOLD {
        score += 25.0;
        parts.push("high_entropy".into());
    }

    // CONTEXT: +30 if same region (both assigned, matching)
    let h_region = store.region_ids[h];
    let c_region = store.region_ids[c];
    if h_region >= 0 && h_region == c_region {
        score += 30.0;
        parts.push("same_region".into());
    }

    // USERNAME NEARBY: +30 if any neighbor of candidate has username header flag
    let neighbors = query_radius(store, store.get_row(c), store.get_col(c), NEIGHBOR_RADIUS);
    let username_nearby = neighbors
        .iter()
        .any(|&nid| store.feature_flags[nid as usize] & FLAG_IS_USERNAME_HEADER != 0);
    if username_nearby {
        score += 30.0;
        parts.push("username_nearby".into());
    }

    // URL NEARBY: +20 if any neighbor of candidate has url header flag
    let url_nearby = neighbors
        .iter()
        .any(|&nid| store.feature_flags[nid as usize] & FLAG_IS_URL_HEADER != 0);
    if url_nearby {
        score += 20.0;
        parts.push("url_nearby".into());
    }

    // PENALTY: -80 if candidate looks like a header itself
    if flags & (FLAG_IS_PASSWORD_HEADER | FLAG_IS_USERNAME_HEADER) != 0 {
        score -= 80.0;
        parts.push("penalty_header=-80".into());
    }

    // PENALTY: -80 if candidate is a form label ("Database:", "Post ID:")
    if value.trim_end().ends_with(':') {
        score -= 80.0;
        parts.push("penalty_label=-80".into());
    }

    // PENALTY: -40 if candidate contains whitespace (prose, column headers)
    if value.trim().contains(char::is_whitespace) {
        score -= 40.0;
        parts.push("penalty_whitespace=-40".into());
    }

    // PENALTY: -50 if value too short
    if value.len() < MIN_CANDIDATE_LENGTH {
        score -= 50.0;
        parts.push("penalty_short=-50".into());
    }

    (score, parts.join(";"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{classify_cells, reduce_candidate_space};
    use crate::features::precompute_features;
    use crate::regions::detect_regions;
    use crate::spatial::DISTANCE_TABLE;
    use crate::store::RawCell;

    fn raw(row: u32, col: u32, value: &str, formula: &str, comment: &str) -> RawCell {
        RawCell {
            row,
            col,
            value: value.into(),
            formula: formula.into(),
            comment: comment.into(),
            sheet_name: "Sheet1".into(),
            is_merged_origin: false,
        }
    }

    fn build_store(cells: Vec<RawCell>) -> CellStore {
        let mut store = CellStore::from_raw(cells);
        precompute_features(&mut store);
        store
    }

    #[test]
    fn right_adjacent_strong_password_above_threshold() {
        // Header at (0,0), strong password at (0,1)
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "P@ssw0rd!", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        detect_regions(&mut store, &candidates);

        let (score, _reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(
            score > RELATIONSHIP_THRESHOLD,
            "score {score} should exceed threshold {RELATIONSHIP_THRESHOLD}"
        );
    }

    #[test]
    fn adjacent_value_beats_nearby_label() {
        // Real value right of the header must outscore a label two rows up
        let mut store = build_store(vec![
            raw(0, 0, "Portfolio:", "", ""),
            raw(0, 1, "WEST", "", ""),
            raw(1, 0, "Username:", "", ""),
            raw(1, 1, "jdoe", "", ""),
            raw(2, 0, "Password:", "", ""),
            raw(2, 1, "lantern", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        detect_regions(&mut store, &candidates);

        let (value_score, _) = score_candidate(&store, 4, 5, &DISTANCE_TABLE);
        let (label_score, reason) = score_candidate(&store, 4, 0, &DISTANCE_TABLE);
        assert!(reason.contains("penalty_label"));
        assert!(value_score > label_score);
    }

    #[test]
    fn whitespace_candidate_penalised() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "Please enter the Password", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(reason.contains("penalty_whitespace"));
    }

    #[test]
    fn distance_component_from_table() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "test", "", ""),
        ]);
        let (score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        // Right-adjacent = 100.0
        assert!(reason.contains("distance=100"));
        assert!(score >= 100.0 - 50.0); // at minimum: 100 - 50(short penalty)
    }

    #[test]
    fn strong_password_gets_all_bonuses() {
        // "P@ssw0rd!" has upper, lower, digit, special, len>=8
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "P@ssw0rd!", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(reason.contains("length>=8"), "missing length bonus");
        assert!(reason.contains("upper"), "missing upper bonus");
        assert!(reason.contains("lower"), "missing lower bonus");
        assert!(reason.contains("digit"), "missing digit bonus");
        assert!(reason.contains("special"), "missing special bonus");
    }

    #[test]
    fn same_region_context_bonus() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "hunter2!", "", "note"),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        detect_regions(&mut store, &candidates);

        // Both should be in the same region
        assert_eq!(store.region_ids[0], store.region_ids[1]);

        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(reason.contains("same_region"), "missing same_region bonus");
    }

    #[test]
    fn username_nearby_bonus() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "s3cr3t!!", "", ""),
            raw(1, 1, "Username", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(
            reason.contains("username_nearby"),
            "missing username_nearby bonus"
        );
    }

    #[test]
    fn url_nearby_bonus() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "s3cr3t!!", "", ""),
            raw(1, 1, "URL", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(reason.contains("url_nearby"), "missing url_nearby bonus");
    }

    #[test]
    fn penalty_candidate_is_header() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "Username", "", ""),
        ]);
        let (score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(
            reason.contains("penalty_header=-80"),
            "missing header penalty"
        );
        // 100 (distance) - 80 (header penalty) - 50 (short, len=8 but "Username" is 8)
        // Actually "Username" is 8 chars, so no short penalty
        assert!(score <= 100.0, "header penalty should reduce score");
    }

    #[test]
    fn penalty_short_value() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "abc", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(
            reason.contains("penalty_short=-50"),
            "missing short penalty"
        );
    }

    #[test]
    fn reason_is_semicolon_separated() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "P@ssw0rd!", "", ""),
        ]);
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(reason.contains(';'), "reason should be semicolon-separated");
        let parts: Vec<&str> = reason.split(';').collect();
        assert!(parts.len() >= 2, "should have multiple components");
    }

    #[test]
    fn entropy_bonus_when_high() {
        // "xK9$mQ2!wP4z" should have high entropy
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "xK9$mQ2!wP4z", "", ""),
        ]);
        assert!(
            store.entropy[1] > ENTROPY_THRESHOLD,
            "entropy should be above threshold"
        );
        let (_score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        assert!(
            reason.contains("high_entropy"),
            "missing entropy bonus in reason"
        );
    }

    #[test]
    fn far_away_cell_low_distance() {
        let store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(3, 3, "s3cr3t!!", "", ""),
        ]);
        let (score, reason) = score_candidate(&store, 0, 1, &DISTANCE_TABLE);
        // (3,3) relative → Euclidean decay: max(0, 60 - sqrt(18)*10) ≈ 17.57
        assert!(reason.contains("distance="));
        // Extract the distance component and verify it's in the expected range
        let dist_str = reason.split(';').next().unwrap();
        let dist_val: f32 = dist_str.strip_prefix("distance=").unwrap().parse().unwrap();
        assert!(
            (dist_val - 17.57).abs() < 0.5,
            "expected ~17.57, got {dist_val}"
        );
        assert!(score < 100.0, "far cell should have lower distance score");
    }
}
