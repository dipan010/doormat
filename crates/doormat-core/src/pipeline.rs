use ahash::AHashMap;
use rayon::prelude::*;

use crate::candidates::{classify_cells, reduce_candidate_space};
use crate::detection::{analyze_comments, analyze_formulas, detect_inline_credentials};
use crate::features::precompute_features;
use crate::inference::infer_relationships;
use crate::regions::detect_regions;
use crate::spatial::DISTANCE_TABLE;
use crate::store::{CellStore, RawCell};
use crate::types::Relationship;

/// Remove duplicate relationships by keeping only the highest-confidence
/// result for each unique value string.
pub fn deduplicate(relationships: Vec<Relationship>) -> Vec<Relationship> {
    let mut best: AHashMap<String, Relationship> = AHashMap::new();

    for rel in relationships {
        best.entry(rel.value.clone())
            .and_modify(|existing| {
                if rel.confidence > existing.confidence {
                    *existing = rel.clone();
                }
            })
            .or_insert(rel);
    }

    best.into_values().collect()
}

/// Run the full single-sheet pipeline (phases 2–12):
///
/// `from_raw` → `precompute_features` → `reduce_candidate_space` →
/// `classify_cells` → `detect_regions` → `detect_inline_credentials` →
/// `analyze_formulas` → `analyze_comments` → `infer_relationships` →
/// `deduplicate`
pub fn process_sheet(cells: Vec<RawCell>) -> Vec<Relationship> {
    if cells.is_empty() {
        return Vec::new();
    }

    let mut store = CellStore::from_raw(cells);
    precompute_features(&mut store);
    let candidates = reduce_candidate_space(&store);
    classify_cells(&mut store, &candidates);
    detect_regions(&mut store, &candidates);

    let mut all_rels = Vec::new();
    all_rels.extend(detect_inline_credentials(&store, &candidates));
    all_rels.extend(analyze_formulas(&store, &candidates));
    all_rels.extend(analyze_comments(&store, &candidates));
    all_rels.extend(infer_relationships(&store, &DISTANCE_TABLE));

    deduplicate(all_rels)
}

/// Process multiple sheets in parallel using Rayon, then deduplicate
/// across all sheets. Each sheet runs independently (GAP 1 fix: no
/// shared writes, no mutex).
pub fn process_workbook(sheets: Vec<Vec<RawCell>>) -> Vec<Relationship> {
    let per_sheet: Vec<Vec<Relationship>> = sheets.into_par_iter().map(process_sheet).collect();

    let all_rels: Vec<Relationship> = per_sheet.into_iter().flatten().collect();
    deduplicate(all_rels)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    // ---------- deduplicate ----------

    #[test]
    fn dedup_keeps_higher_confidence() {
        let rels = vec![
            Relationship {
                header_cell_id: 0,
                value_cell_id: 1,
                key: "Password".into(),
                value: "s3cret!!".into(),
                confidence: 100.0,
                reason: "low".into(),
            },
            Relationship {
                header_cell_id: 2,
                value_cell_id: 3,
                key: "Pwd".into(),
                value: "s3cret!!".into(),
                confidence: 200.0,
                reason: "high".into(),
            },
        ];
        let result = deduplicate(rels);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].confidence, 200.0);
        assert_eq!(result[0].reason, "high");
    }

    #[test]
    fn dedup_keeps_both_different_values() {
        let rels = vec![
            Relationship {
                header_cell_id: 0,
                value_cell_id: 1,
                key: "Password".into(),
                value: "alpha123".into(),
                confidence: 150.0,
                reason: "a".into(),
            },
            Relationship {
                header_cell_id: 2,
                value_cell_id: 3,
                key: "Token".into(),
                value: "beta456!".into(),
                confidence: 160.0,
                reason: "b".into(),
            },
        ];
        let result = deduplicate(rels);
        assert_eq!(result.len(), 2);
    }

    // ---------- process_sheet ----------

    #[test]
    fn process_sheet_finds_password() {
        let cells = vec![raw(1, 1, "Password"), raw(1, 2, "s3cret!!")];
        let rels = process_sheet(cells);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].key, "Password");
        assert_eq!(rels[0].value, "s3cret!!");
    }

    #[test]
    fn process_sheet_no_credentials() {
        let cells = vec![
            raw(0, 0, "Name"),
            raw(0, 1, "Alice"),
            raw(1, 0, "Age"),
            raw(1, 1, "30"),
        ];
        let rels = process_sheet(cells);
        assert!(rels.is_empty());
    }

    #[test]
    fn process_sheet_empty_input() {
        let rels = process_sheet(Vec::new());
        assert!(rels.is_empty());
    }

    // ---------- process_workbook ----------

    #[test]
    fn process_workbook_two_sheets() {
        let sheet1 = vec![raw(0, 0, "Password"), raw(0, 1, "s3cret!!")];
        let sheet2 = vec![raw(0, 0, "Password"), raw(0, 1, "hunter2!")];
        let rels = process_workbook(vec![sheet1, sheet2]);
        // Two different values → both kept
        assert_eq!(rels.len(), 2);
        let values: Vec<&str> = rels.iter().map(|r| r.value.as_str()).collect();
        assert!(values.contains(&"s3cret!!"));
        assert!(values.contains(&"hunter2!"));
    }

    #[test]
    fn process_workbook_empty_sheets() {
        let rels = process_workbook(Vec::new());
        assert!(rels.is_empty());
    }

    #[test]
    fn process_workbook_deduplicates_across_sheets() {
        // Same credential on two sheets → deduplicated to one
        let sheet1 = vec![raw(0, 0, "Password"), raw(0, 1, "s3cret!!")];
        let sheet2 = vec![raw(0, 0, "Password"), raw(0, 1, "s3cret!!")];
        let rels = process_workbook(vec![sheet1, sheet2]);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "s3cret!!");
    }
}
