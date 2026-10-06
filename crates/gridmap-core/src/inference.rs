use ahash::AHashSet;

use crate::scoring::score_candidate;
use crate::spatial::{query_radius, DistanceTable};
use crate::store::CellStore;
use crate::types::*;

/// True when the cell has a non-empty neighbour directly left or right.
///
/// A split-password fragment stands alone in its column. A cell with a
/// horizontal neighbour is a form label (`Post ID:` | `812586`) or a table
/// row holding a complete value of its own.
fn has_horizontal_neighbour(store: &CellStore, row: u32, col: u32) -> bool {
    [col.checked_sub(1), col.checked_add(1)]
        .into_iter()
        .flatten()
        .filter_map(|c| store.coord_to_id.get(&(row, c)))
        .any(|&id| !store.get_value(id as usize).trim().is_empty())
}

/// Walk up to 5 cells directly below `header_id` in the same column.
/// Each cell must have `CellType::Value`, be non-empty, not be a password
/// header, not end in `:`, and have no horizontal neighbour. If >= 2 parts
/// found, each < 30 chars, and the combined string >= 6 chars, returns
/// `(combined_value, first_below_cell_id)`.
fn detect_split_password(store: &CellStore, header_id: u32) -> Option<(String, u32)> {
    let h = header_id as usize;
    let header_row = store.get_row(h);
    let header_col = store.get_col(h);

    let mut parts: Vec<&str> = Vec::new();
    let mut first_id: Option<u32> = None;

    for offset in 1..=5 {
        let target_row = header_row + offset;
        if let Some(&cell_id) = store.coord_to_id.get(&(target_row, header_col)) {
            let id = cell_id as usize;

            // Must be Value type, non-empty, not a password header
            if store.get_cell_type(id) != CellType::Value {
                break;
            }
            let value = store.get_value(id);
            if value.is_empty() {
                break;
            }
            if store.feature_flags[id] & FLAG_IS_PASSWORD_HEADER != 0 {
                break;
            }
            if value.len() >= 30 {
                break;
            }
            if value.trim_end().ends_with(':') {
                break;
            }
            if has_horizontal_neighbour(store, target_row, header_col) {
                break;
            }

            parts.push(value);
            if first_id.is_none() {
                first_id = Some(cell_id);
            }
        } else {
            break;
        }
    }

    if parts.len() < 2 {
        return None;
    }

    let combined: String = parts.concat();
    if combined.len() < 6 {
        return None;
    }

    Some((combined, first_id.expect("first_id set when parts >= 2")))
}

/// Pair password headers with their best-matching neighbor value cells.
///
/// For each cell with `FLAG_IS_PASSWORD_HEADER`:
/// 1. Try `detect_split_password` — if found, emit with confidence 200.0
/// 2. Otherwise, query all neighbors within `NEIGHBOR_RADIUS`
/// 3. Filter to `CellType::Value` only, skip already-used values
/// 4. Score each via `score_candidate`, pick best above `RELATIONSHIP_THRESHOLD`
/// 5. Mark claimed values in `used_value_ids` to prevent double-assignment
pub fn infer_relationships(store: &CellStore, dist_table: &DistanceTable) -> Vec<Relationship> {
    let mut results: Vec<Relationship> = Vec::new();
    let mut used_value_ids: AHashSet<u32> = AHashSet::new();

    // Collect all password header cell IDs
    let header_ids: Vec<u32> = (0..store.len())
        .filter(|&i| store.feature_flags[i] & FLAG_IS_PASSWORD_HEADER != 0)
        .map(|i| i as u32)
        .collect();

    for &header_id in &header_ids {
        let h = header_id as usize;

        // 1. Try split password detection
        if let Some((combined, first_cell_id)) = detect_split_password(store, header_id) {
            used_value_ids.insert(first_cell_id);
            results.push(Relationship {
                header_cell_id: h,
                value_cell_id: first_cell_id as usize,
                key: store.get_value(h).to_string(),
                value: combined,
                confidence: 200.0,
                reason: "split_password".into(),
            });
            continue;
        }

        // 2. Query neighbors within radius
        let neighbors = query_radius(store, store.get_row(h), store.get_col(h), NEIGHBOR_RADIUS);

        // 3. Filter to Value type, not already used
        let mut best_score = f32::NEG_INFINITY;
        let mut best_id: Option<u32> = None;
        let mut best_reason = String::new();

        for &nid in &neighbors {
            let n = nid as usize;
            if store.get_cell_type(n) != CellType::Value {
                continue;
            }
            if used_value_ids.contains(&nid) {
                continue;
            }

            // 4. Score candidate
            let (score, reason) = score_candidate(store, header_id, nid, dist_table);
            if score > best_score {
                best_score = score;
                best_id = Some(nid);
                best_reason = reason;
            }
        }

        // 5. Accept if above threshold
        if let Some(value_id) = best_id {
            if best_score >= RELATIONSHIP_THRESHOLD {
                used_value_ids.insert(value_id);
                results.push(Relationship {
                    header_cell_id: h,
                    value_cell_id: value_id as usize,
                    key: store.get_value(h).to_string(),
                    value: store.get_value(value_id as usize).to_string(),
                    confidence: best_score,
                    reason: best_reason,
                });
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{classify_cells, reduce_candidate_space};
    use crate::features::precompute_features;
    use crate::regions::detect_regions;
    use crate::spatial::DISTANCE_TABLE;
    use crate::store::RawCell;

    fn raw(row: u32, col: u32, value: &str) -> RawCell {
        RawCell {
            row,
            col,
            value: value.into(),
            formula: String::new(),
            comment: String::new(),
            sheet_name: "Sheet1".into(),
            is_merged_origin: false,
        }
    }

    fn pipeline_store(cells: Vec<RawCell>) -> CellStore {
        let mut store = CellStore::from_raw(cells);
        precompute_features(&mut store);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        detect_regions(&mut store, &candidates);
        store
    }

    #[test]
    fn header_pairs_with_adjacent_value() {
        let store = pipeline_store(vec![raw(1, 1, "Password"), raw(1, 2, "s3cret!!")]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].key, "Password");
        assert_eq!(rels[0].value, "s3cret!!");
        assert!(rels[0].confidence >= RELATIONSHIP_THRESHOLD);
    }

    #[test]
    fn split_password_three_parts() {
        let store = pipeline_store(vec![
            raw(0, 0, "Password"),
            raw(1, 0, "ab"),
            raw(2, 0, "cd"),
            raw(3, 0, "ef"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "abcdef");
        assert_eq!(rels[0].confidence, 200.0);
        assert_eq!(rels[0].reason, "split_password");
    }

    #[test]
    fn split_password_needs_two_parts() {
        // Only 1 cell below → no split
        let store = pipeline_store(vec![raw(0, 0, "Password"), raw(1, 0, "abcdef")]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        // Should fall through to normal scoring, not split
        if !rels.is_empty() {
            assert_ne!(rels[0].reason, "split_password");
        }
    }

    #[test]
    fn split_password_combined_too_short() {
        // Two parts but combined only 4 chars < 6
        let store = pipeline_store(vec![
            raw(0, 0, "Password"),
            raw(1, 0, "ab"),
            raw(2, 0, "cd"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        // "abcd" is 4 chars < 6, no split
        for rel in &rels {
            assert_ne!(rel.reason, "split_password");
        }
    }

    #[test]
    fn two_headers_one_value_no_double_claim() {
        // Two password headers adjacent to the same value cell
        let store = pipeline_store(vec![
            raw(0, 0, "Password"),
            raw(0, 1, "s3cret!!"),
            raw(0, 2, "Password"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        // Value should be claimed by at most one header
        let value_ids: Vec<usize> = rels.iter().map(|r| r.value_cell_id).collect();
        let unique: AHashSet<usize> = value_ids.iter().copied().collect();
        assert_eq!(value_ids.len(), unique.len(), "no double-assignment");
    }

    #[test]
    fn no_value_cells_nearby() {
        // Only headers, no value cells
        let store = pipeline_store(vec![raw(0, 0, "Password"), raw(0, 1, "Username")]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        // Both are headers, neither should pair with the other
        assert!(rels.is_empty());
    }

    #[test]
    fn value_beyond_radius_not_found() {
        // Value at distance > NEIGHBOR_RADIUS (3)
        let store = pipeline_store(vec![raw(0, 0, "Password"), raw(0, 10, "s3cret!!")]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.is_empty());
    }

    #[test]
    fn split_password_stops_at_password_header() {
        // Cell below header is itself a password header → stops walk
        let store = pipeline_store(vec![
            raw(0, 0, "Password"),
            raw(1, 0, "Token"), // password header → breaks split walk
            raw(2, 0, "ab"),
            raw(3, 0, "cd"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        for rel in &rels {
            if rel.header_cell_id == 0 {
                assert_ne!(rel.reason, "split_password");
            }
        }
    }

    #[test]
    fn split_password_rejects_form_labels() {
        // Labels below the header each have their own value to the right
        let store = pipeline_store(vec![
            raw(0, 0, "Password:"),
            raw(1, 0, "Price"),
            raw(1, 1, "123456"),
            raw(2, 0, "Basis"),
            raw(2, 1, "12"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.iter().all(|r| r.reason != "split_password"));
    }

    #[test]
    fn split_password_rejects_table_column() {
        // A password column in a table: each row is a separate credential
        let store = pipeline_store(vec![
            raw(0, 0, "User ID"),
            raw(0, 1, "Password"),
            raw(1, 0, "usr1001"),
            raw(1, 1, "alpha7"),
            raw(2, 0, "usr1002"),
            raw(2, 1, "bravo8"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.iter().all(|r| r.reason != "split_password"));
    }

    #[test]
    fn split_password_rejects_colon_labels() {
        let store = pipeline_store(vec![
            raw(0, 0, "Password:"),
            raw(1, 0, "Post ID:"),
            raw(2, 0, "Region"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.iter().all(|r| r.reason != "split_password"));
    }

    #[test]
    fn split_password_stops_at_long_part() {
        // A part that is >= 30 chars → breaks the walk
        let long = "a".repeat(30);
        let store = pipeline_store(vec![
            raw(0, 0, "Password"),
            raw(1, 0, &long),
            raw(2, 0, "cd"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        for rel in &rels {
            assert_ne!(rel.reason, "split_password");
        }
    }
}
