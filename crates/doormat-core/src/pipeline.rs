use ahash::AHashMap;
use rayon::prelude::*;

use crate::candidates::{classify_cells, reduce_candidate_space};
use crate::detection::{analyze_comments, analyze_formulas, detect_inline_credentials};
use crate::features::precompute_features;
use crate::inference::infer_relationships;
use crate::regions::detect_regions;
use crate::spatial::DISTANCE_TABLE;
use crate::store::{CellStore, RawCell};
use crate::types::{Finding, Relationship};

/// Remove duplicate relationships within one sheet.
///
/// Two relationships are duplicates when they report the same value in the
/// same cell (for example the inline and comment pathways both firing on
/// one cell). The highest-confidence one is kept. Equal values in different
/// cells are separate findings.
pub fn deduplicate(relationships: Vec<Relationship>) -> Vec<Relationship> {
    let mut best: AHashMap<(usize, String), Relationship> = AHashMap::new();

    for rel in relationships {
        best.entry((rel.value_cell_id, rel.value.clone()))
            .and_modify(|existing| {
                if rel.confidence > existing.confidence {
                    *existing = rel.clone();
                }
            })
            .or_insert(rel);
    }

    best.into_values().collect()
}

/// Resolve a relationship's internal cell ids to a located `Finding`.
fn locate(store: &CellStore, rel: Relationship) -> Finding {
    Finding {
        sheet: store.get_sheet_name(rel.value_cell_id).to_string(),
        header_row: store.get_row(rel.header_cell_id),
        header_col: store.get_col(rel.header_cell_id),
        value_row: store.get_row(rel.value_cell_id),
        value_col: store.get_col(rel.value_cell_id),
        key: rel.key,
        value: rel.value,
        confidence: rel.confidence,
        reason: rel.reason,
    }
}

/// Run the full single-sheet pipeline (phases 2-12):
///
/// `from_raw` → `precompute_features` → `reduce_candidate_space` →
/// `classify_cells` → `detect_regions` → `detect_inline_credentials` →
/// `analyze_formulas` → `analyze_comments` → `infer_relationships` →
/// `deduplicate` → locate
///
/// Findings are returned in reading order: by value cell row, then column,
/// then header position, highest confidence first on ties.
pub fn process_sheet(cells: Vec<RawCell>) -> Vec<Finding> {
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

    let mut findings: Vec<Finding> = deduplicate(all_rels)
        .into_iter()
        .map(|rel| locate(&store, rel))
        .collect();
    findings.sort_by(|a, b| {
        (a.value_row, a.value_col, a.header_row, a.header_col)
            .cmp(&(b.value_row, b.value_col, b.header_row, b.header_col))
            .then(b.confidence.total_cmp(&a.confidence))
    });
    findings
}

/// Process multiple sheets in parallel using Rayon. Each sheet runs
/// independently (no shared writes, no mutex) and results are
/// concatenated in sheet order.
pub fn process_workbook(sheets: Vec<Vec<RawCell>>) -> Vec<Finding> {
    let per_sheet: Vec<Vec<Finding>> = sheets.into_par_iter().map(process_sheet).collect();
    per_sheet.into_iter().flatten().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(row: u32, col: u32, value: &str) -> RawCell {
        raw_on("Sheet1", row, col, value)
    }

    fn raw_on(sheet: &str, row: u32, col: u32, value: &str) -> RawCell {
        RawCell {
            row,
            col,
            value: value.into(),
            formula: String::new(),
            comment: String::new(),
            sheet_name: sheet.into(),
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
                value_cell_id: 1,
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

    #[test]
    fn dedup_keeps_same_value_in_different_cells() {
        let rel = |cell: usize| Relationship {
            header_cell_id: cell - 1,
            value_cell_id: cell,
            key: "Password".into(),
            value: "shared1!".into(),
            confidence: 150.0,
            reason: "r".into(),
        };
        assert_eq!(deduplicate(vec![rel(1), rel(3)]).len(), 2);
    }

    // ---------- process_sheet ----------

    #[test]
    fn process_sheet_reports_locations() {
        let cells = vec![
            raw_on("Servers", 3, 1, "Password"),
            raw_on("Servers", 3, 2, "s3cret!!"),
        ];
        let found = process_sheet(cells);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].sheet, "Servers");
        assert_eq!((found[0].header_row, found[0].header_col), (3, 1));
        assert_eq!((found[0].value_row, found[0].value_col), (3, 2));
    }

    #[test]
    fn process_sheet_orders_by_position() {
        let cells = vec![
            raw(5, 1, "password: zz9Top!x"),
            raw(1, 1, "password: aa1Low!y"),
        ];
        let found = process_sheet(cells);
        let rows: Vec<u32> = found.iter().map(|f| f.value_row).collect();
        assert_eq!(rows, vec![1, 5]);
    }

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
    fn process_workbook_keeps_same_credential_on_each_sheet() {
        // The same credential on two sheets is two locations, in sheet order
        let sheet1 = vec![raw_on("A", 1, 1, "Password"), raw_on("A", 1, 2, "s3cret!!")];
        let sheet2 = vec![raw_on("B", 1, 1, "Password"), raw_on("B", 1, 2, "s3cret!!")];
        let found = process_workbook(vec![sheet1, sheet2]);
        let sheets: Vec<&str> = found.iter().map(|f| f.sheet.as_str()).collect();
        assert_eq!(sheets, vec!["A", "B"]);
    }
}
