use ahash::{AHashMap, AHashSet};
use rayon::prelude::*;

use crate::candidates::{classify_cells, reduce_candidate_space, LOOSE_INLINE_REGEX};
use crate::detection::{analyze_comments, analyze_formulas, detect_inline_credentials};
use crate::features::{
    char_class_counts, compute_entropy, has_non_secret_part, is_ambiguous_label,
    is_boolean_literal, is_compound_credential_label, is_date_like, is_exact_keyword,
    is_numeric_amount, is_password_label, is_url_or_email, keyword_class, label_has_word,
    normalize, precompute_features,
};
use crate::inference::{
    detect_split_password, detect_table_column, detect_variable_values, filled, is_table_label,
};
use crate::pipeline::process_sheet;
use crate::regions::detect_regions;
use crate::spatial::{query_radius, DISTANCE_TABLE};
use crate::store::{CellStore, RawCell};
use crate::types::*;

/// A candidate before its features: label and value cell ids, the rule that
/// proposed it, the value text and the label text.
struct Proposal {
    label: usize,
    value: usize,
    pathway: Pathway,
    text: String,
    label_text: String,
}

/// Facts about a sheet that several candidates' features share.
struct SheetContext {
    /// Highest baseline confidence per value cell, from `process_sheet`.
    baseline: AHashMap<(u32, u32), f32>,
    /// Cells per row that are password, username or URL labels.
    row_labels: AHashMap<u32, u32>,
    /// Password labels in the sheet.
    sheet_labels: u32,
    /// Value cells of each password label's qualifying credential table.
    tables: AHashMap<usize, AHashSet<u32>>,
    /// Column statistics per value cell (length, distinct share, shape share).
    columns: AHashMap<usize, (f32, f32, f32)>,
}

const LABEL_FLAGS: u16 = FLAG_IS_PASSWORD_HEADER | FLAG_IS_USERNAME_HEADER | FLAG_IS_URL_HEADER;
const SHAPE_FLAGS: u16 = FLAG_HAS_UPPER | FLAG_HAS_LOWER | FLAG_HAS_DIGIT | FLAG_HAS_SPECIAL;

fn proposal(store: &CellStore, label: usize, value: usize, pathway: Pathway) -> Proposal {
    Proposal {
        label,
        value,
        pathway,
        text: store.get_value(value).to_string(),
        label_text: store.get_value(label).to_string(),
    }
}

fn from_relationship(rel: Relationship, pathway: Pathway) -> Proposal {
    Proposal {
        label: rel.header_cell_id,
        value: rel.value_cell_id,
        pathway,
        text: rel.value,
        label_text: rel.key,
    }
}

/// The first assignment-style and the first prose-style loose match in a cell.
fn loose_inline(store: &CellStore, id: usize) -> Vec<Proposal> {
    let value = store.get_value(id);
    let mut seen = AHashSet::new();
    LOOSE_INLINE_REGEX
        .captures_iter(value)
        .filter_map(|caps| {
            let (separator, pathway) = match caps.get(1) {
                Some(m) => (m, Pathway::Inline),
                None => (caps.get(2)?, Pathway::Prose),
            };
            let token = caps.get(3)?;
            seen.insert(pathway).then(|| Proposal {
                label: id,
                value: id,
                pathway,
                text: token.as_str().to_string(),
                label_text: value[..separator.start()].trim().to_string(),
            })
        })
        .collect()
}

/// Filled cells in the column below a label, down to a new label or more
/// than `GAP_MAX_EMPTY` empty rows, whether or not they form a table.
fn column_below(store: &CellStore, label: usize) -> Vec<u32> {
    let (row, col) = (store.get_row(label), store.get_col(label));
    let mut gap = 0;
    let mut cells = Vec::new();
    for r in row + 1..=row.saturating_add(TABLE_MAX_ROWS) {
        match filled(store, r, col) {
            Some(id) if is_table_label(store, id as usize) => break,
            Some(id) => {
                gap = 0;
                cells.push(id);
            }
            None => {
                gap += 1;
                if gap > GAP_MAX_EMPTY {
                    break;
                }
            }
        }
    }
    cells
}

/// The first filled cell right of and below a label, at most `GAP_MAX_EMPTY`
/// empty cells away. A password label's neighbours are spatial candidates, so
/// its gap values start two cells away; any other label's start next to it.
fn gap_values(store: &CellStore, label: usize) -> Vec<u32> {
    let (row, col) = (store.get_row(label), store.get_col(label));
    let nearest = if store.feature_flags[label] & FLAG_IS_PASSWORD_HEADER != 0 {
        2
    } else {
        1
    };
    [(0, 1), (1, 0)]
        .into_iter()
        .filter_map(|(dr, dc)| {
            (1..=GAP_MAX_EMPTY + 1)
                .find_map(|d| {
                    filled(
                        store,
                        row.saturating_add(d * dr),
                        col.saturating_add(d * dc),
                    )
                    .map(|id| (d, id))
                })
                .filter(|&(d, _)| d >= nearest)
                .map(|(_, id)| id)
        })
        .collect()
}

/// True when a cell that is not a password label can head a column of
/// secrets: at most three words, one of them a whole `COLUMN_HEAD_WORDS` word
/// (`login_totp`), and nothing filled directly above it (a header position).
fn is_column_head(store: &CellStore, id: usize) -> bool {
    let (row, col) = (store.get_row(id), store.get_col(id));
    let value = store.get_value(id);
    value.split_whitespace().count() <= 3
        && label_has_word(value, COLUMN_HEAD_WORDS)
        && row
            .checked_sub(1)
            .is_none_or(|above| filled(store, above, col).is_none())
}

/// Every candidate the research generator proposes for one sheet, in the
/// order: fixed-rule single-cell pathways, loose single-cell matches, then
/// each label's pathways.
fn propose(store: &CellStore, candidate_ids: &[u32]) -> Vec<Proposal> {
    let mut out: Vec<Proposal> = Vec::new();
    out.extend(
        detect_inline_credentials(store, candidate_ids)
            .into_iter()
            .map(|rel| from_relationship(rel, Pathway::Inline)),
    );
    out.extend(
        analyze_formulas(store, candidate_ids)
            .into_iter()
            .map(|rel| from_relationship(rel, Pathway::Formula)),
    );
    out.extend(
        analyze_comments(store, candidate_ids)
            .into_iter()
            .map(|rel| from_relationship(rel, Pathway::Comment)),
    );
    out.extend(
        (0..store.len())
            .filter(|&i| store.get_formula(i).is_empty())
            .flat_map(|i| loose_inline(store, i)),
    );

    let labels = (0..store.len())
        .filter(|&i| store.feature_flags[i] & (FLAG_IS_PASSWORD_HEADER | FLAG_IS_KEY_LABEL) != 0);
    for h in labels {
        if let Some(values) = detect_variable_values(store, h as u32) {
            out.extend(
                values
                    .into_iter()
                    .map(|v| proposal(store, h, v as usize, Pathway::Variable)),
            );
        }
        if store.feature_flags[h] & FLAG_IS_PASSWORD_HEADER == 0 {
            continue;
        }
        out.extend(
            detect_table_column(store, h as u32)
                .into_iter()
                .chain(column_below(store, h))
                .map(|v| proposal(store, h, v as usize, Pathway::Table)),
        );
        if let Some((combined, first)) = detect_split_password(store, h as u32) {
            out.push(Proposal {
                text: combined,
                ..proposal(store, h, first as usize, Pathway::Split)
            });
        }
        out.extend(
            query_radius(store, store.get_row(h), store.get_col(h), NEIGHBOR_RADIUS)
                .into_iter()
                .map(|n| n as usize)
                .filter(|&n| {
                    store.get_cell_type(n) == CellType::Value
                        && store.get_formula(n).is_empty()
                        && !is_boolean_literal(&store.normalized_values[n])
                })
                .map(|n| proposal(store, h, n, Pathway::Spatial)),
        );
    }

    // A label written as prose (`The password is:`) is not a password label,
    // so gaps start from any cell holding a credential word, and next to it.
    let gap_labels = (0..store.len()).filter(|&i| {
        store.feature_flags[i] & (FLAG_IS_PASSWORD_HEADER | FLAG_HAS_CREDENTIAL_WORD) != 0
            && store.get_formula(i).is_empty()
    });
    for h in gap_labels {
        out.extend(
            gap_values(store, h)
                .into_iter()
                .map(|v| proposal(store, h, v as usize, Pathway::Gap)),
        );
        // A header that is not a password label (`login_totp`) may still head
        // a column of secrets.
        if store.feature_flags[h] & FLAG_IS_PASSWORD_HEADER == 0 && is_column_head(store, h) {
            out.extend(
                column_below(store, h)
                    .into_iter()
                    .map(|v| proposal(store, h, v as usize, Pathway::Table)),
            );
        }
    }
    out
}

/// Length, share of distinct values and share of cells with the same
/// character-class pattern, over the unbroken run of filled cells in the
/// value's column.
fn column_stats(store: &CellStore, value: usize) -> (f32, f32, f32) {
    let (row, col) = (store.get_row(value), store.get_col(value));
    let up = (1..=row)
        .map_while(|d| filled(store, row - d, col))
        .collect::<Vec<_>>();
    let down = (1..=TABLE_MAX_ROWS)
        .map_while(|d| filled(store, row.checked_add(d)?, col))
        .collect::<Vec<_>>();
    let run: Vec<usize> = up
        .into_iter()
        .chain(std::iter::once(value as u32))
        .chain(down)
        .map(|id| id as usize)
        .collect();
    let len = run.len() as f32;
    let distinct = run
        .iter()
        .map(|&id| store.normalized_values[id].as_str())
        .collect::<AHashSet<_>>()
        .len() as f32;
    let shape = store.feature_flags[value] & SHAPE_FLAGS;
    let same_shape = run
        .iter()
        .filter(|&&id| store.feature_flags[id] & SHAPE_FLAGS == shape)
        .count() as f32;
    (len, distinct / len, same_shape / len)
}

/// Empty cells strictly between a label and a value on one row or column.
fn empty_between(store: &CellStore, label: usize, value: usize) -> u32 {
    let (lr, lc) = (store.get_row(label), store.get_col(label));
    let (vr, vc) = (store.get_row(value), store.get_col(value));
    let empty = |r: u32, c: u32| u32::from(filled(store, r, c).is_none());
    if lr == vr && vc > lc {
        (lc + 1..vc).map(|c| empty(lr, c)).sum()
    } else if lc == vc && vr > lr {
        (lr + 1..vr).map(|r| empty(r, lc)).sum()
    } else {
        0
    }
}

fn flag(b: bool) -> f32 {
    f32::from(u8::from(b))
}

/// The feature vector of one proposal, in `FEATURE_NAMES` order.
fn features(store: &CellStore, p: &Proposal, ctx: &mut SheetContext) -> [f32; FEATURE_COUNT] {
    let (h, v) = (p.label, p.value);
    let text = p.text.as_str();
    let [upper, lower, digit, special, space] = char_class_counts(text);
    let (lr, lc) = (store.get_row(h), store.get_col(h));
    let (vr, vc) = (store.get_row(v), store.get_col(v));
    let (dr, dc) = (vr as i32 - lr as i32, vc as i32 - lc as i32);

    let label = p.label_text.as_str();
    let assignment_end = match p.pathway {
        Pathway::Inline => true,
        Pathway::Prose => false,
        _ => label.trim_end().ends_with([':', '=']),
    };
    let own_label = u32::from(store.feature_flags[h] & LABEL_FLAGS != 0);
    let others_in_row = ctx.row_labels.get(&lr).copied().unwrap_or(0) - own_label;
    let in_table = ctx.tables.get(&h).is_some_and(|t| t.contains(&(v as u32)));

    let (mut username, mut url) = (false, false);
    for n in query_radius(store, vr, vc, NEIGHBOR_RADIUS) {
        let f = store.feature_flags[n as usize];
        username |= f & FLAG_IS_USERNAME_HEADER != 0;
        url |= f & FLAG_IS_URL_HEADER != 0;
    }
    let (column_len, column_distinct, column_shape) = *ctx
        .columns
        .entry(v)
        .or_insert_with(|| column_stats(store, v));
    let same_region = store.region_ids[h] >= 0 && store.region_ids[h] == store.region_ids[v];

    [
        text.chars().count() as f32,
        upper as f32,
        lower as f32,
        digit as f32,
        special as f32,
        space as f32,
        compute_entropy(text),
        flag(is_numeric_amount(text)),
        flag(is_date_like(text)),
        flag(is_url_or_email(text)),
        flag(TABLE_PLACEHOLDERS.contains(&normalize(text).as_str())),
        flag(is_password_label(text)),
        flag(!store.get_formula(v).is_empty()),
        f32::from(keyword_class(label)),
        flag(is_compound_credential_label(label)),
        flag(is_exact_keyword(label)),
        flag(is_ambiguous_label(label)),
        label.chars().count() as f32,
        flag(assignment_end),
        flag(others_in_row > 0),
        flag(has_non_secret_part(label)),
        dr as f32,
        dc as f32,
        empty_between(store, h, v) as f32,
        flag(same_region),
        DISTANCE_TABLE.score(dr, dc),
        flag(in_table),
        flag(username),
        flag(url),
        column_len,
        column_distinct,
        column_shape,
        flag(store.get_sheet_name(v).ends_with(HIDDEN_SHEET_SUFFIX)),
        ctx.sheet_labels as f32,
        f32::from(p.pathway as u8),
        ctx.baseline.get(&(vr, vc)).copied().unwrap_or(0.0),
    ]
}

/// Character range of `text` within the value cell's text, when it is a
/// proper part of it.
fn span(cell: &str, text: &str) -> Option<(u32, u32)> {
    if text.is_empty() || text == cell {
        return None;
    }
    let at = cell.find(text)?;
    let start = cell[..at].chars().count() as u32;
    Some((start, start + text.chars().count() as u32))
}

/// List every credential candidate in one sheet with its features, for the
/// learned-scorer experiment (ADR 0016).
///
/// The generator proposes more than the pipeline reports: every pathway's
/// candidates before its threshold or accept rule (spatial pairs at any score,
/// the column below every password label, variable values and split
/// passwords whatever came first), plus loosened rules the pipeline does not
/// have: prose and whitespace forms (`the password is x`), every inline
/// keyword including everyday words, the first value right of or below any
/// cell holding a credential word (up to `GAP_MAX_EMPTY` empty cells away), and
/// the column below a short header naming a secret (`login_totp`). Columns
/// allow up to `GAP_MAX_EMPTY` empty rows. Every `process_sheet` finding appears as a
/// candidate with the same label and value cells, and its confidence is the
/// `baseline_confidence` feature.
///
/// Candidates are unique per (label cell, value cell, pathway) and sorted by
/// value cell, label cell and pathway. They carry no value text.
pub fn export_sheet(cells: Vec<RawCell>) -> Vec<Candidate> {
    if cells.is_empty() {
        return Vec::new();
    }
    let mut baseline: AHashMap<(u32, u32), f32> = AHashMap::new();
    for f in process_sheet(cells.clone()) {
        let best = baseline
            .entry((f.value_row, f.value_col))
            .or_insert(f.confidence);
        *best = best.max(f.confidence);
    }

    let mut store = CellStore::from_raw(cells);
    precompute_features(&mut store);
    let candidate_ids = reduce_candidate_space(&store);
    classify_cells(&mut store, &candidate_ids);
    detect_regions(&mut store, &candidate_ids);

    let mut row_labels: AHashMap<u32, u32> = AHashMap::new();
    for i in (0..store.len()).filter(|&i| store.feature_flags[i] & LABEL_FLAGS != 0) {
        *row_labels.entry(store.get_row(i)).or_insert(0) += 1;
    }
    let password_labels: Vec<usize> = (0..store.len())
        .filter(|&i| store.feature_flags[i] & FLAG_IS_PASSWORD_HEADER != 0)
        .collect();
    let tables = password_labels
        .iter()
        .map(|&h| {
            (
                h,
                detect_table_column(&store, h as u32).into_iter().collect(),
            )
        })
        .collect();
    let mut ctx = SheetContext {
        baseline,
        row_labels,
        sheet_labels: password_labels.len() as u32,
        tables,
        columns: AHashMap::new(),
    };

    let mut seen = AHashSet::new();
    let mut out: Vec<Candidate> = propose(&store, &candidate_ids)
        .into_iter()
        .filter(|p| seen.insert((p.label, p.value, p.pathway)))
        .map(|p| Candidate {
            sheet: store.get_sheet_name(p.value).to_string(),
            label_row: store.get_row(p.label),
            label_col: store.get_col(p.label),
            value_row: store.get_row(p.value),
            value_col: store.get_col(p.value),
            pathway: p.pathway,
            span: span(store.get_value(p.value), &p.text),
            features: features(&store, &p, &mut ctx),
        })
        .collect();
    out.sort_by_key(|c| {
        (
            c.value_row,
            c.value_col,
            c.label_row,
            c.label_col,
            c.pathway,
        )
    });
    out
}

/// Run `export_sheet` on every sheet in parallel; results are concatenated in
/// sheet order.
pub fn export_workbook(sheets: Vec<Vec<RawCell>>) -> Vec<Candidate> {
    let per_sheet: Vec<Vec<Candidate>> = sheets.into_par_iter().map(export_sheet).collect();
    per_sheet.into_iter().flatten().collect()
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

    fn feature(c: &Candidate, name: &str) -> f32 {
        let i = FEATURE_NAMES
            .iter()
            .position(|&n| n == name)
            .expect("known feature");
        c.features[i]
    }

    fn sheets() -> Vec<Vec<RawCell>> {
        vec![
            vec![raw(1, 1, "Password"), raw(1, 2, "Xq7-test-Zz")],
            vec![raw(1, 1, "db password: Fake-Pw-1")],
            vec![
                raw(1, 1, "Username"),
                raw(1, 2, "Password"),
                raw(2, 1, "user.one"),
                raw(2, 2, "Fake1aa"),
                raw(3, 1, "user.two"),
                raw(3, 2, "Fake2bb"),
            ],
            vec![
                raw(1, 1, "MYSQL_DATABASE"),
                raw(1, 2, "app"),
                raw(2, 1, "MYSQL_ROOT_PASSWORD"),
                raw(2, 2, "fake-root-pw"),
            ],
            vec![
                raw(1, 1, "Password"),
                raw(2, 1, "Fa"),
                raw(3, 1, "ke"),
                raw(4, 1, "Pw9"),
            ],
        ]
    }

    #[test]
    fn every_pipeline_finding_is_a_candidate() {
        for cells in sheets() {
            let findings = process_sheet(cells.clone());
            assert!(!findings.is_empty());
            let candidates = export_sheet(cells);
            for f in findings {
                let found = candidates.iter().find(|c| {
                    (c.label_row, c.label_col, c.value_row, c.value_col)
                        == (f.header_row, f.header_col, f.value_row, f.value_col)
                });
                let c = found.expect("finding listed as a candidate");
                assert!(feature(c, "baseline_confidence") >= f.confidence);
            }
        }
    }

    #[test]
    fn prose_password_with_span() {
        let candidates = export_sheet(vec![raw(1, 1, r#"the password is "Zz-fake-9""#)]);
        let prose = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Prose)
            .expect("prose candidate");
        assert_eq!(prose.span, Some((17, 26)));
        assert_eq!(feature(prose, "baseline_confidence"), 0.0);
        assert_eq!(feature(prose, "label_assignment_end"), 0.0);
    }

    #[test]
    fn gap_label_two_cells_away() {
        let candidates = export_sheet(vec![raw(1, 1, "The password is:"), raw(1, 3, "Fake-gap-1")]);
        let gap = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Gap)
            .expect("gap candidate");
        assert_eq!((gap.value_row, gap.value_col), (1, 3));
        assert_eq!(feature(gap, "empty_between"), 1.0);
        assert_eq!(feature(gap, "col_offset"), 2.0);
    }

    #[test]
    fn column_without_account_label_is_listed() {
        let cells = vec![
            raw(1, 1, "Password"),
            raw(2, 1, "Fake-col-1"),
            raw(3, 1, "Fake-col-2"),
            raw(4, 1, "Fake-col-3"),
        ];
        let table: Vec<Candidate> = export_sheet(cells)
            .into_iter()
            .filter(|c| c.pathway == Pathway::Table)
            .collect();
        assert_eq!(table.len(), 3);
        assert!(table.iter().all(|c| feature(c, "in_table_column") == 0.0));
        assert!(table.iter().all(|c| feature(c, "column_len") == 4.0));
    }

    #[test]
    fn column_continues_over_empty_rows() {
        let cells = vec![
            raw(1, 1, "Password"),
            raw(4, 1, "Fake-col-1"),
            raw(5, 1, "Fake-col-2"),
            raw(10, 1, "Fake-col-3"),
        ];
        let rows: Vec<u32> = export_sheet(cells)
            .into_iter()
            .filter(|c| c.pathway == Pathway::Table)
            .map(|c| c.value_row)
            .collect();
        assert_eq!(rows, vec![4, 5]);
    }

    #[test]
    fn short_credential_word_heads_a_column() {
        let cells = vec![
            raw(1, 1, "login_username"),
            raw(1, 2, "login_totp"),
            raw(2, 1, "user.one"),
            raw(2, 2, "FAKETOTPSEEDONE"),
            raw(3, 1, "user.two"),
            raw(3, 2, "FAKETOTPSEEDTWO"),
        ];
        let table: Vec<(u32, u32)> = export_sheet(cells)
            .into_iter()
            .filter(|c| c.pathway == Pathway::Table && (c.label_row, c.label_col) == (1, 2))
            .map(|c| (c.value_row, c.value_col))
            .collect();
        assert_eq!(table, vec![(2, 2), (3, 2)]);
    }

    #[test]
    fn data_cell_with_a_credential_word_heads_no_column() {
        let cells = vec![
            raw(1, 1, "Role"),
            raw(2, 1, "Secretary"),
            raw(3, 1, "Fake-col-1"),
            raw(4, 1, "Fake-col-2"),
        ];
        assert!(export_sheet(cells)
            .iter()
            .all(|c| c.pathway != Pathway::Table));
    }

    #[test]
    fn prose_label_takes_the_next_cell() {
        let candidates = export_sheet(vec![
            raw(1, 1, "System user password:"),
            raw(1, 2, "Fake-next-1"),
        ]);
        let gap = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Gap)
            .expect("gap candidate");
        assert_eq!((gap.value_row, gap.value_col), (1, 2));
        assert_eq!(feature(gap, "empty_between"), 0.0);
    }

    #[test]
    fn gap_value_that_looks_like_a_label_is_listed() {
        let candidates = export_sheet(vec![raw(1, 1, "The password is:"), raw(1, 3, "Passwd")]);
        let gap = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Gap)
            .expect("gap candidate");
        assert_eq!(feature(gap, "value_is_keyword"), 1.0);
    }

    #[test]
    fn ambiguous_inline_word_is_listed() {
        let candidates = export_sheet(vec![raw(1, 1, "Pwd: 500mg")]);
        let inline = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Inline)
            .expect("inline candidate");
        assert_eq!(feature(inline, "label_ambiguous"), 1.0);
    }

    #[test]
    fn table_candidates_know_their_table() {
        let candidates = export_sheet(sheets().swap_remove(2));
        let table = candidates
            .iter()
            .find(|c| c.pathway == Pathway::Table && (c.value_row, c.value_col) == (2, 2))
            .expect("table candidate");
        assert_eq!(feature(table, "in_table_column"), 1.0);
        assert_eq!(feature(table, "label_in_header_row"), 1.0);
        assert_eq!(
            feature(table, "baseline_confidence"),
            TABLE_COLUMN_CONFIDENCE
        );
    }

    #[test]
    fn output_is_unique_and_sorted() {
        for cells in sheets() {
            let a = export_sheet(cells.clone());
            assert_eq!(a, export_sheet(cells));
            let keys: Vec<_> = a
                .iter()
                .map(|c| {
                    (
                        c.value_row,
                        c.value_col,
                        c.label_row,
                        c.label_col,
                        c.pathway,
                    )
                })
                .collect();
            let mut sorted = keys.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(keys, sorted);
        }
    }

    #[test]
    fn hidden_sheet_flag() {
        let mut cells = vec![raw(1, 1, "Password"), raw(1, 2, "Xq7-test-Zz")];
        for c in &mut cells {
            c.sheet_name = format!("Secret{HIDDEN_SHEET_SUFFIX}");
        }
        let candidates = export_sheet(cells);
        assert!(candidates.iter().all(|c| feature(c, "sheet_hidden") == 1.0));
    }

    #[test]
    fn empty_sheet_has_no_candidates() {
        assert!(export_sheet(Vec::new()).is_empty());
    }

    #[test]
    fn workbook_keeps_sheet_order() {
        let all = export_workbook(sheets());
        let per_sheet: Vec<Candidate> = sheets().into_iter().flat_map(export_sheet).collect();
        assert_eq!(all, per_sheet);
    }
}
