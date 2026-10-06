use ahash::AHashMap;

use crate::types::CellType;

// ---------- Raw input from Python ----------

/// A single cell extracted from a spreadsheet, before columnar storage.
#[derive(Debug, Clone)]
pub struct RawCell {
    /// Zero-based row coordinate.
    pub row: u32,
    /// Zero-based column coordinate.
    pub col: u32,
    /// Cell display value as a string.
    pub value: String,
    /// Formula string (e.g., "=SUM(A1:B5)"), empty if no formula.
    pub formula: String,
    /// Comment annotation text, empty if no comment.
    pub comment: String,
    /// Name of the sheet this cell belongs to.
    pub sheet_name: String,
    /// Whether this cell is the origin of a merged cell range.
    pub is_merged_origin: bool,
}

// ---------- Columnar cell store ----------

/// Columnar cell store: one `Vec` per attribute, indexed by cell id.
///
/// Input columns (set once in `from_raw`, never mutated afterwards):
///   rows, cols, values, formulas, comments, sheet_names, merged_flags
///
/// Workspace columns (written by pipeline phases):
///   normalized_values, feature_flags, entropy, cell_types, region_ids
#[derive(Debug, Clone)]
pub struct CellStore {
    // -- Input columns --
    pub(crate) rows: Vec<u32>,
    pub(crate) cols: Vec<u32>,
    pub(crate) values: Vec<String>,
    pub(crate) formulas: Vec<String>,
    pub(crate) comments: Vec<String>,
    pub(crate) sheet_names: Vec<String>,
    pub(crate) merged_flags: Vec<bool>,

    // -- Workspace --
    /// Cell classification (CellType as u8). Written by `classify_cells`.
    pub cell_types: Vec<u8>,
    /// Normalized cell values (lowercased, separators stripped). Written by `precompute_features`.
    pub normalized_values: Vec<String>,
    /// Bitmask feature flags per cell. Written by `precompute_features`.
    pub feature_flags: Vec<u16>,
    /// Shannon entropy per cell. Written by `precompute_features`.
    pub entropy: Vec<f32>,
    /// Region ID per cell (-1 if unassigned). Written by `detect_regions`.
    pub region_ids: Vec<i32>,

    // -- Lookup index --
    /// Map from (row, col) coordinate to cell index for O(1) spatial lookups.
    pub coord_to_id: AHashMap<(u32, u32), u32>,
}

impl CellStore {
    /// Build a `CellStore` from raw cell input.
    ///
    /// Strings are moved out of each `RawCell`, so no cell text is copied.
    /// A repeated (row, col) keeps its first value and appends its comment
    /// to the existing cell (FIX 3).
    pub fn from_raw(cells: Vec<RawCell>) -> Self {
        let cap = cells.len();

        let mut rows: Vec<u32> = Vec::with_capacity(cap);
        let mut cols: Vec<u32> = Vec::with_capacity(cap);
        let mut values: Vec<String> = Vec::with_capacity(cap);
        let mut formulas: Vec<String> = Vec::with_capacity(cap);
        let mut comments: Vec<String> = Vec::with_capacity(cap);
        let mut sheet_names: Vec<String> = Vec::with_capacity(cap);
        let mut merged_flags: Vec<bool> = Vec::with_capacity(cap);

        let mut coord_to_id: AHashMap<(u32, u32), u32> = AHashMap::with_capacity(cap);

        for cell in cells {
            let key = (cell.row, cell.col);
            if let Some(&existing_id) = coord_to_id.get(&key) {
                // FIX 3: merge comment into existing slot
                if !cell.comment.is_empty() {
                    let existing = &mut comments[existing_id as usize];
                    if existing.is_empty() {
                        *existing = cell.comment;
                    } else {
                        existing.push('\n');
                        existing.push_str(&cell.comment);
                    }
                }
                continue;
            }

            let id = u32::try_from(rows.len()).expect("a sheet holds fewer than 2^32 cells");
            coord_to_id.insert(key, id);

            rows.push(cell.row);
            cols.push(cell.col);
            values.push(cell.value);
            formulas.push(cell.formula);
            comments.push(cell.comment);
            sheet_names.push(cell.sheet_name);
            merged_flags.push(cell.is_merged_origin);
        }

        let n = rows.len();

        CellStore {
            rows,
            cols,
            values,
            formulas,
            comments,
            sheet_names,
            merged_flags,
            cell_types: vec![CellType::Value as u8; n],
            normalized_values: vec![String::new(); n],
            feature_flags: vec![0u16; n],
            entropy: vec![0.0f32; n],
            region_ids: vec![-1i32; n],
            coord_to_id,
        }
    }

    /// Number of cells in the store.
    #[inline]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Whether the store is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    // -- Accessors for input columns --

    /// Row coordinate for cell `i`.
    #[inline]
    pub fn get_row(&self, i: usize) -> u32 {
        self.rows[i]
    }

    /// Column coordinate for cell `i`.
    #[inline]
    pub fn get_col(&self, i: usize) -> u32 {
        self.cols[i]
    }

    /// Cell value string for cell `i`.
    #[inline]
    pub fn get_value(&self, i: usize) -> &str {
        &self.values[i]
    }

    /// Formula string for cell `i`.
    #[inline]
    pub fn get_formula(&self, i: usize) -> &str {
        &self.formulas[i]
    }

    /// Comment string for cell `i`.
    #[inline]
    pub fn get_comment(&self, i: usize) -> &str {
        &self.comments[i]
    }

    /// Sheet name for cell `i`.
    #[inline]
    pub fn get_sheet_name(&self, i: usize) -> &str {
        &self.sheet_names[i]
    }

    /// Merged-origin flag for cell `i`.
    #[inline]
    pub fn get_merged_flag(&self, i: usize) -> bool {
        self.merged_flags[i]
    }

    /// Cell type for cell `i` (converted from u8 storage).
    #[inline]
    pub fn get_cell_type(&self, i: usize) -> CellType {
        CellType::from_u8(self.cell_types[i])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_raw(row: u32, col: u32, value: &str, comment: &str) -> RawCell {
        RawCell {
            row,
            col,
            value: value.into(),
            formula: String::new(),
            comment: comment.into(),
            sheet_name: "Sheet1".into(),
            is_merged_origin: false,
        }
    }

    #[test]
    fn from_raw_basic() {
        let cells: Vec<RawCell> = (0..5)
            .map(|i| make_raw(i, 0, &format!("v{i}"), ""))
            .collect();
        let store = CellStore::from_raw(cells);
        assert_eq!(store.len(), 5);
        assert!(!store.is_empty());
    }

    #[test]
    fn from_raw_dedup_coords() {
        let cells = vec![
            make_raw(0, 0, "first", ""),
            make_raw(0, 0, "duplicate", ""),
            make_raw(1, 0, "other", ""),
        ];
        let store = CellStore::from_raw(cells);
        assert_eq!(store.len(), 2);
        // First occurrence wins for the value
        assert_eq!(store.get_value(0), "first");
    }

    #[test]
    fn duplicate_coord_merges_comment() {
        let cells = vec![
            make_raw(0, 0, "val", "note1"),
            make_raw(0, 0, "val", "note2"),
        ];
        let store = CellStore::from_raw(cells);
        assert_eq!(store.len(), 1);
        assert_eq!(store.get_comment(0), "note1\nnote2");
    }

    #[test]
    fn duplicate_coord_empty_comment_no_merge() {
        let cells = vec![make_raw(0, 0, "val", "original"), make_raw(0, 0, "val", "")];
        let store = CellStore::from_raw(cells);
        assert_eq!(store.get_comment(0), "original");
    }

    #[test]
    fn coord_to_id_lookup() {
        let cells = vec![make_raw(3, 7, "target", ""), make_raw(0, 0, "origin", "")];
        let store = CellStore::from_raw(cells);
        let id = store.coord_to_id[&(3, 7)];
        assert_eq!(id, 0);
        assert_eq!(store.get_value(id as usize), "target");
    }

    #[test]
    fn from_raw_empty_input() {
        let store = CellStore::from_raw(vec![]);
        assert_eq!(store.len(), 0);
        assert!(store.is_empty());
    }

    #[test]
    fn accessor_row_col() {
        let store = CellStore::from_raw(vec![make_raw(5, 7, "test", "")]);
        assert_eq!(store.get_row(0), 5);
        assert_eq!(store.get_col(0), 7);
    }

    #[test]
    fn accessor_formula_sheet_merged() {
        let cell = RawCell {
            row: 0,
            col: 0,
            value: "v".into(),
            formula: "=SUM(A1)".into(),
            comment: String::new(),
            sheet_name: "Data".into(),
            is_merged_origin: true,
        };
        let store = CellStore::from_raw(vec![cell]);
        assert_eq!(store.get_formula(0), "=SUM(A1)");
        assert_eq!(store.get_sheet_name(0), "Data");
        assert!(store.get_merged_flag(0));
    }

    #[test]
    fn cell_type_default_is_value() {
        let store = CellStore::from_raw(vec![make_raw(0, 0, "test", "")]);
        assert_eq!(store.get_cell_type(0), CellType::Value);
    }
}
