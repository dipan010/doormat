use std::collections::VecDeque;

use ahash::AHashSet;

use crate::spatial::query_radius;
use crate::store::CellStore;
use crate::types::*;

/// Cluster spatially adjacent candidate cells into regions using BFS.
///
/// Each unvisited candidate seeds a new region. Neighbor expansion uses
/// `query_radius` with `NEIGHBOR_RADIUS`. Only candidate cells join the
/// BFS queue, but `query_radius` reads from the full `coord_to_id` (FIX 2).
/// Uses `VecDeque` for O(1) pop_front (FIX 1 / WIN 1).
pub fn detect_regions(store: &mut CellStore, candidate_ids: &[u32]) -> Vec<Region> {
    let candidate_set: AHashSet<u32> = candidate_ids.iter().copied().collect();
    let mut visited: AHashSet<u32> = AHashSet::with_capacity(candidate_ids.len());
    let mut regions: Vec<Region> = Vec::new();

    for &seed in candidate_ids {
        if visited.contains(&seed) {
            continue;
        }

        let region_id = regions.len();
        let mut cell_ids: Vec<usize> = Vec::new();
        let mut header_ids: Vec<usize> = Vec::new();
        let mut value_ids: Vec<usize> = Vec::new();

        // FIX 1 / WIN 1: VecDeque for O(1) pop_front
        let mut queue: VecDeque<u32> = VecDeque::new();
        queue.push_back(seed);
        visited.insert(seed);

        while let Some(cell_id) = queue.pop_front() {
            let id = cell_id as usize;
            cell_ids.push(id);
            store.region_ids[id] = region_id as i32;

            match store.get_cell_type(id) {
                CellType::Header => header_ids.push(id),
                CellType::Value => value_ids.push(id),
                _ => {}
            }

            let row = store.get_row(id);
            let col = store.get_col(id);
            let neighbors = query_radius(store, row, col, NEIGHBOR_RADIUS);

            for neighbor_id in neighbors {
                if candidate_set.contains(&neighbor_id) && visited.insert(neighbor_id) {
                    queue.push_back(neighbor_id);
                }
            }
        }

        regions.push(Region {
            region_id,
            cell_ids,
            header_ids,
            value_ids,
        });
    }

    regions
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::{classify_cells, reduce_candidate_space};
    use crate::features::precompute_features;
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
    fn two_adjacent_candidates_one_region() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "secret123", "", "note"),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].cell_ids.len(), 2);
    }

    #[test]
    fn two_candidates_far_apart_two_regions() {
        // Place cells far enough apart that NEIGHBOR_RADIUS=3 can't bridge them
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 10, "Token", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].cell_ids.len(), 1);
        assert_eq!(regions[1].cell_ids.len(), 1);
    }

    #[test]
    fn header_in_region_header_ids() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "hunter2", "", "note"),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 1);
        assert!(!regions[0].header_ids.is_empty());
        // "Password" should be classified as Header
        let pwd_id = store.coord_to_id[&(0, 0)] as usize;
        assert!(regions[0].header_ids.contains(&pwd_id));
    }

    #[test]
    fn value_in_region_value_ids() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "hunter2", "", "note"),
            raw(0, 2, "extra", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 1);
        // "hunter2" has a comment → candidate → Value (row has 3 cells, not section)
        assert!(!regions[0].value_ids.is_empty());
    }

    #[test]
    fn store_region_ids_updated() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "data", "", "note"),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 1);
        for &cell_id in &regions[0].cell_ids {
            assert_eq!(store.region_ids[cell_id], 0);
        }
    }

    #[test]
    fn empty_candidates_empty_regions() {
        let mut store = build_store(vec![raw(0, 0, "Hello", "", "")]);
        let regions = detect_regions(&mut store, &[]);
        assert!(regions.is_empty());
    }

    #[test]
    fn single_isolated_candidate() {
        let mut store = build_store(vec![raw(0, 0, "Password", "", "")]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].cell_ids.len(), 1);
        assert_eq!(regions[0].region_id, 0);
    }

    #[test]
    fn region_ids_sequential() {
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 10, "Token", "", ""),
            raw(0, 20, "Secret", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        assert_eq!(regions.len(), 3);
        for (i, region) in regions.iter().enumerate() {
            assert_eq!(region.region_id, i);
        }
    }

    #[test]
    fn non_candidate_neighbor_does_not_join() {
        // "Password" is a candidate, "Hello" is not, "Token" is a candidate
        // "Hello" sits between them but should not join the BFS
        let mut store = build_store(vec![
            raw(0, 0, "Password", "", ""),
            raw(0, 1, "Hello", "", ""),
            raw(0, 2, "Token", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        let regions = detect_regions(&mut store, &candidates);

        // Password and Token are within radius 3, so BFS from Password
        // should find Token via query_radius (even though Hello is not a candidate,
        // both Password and Token are within NEIGHBOR_RADIUS of each other)
        assert_eq!(regions.len(), 1);
        assert_eq!(regions[0].cell_ids.len(), 2);
    }
}
