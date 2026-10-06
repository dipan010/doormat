use ahash::AHashMap;
use arrow2::array::{Array, BooleanArray, MutableUtf8Array, UInt32Array, Utf8Array};

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

// ---------- Hybrid Arrow / Vec cell store ----------

/// Columnar cell store with Arrow-backed immutable columns and Vec workspace
/// for mutable pipeline data.
///
/// Immutable columns (set once in `from_raw`, never mutated):
///   rows, cols, values, formulas, comments, sheet_names, merged_flags
///
/// Mutable workspace (written by pipeline phases):
///   normalized_values, feature_flags, entropy, cell_types, region_ids
#[derive(Debug, Clone)]
pub struct CellStore {
    // -- Arrow-backed immutable columns --
    pub(crate) rows: UInt32Array,
    pub(crate) cols: UInt32Array,
    pub(crate) values: Utf8Array<i32>,
    pub(crate) formulas: Utf8Array<i32>,
    pub(crate) comments: Utf8Array<i32>,
    pub(crate) sheet_names: Utf8Array<i32>,
    pub(crate) merged_flags: BooleanArray,

    // -- Mutable workspace (Vec-backed) --
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
    /// Uses `MutableUtf8Array` for write-once string columns (values,
    /// formulas, sheet_names) to build Arrow arrays incrementally during
    /// collection, avoiding the separate Vec→Arrow copy pass.
    /// Comments use `Vec<String>` because FIX 3 comment merge requires
    /// random-access mutation of existing entries.
    pub fn from_raw(cells: Vec<RawCell>) -> Self {
        let cap = cells.len();

        let mut t_rows: Vec<u32> = Vec::with_capacity(cap);
        let mut t_cols: Vec<u32> = Vec::with_capacity(cap);
        // Write-once string columns: build Arrow arrays incrementally
        let mut m_values: MutableUtf8Array<i32> = MutableUtf8Array::with_capacity(cap);
        let mut m_formulas: MutableUtf8Array<i32> = MutableUtf8Array::with_capacity(cap);
        let mut m_sheet_names: MutableUtf8Array<i32> = MutableUtf8Array::with_capacity(cap);
        // Comments need Vec<String> for FIX 3 random-access mutation
        let mut t_comments: Vec<String> = Vec::with_capacity(cap);
        let mut t_merged: Vec<bool> = Vec::with_capacity(cap);

        let mut coord_to_id: AHashMap<(u32, u32), u32> = AHashMap::with_capacity(cap);

        for cell in cells {
            let key = (cell.row, cell.col);
            if let Some(&existing_id) = coord_to_id.get(&key) {
                // FIX 3: merge comment into existing slot
                let id = existing_id as usize;
                if !cell.comment.is_empty() {
                    let existing = &mut t_comments[id];
                    if existing.is_empty() {
                        *existing = cell.comment;
                    } else {
                        existing.push('\n');
                        existing.push_str(&cell.comment);
                    }
                }
                continue;
            }

            let id = t_rows.len() as u32;
            coord_to_id.insert(key, id);

            t_rows.push(cell.row);
            t_cols.push(cell.col);
            // Push into MutableUtf8Array (copies bytes into contiguous
            // buffer incrementally — no second-pass copy needed)
            m_values.push(Some(cell.value.as_str()));
            m_formulas.push(Some(cell.formula.as_str()));
            m_sheet_names.push(Some(cell.sheet_name.as_str()));
            t_comments.push(cell.comment);
            t_merged.push(cell.is_merged_origin);
        }

        let n = t_rows.len();

        // Convert to immutable Arrow arrays
        let rows = UInt32Array::from_vec(t_rows);
        let cols = UInt32Array::from_vec(t_cols);
        let values: Utf8Array<i32> = m_values.into();
        let formulas: Utf8Array<i32> = m_formulas.into();
        let sheet_names: Utf8Array<i32> = m_sheet_names.into();
        // Comments still need the iter copy (Vec<String> for FIX 3)
        let comments = Utf8Array::<i32>::from_iter_values(t_comments.iter().map(|s| s.as_str()));
        let merged_flags = BooleanArray::from_slice(t_merged);

        // Initialize mutable workspace
        let cell_types = vec![CellType::Value as u8; n];
        let normalized_values = vec![String::new(); n];
        let feature_flags = vec![0u16; n];
        let entropy = vec![0.0f32; n];
        let region_ids = vec![-1i32; n];

        CellStore {
            rows,
            cols,
            values,
            formulas,
            comments,
            sheet_names,
            merged_flags,
            cell_types,
            normalized_values,
            feature_flags,
            entropy,
            region_ids,
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

    // -- Accessors for Arrow-backed immutable columns --

    /// Row coordinate for cell `i`.
    #[inline]
    pub fn get_row(&self, i: usize) -> u32 {
        self.rows.value(i)
    }

    /// Column coordinate for cell `i`.
    #[inline]
    pub fn get_col(&self, i: usize) -> u32 {
        self.cols.value(i)
    }

    /// Cell value string for cell `i`.
    #[inline]
    pub fn get_value(&self, i: usize) -> &str {
        self.values.value(i)
    }

    /// Formula string for cell `i`.
    #[inline]
    pub fn get_formula(&self, i: usize) -> &str {
        self.formulas.value(i)
    }

    /// Comment string for cell `i`.
    #[inline]
    pub fn get_comment(&self, i: usize) -> &str {
        self.comments.value(i)
    }

    /// Sheet name for cell `i`.
    #[inline]
    pub fn get_sheet_name(&self, i: usize) -> &str {
        self.sheet_names.value(i)
    }

    /// Merged-origin flag for cell `i`.
    #[inline]
    pub fn get_merged_flag(&self, i: usize) -> bool {
        self.merged_flags.value(i)
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
