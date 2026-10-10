use std::sync::LazyLock;

use crate::store::CellStore;
use crate::types::NEIGHBOR_RADIUS;

const TABLE_SIZE: usize = (2 * NEIGHBOR_RADIUS + 1) as usize; // 7

// ---------- Distance table ----------

/// Pre-computed spatial scoring table for cell-to-cell distance within
/// `NEIGHBOR_RADIUS`. Indexed by `(dr + RADIUS, dc + RADIUS)`.
#[derive(Debug, Clone)]
pub struct DistanceTable {
    data: [[f32; TABLE_SIZE]; TABLE_SIZE],
}

impl DistanceTable {
    /// Build a new distance table with pre-computed scores for all offsets.
    pub fn new() -> Self {
        let mut data = [[0.0_f32; TABLE_SIZE]; TABLE_SIZE];

        for dr in -NEIGHBOR_RADIUS..=NEIGHBOR_RADIUS {
            for dc in -NEIGHBOR_RADIUS..=NEIGHBOR_RADIUS {
                let r = (dr + NEIGHBOR_RADIUS) as usize;
                let c = (dc + NEIGHBOR_RADIUS) as usize;
                data[r][c] = Self::compute_score(dr, dc);
            }
        }

        DistanceTable { data }
    }

    fn compute_score(dr: i32, dc: i32) -> f32 {
        match (dr, dc) {
            // Self
            (0, 0) => 0.0,
            // Right adjacent
            (0, 1) => 100.0,
            // Below adjacent
            (1, 0) => 95.0,
            // Left adjacent
            (0, -1) => 80.0,
            // Above adjacent
            (-1, 0) => 70.0,
            // Diagonal adjacent
            (dr, dc) if dr.abs() == 1 && dc.abs() == 1 => 70.0,
            // Same column, farther than adjacent: gradual decay
            (dr, 0) => f32::max(0.0, 90.0 - (dr.abs() - 1) as f32 * 15.0),
            // Same row, farther than adjacent: gradual decay
            (0, dc) => f32::max(0.0, 90.0 - (dc.abs() - 1) as f32 * 15.0),
            // General case: Euclidean decay
            (dr, dc) => f32::max(0.0, 60.0 - ((dr * dr + dc * dc) as f32).sqrt() * 10.0),
        }
    }

    /// Look up the pre-computed score for a (dr, dc) offset.
    /// Returns 0.0 for offsets outside the radius.
    pub fn score(&self, dr: i32, dc: i32) -> f32 {
        if dr.abs() > NEIGHBOR_RADIUS || dc.abs() > NEIGHBOR_RADIUS {
            return 0.0;
        }
        self.data[(dr + NEIGHBOR_RADIUS) as usize][(dc + NEIGHBOR_RADIUS) as usize]
    }
}

impl Default for DistanceTable {
    fn default() -> Self {
        Self::new()
    }
}

/// Global lazily-initialized distance table singleton.
pub static DISTANCE_TABLE: LazyLock<DistanceTable> = LazyLock::new(DistanceTable::new);

// ---------- Neighbor query ----------

/// Returns cell_ids of all cells within `radius` of (row, col),
/// excluding the center cell itself. Queries coord_to_id which
/// contains every cell, not only candidates.
pub fn query_radius(store: &CellStore, row: u32, col: u32, radius: i32) -> Vec<u32> {
    let mut result = Vec::new();
    for_each_neighbor(store, row, col, radius, |cell_id| {
        result.push(cell_id);
    });
    result
}

/// Iterate neighbors without allocating a Vec.
/// Calls `f(cell_id)` for each cell within `radius` of (row, col),
/// excluding the center cell itself.
#[inline]
pub fn for_each_neighbor<F>(store: &CellStore, row: u32, col: u32, radius: i32, mut f: F)
where
    F: FnMut(u32),
{
    for dr in -radius..=radius {
        for dc in -radius..=radius {
            if dr == 0 && dc == 0 {
                continue;
            }

            let nr = row as i32 + dr;
            let nc = col as i32 + dc;

            if nr < 0 || nc < 0 {
                continue;
            }

            if let Some(&cell_id) = store.coord_to_id.get(&(nr as u32, nc as u32)) {
                f(cell_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::RawCell;

    #[test]
    fn score_right_adjacent() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(0, 1), 100.0);
    }

    #[test]
    fn score_left_adjacent() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(0, -1), 80.0);
    }

    #[test]
    fn score_below_adjacent() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(1, 0), 95.0);
    }

    #[test]
    fn score_above_adjacent() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(-1, 0), 70.0);
    }

    #[test]
    fn score_self_is_zero() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(0, 0), 0.0);
    }

    #[test]
    fn score_diagonal() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(1, 1), 70.0);
        assert_eq!(dt.score(-1, -1), 70.0);
        assert_eq!(dt.score(1, -1), 70.0);
        assert_eq!(dt.score(-1, 1), 70.0);
    }

    #[test]
    fn score_same_column_decay() {
        let dt = DistanceTable::new();
        // |dr|=2, dc=0 → max(0, 90 - (2-1)*15) = 75
        assert_eq!(dt.score(2, 0), 75.0);
        assert_eq!(dt.score(-2, 0), 75.0);
        // |dr|=3, dc=0 → max(0, 90 - (3-1)*15) = 60
        assert_eq!(dt.score(3, 0), 60.0);
        assert_eq!(dt.score(-3, 0), 60.0);
    }

    #[test]
    fn score_same_row_decay() {
        let dt = DistanceTable::new();
        // dr=0, |dc|=2 → max(0, 90 - (2-1)*15) = 75
        assert_eq!(dt.score(0, 2), 75.0);
        assert_eq!(dt.score(0, -2), 75.0);
        // dr=0, |dc|=3 → max(0, 90 - (3-1)*15) = 60
        assert_eq!(dt.score(0, 3), 60.0);
        assert_eq!(dt.score(0, -3), 60.0);
    }

    #[test]
    fn score_general_euclidean_decay() {
        let dt = DistanceTable::new();
        // (2,1) → max(0, 60 - sqrt(5)*10) ≈ 37.64
        let s = dt.score(2, 1);
        assert!((s - 37.64).abs() < 0.1, "expected ~37.64, got {s}");
        // (2,2) → max(0, 60 - sqrt(8)*10) ≈ 31.72
        let s = dt.score(2, 2);
        assert!((s - 31.72).abs() < 0.1, "expected ~31.72, got {s}");
        // (3,3) → max(0, 60 - sqrt(18)*10) ≈ 17.57
        let s = dt.score(3, 3);
        assert!((s - 17.57).abs() < 0.1, "expected ~17.57, got {s}");
    }

    #[test]
    fn score_negative_indices_no_panic() {
        let dt = DistanceTable::new();
        // All negative offsets within radius should return valid scores
        assert!(dt.score(-3, -3) > 0.0);
        assert!(dt.score(-2, -1) > 0.0);
    }

    #[test]
    fn score_out_of_radius_returns_zero() {
        let dt = DistanceTable::new();
        assert_eq!(dt.score(4, 0), 0.0);
        assert_eq!(dt.score(0, -4), 0.0);
    }

    #[test]
    fn global_table_accessible() {
        assert_eq!(DISTANCE_TABLE.score(0, 1), 100.0);
    }

    fn make_raw(row: u32, col: u32) -> RawCell {
        RawCell {
            row,
            col,
            value: format!("({row},{col})"),
            formula: String::new(),
            comment: String::new(),
            sheet_name: "Sheet1".into(),
            is_merged_origin: false,
        }
    }

    #[test]
    fn query_radius_finds_adjacent() {
        let cells = vec![
            make_raw(5, 5), // center
            make_raw(5, 6), // right
            make_raw(6, 5), // below
            make_raw(4, 5), // above
        ];
        let store = CellStore::from_raw(cells);
        let neighbors = query_radius(&store, 5, 5, NEIGHBOR_RADIUS);
        assert_eq!(neighbors.len(), 3);
    }

    #[test]
    fn query_radius_excludes_center() {
        let cells = vec![make_raw(5, 5), make_raw(5, 6)];
        let store = CellStore::from_raw(cells);
        let neighbors = query_radius(&store, 5, 5, NEIGHBOR_RADIUS);
        // Should only contain the right neighbor, not the center
        let center_id = store.coord_to_id[&(5, 5)];
        assert!(!neighbors.contains(&center_id));
        assert_eq!(neighbors.len(), 1);
    }

    #[test]
    fn query_radius_empty_region() {
        let cells = vec![make_raw(0, 0)];
        let store = CellStore::from_raw(cells);
        // Query far from any cell
        let neighbors = query_radius(&store, 100, 100, NEIGHBOR_RADIUS);
        assert!(neighbors.is_empty());
    }
}
