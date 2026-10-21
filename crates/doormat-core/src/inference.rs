use ahash::{AHashMap, AHashSet};

use crate::features::{
    is_boolean_literal, is_compound_credential_label, is_key_material, is_numeric_amount,
    is_variable_name,
};
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

/// True when a cell reads as a single credential-like token: no whitespace,
/// no trailing `:`, and neither a label nor a formula.
fn is_table_token(store: &CellStore, id: usize) -> bool {
    let value = store.get_value(id).trim();
    let flags = store.feature_flags[id];
    !value.is_empty()
        && !value.chars().any(char::is_whitespace)
        && !value.ends_with(':')
        && flags & (FLAG_IS_PASSWORD_HEADER | FLAG_IS_USERNAME_HEADER | FLAG_HAS_FORMULA) == 0
}

/// True when a cell names a credential table's account column: a username
/// label or one of `TABLE_USERNAME_LABELS`.
fn is_account_label(store: &CellStore, id: usize) -> bool {
    store.feature_flags[id] & FLAG_IS_USERNAME_HEADER != 0
        || TABLE_USERNAME_LABELS.contains(&store.normalized_values[id].as_str())
}

/// True when a cell is a credential or account label, which starts a new table.
pub(crate) fn is_table_label(store: &CellStore, id: usize) -> bool {
    store.feature_flags[id] & FLAG_IS_PASSWORD_HEADER != 0 || is_account_label(store, id)
}

/// The id of the non-empty cell at `(row, col)`, if any.
pub(crate) fn filled(store: &CellStore, row: u32, col: u32) -> Option<u32> {
    store
        .coord_to_id
        .get(&(row, col))
        .copied()
        .filter(|&id| !store.get_value(id as usize).trim().is_empty())
}

/// Find a credential table column under `header_id` (`User ID` | `Password`,
/// one account per row).
///
/// The password header must share its row with an account label: a username
/// label or one of `TABLE_USERNAME_LABELS` (such as `ID`), at most
/// `TABLE_HEADER_REACH` columns away; the nearest one names the account
/// column. The table's rows run down from the header while the account cell or
/// the secret cell is filled, ending at a new label row or after more than
/// `TABLE_MAX_GAP` consecutive rows with both empty. Rows without a secret are
/// allowed (an account with no password, or a TOTP column that is mostly
/// empty). The table qualifies with at least `TABLE_MIN_ROWS` rows and at least
/// one secret, and at least `TABLE_TOKEN_SHARE` of its secrets must be single
/// tokens; prose, dates and form labels fail that test. Returns the ids of the
/// token secrets with at least `TABLE_MIN_VALUE_LEN` characters and a letter or
/// digit that are not a placeholder (`TABLE_PLACEHOLDERS`), or an empty vector
/// if the column does not qualify.
pub(crate) fn detect_table_column(store: &CellStore, header_id: u32) -> Vec<u32> {
    let h = header_id as usize;
    let (row, col) = (store.get_row(h), store.get_col(h));

    let account_col = (1..=TABLE_HEADER_REACH)
        .flat_map(|d| [col.checked_sub(d), col.checked_add(d)])
        .flatten()
        .find(|&c| filled(store, row, c).is_some_and(|id| is_account_label(store, id as usize)));
    let Some(account_col) = account_col else {
        return Vec::new();
    };

    let mut gap = 0;
    let rows: Vec<(Option<u32>, Option<u32>)> = (row + 1..=row.saturating_add(TABLE_MAX_ROWS))
        .map(|r| (filled(store, r, col), filled(store, r, account_col)))
        .take_while(|&(secret, account)| {
            gap = if secret.is_some() || account.is_some() {
                0
            } else {
                gap + 1
            };
            let new_header = [secret, account]
                .into_iter()
                .flatten()
                .any(|id| is_table_label(store, id as usize));
            gap <= TABLE_MAX_GAP && !new_header
        })
        .filter(|&(secret, account)| secret.is_some() || account.is_some())
        .collect();
    let secrets: Vec<u32> = rows.iter().filter_map(|&(secret, _)| secret).collect();

    let tokens = secrets
        .iter()
        .filter(|&&id| is_table_token(store, id as usize))
        .count();
    if rows.len() < TABLE_MIN_ROWS
        || secrets.is_empty()
        || (tokens as f32) < TABLE_TOKEN_SHARE * secrets.len() as f32
    {
        return Vec::new();
    }

    secrets
        .into_iter()
        .filter(|&id| {
            let id = id as usize;
            let value = store.get_value(id).trim();
            is_table_token(store, id)
                && value.chars().count() >= TABLE_MIN_VALUE_LEN
                && value.chars().any(char::is_alphanumeric)
                && !TABLE_PLACEHOLDERS.contains(&store.normalized_values[id].as_str())
                && !is_boolean_literal(&store.normalized_values[id])
        })
        .collect()
}

/// True when a cell can be reported as a credential value next to a label:
/// a single token of at least `TABLE_MIN_VALUE_LEN` characters with a letter or
/// digit, and not a placeholder, a switch or an amount.
fn is_credential_value(store: &CellStore, id: usize) -> bool {
    let value = store.get_value(id).trim();
    let normalized = store.normalized_values[id].as_str();
    is_table_token(store, id)
        && value.chars().count() >= TABLE_MIN_VALUE_LEN
        && value.chars().any(char::is_alphanumeric)
        && !TABLE_PLACEHOLDERS.contains(&normalized)
        && !is_boolean_literal(normalized)
        && !is_numeric_amount(value)
}

/// True when the cell at `(row, col)` holds a variable name.
fn variable_name_at(store: &CellStore, row: u32, col: u32) -> bool {
    filled(store, row, col).is_some_and(|id| is_variable_name(store.get_value(id as usize)))
}

/// Values of a compound credential label (`MYSQL_ROOT_PASSWORD`) or key label
/// (`WORDPRESS_AUTH_KEY`) in a list of configuration variables, or `None` if
/// `header_id` is neither. A key label's values must also be key material.
///
/// Variable names run along one axis and their values sit across from them.
/// If more of the label's vertical neighbours than horizontal ones are
/// variable names, the names run down a column and the value is the cell to
/// the label's right. If more horizontal neighbours are names, they form a
/// header row (one row per environment below) and the values are the column
/// below, down to a label or more than `TABLE_MAX_GAP` empty rows. A label with
/// no neighbouring names takes the cell to its right when there is one.
/// Cells that are not credential values (`yes`, a placeholder, prose) are not
/// reported, and a label never takes another variable's value: an empty
/// result means the label has no value.
pub(crate) fn detect_variable_values(store: &CellStore, header_id: u32) -> Option<Vec<u32>> {
    let h = header_id as usize;
    let label = store.get_value(h);
    let key_label = store.feature_flags[h] & FLAG_IS_KEY_LABEL != 0;
    if !key_label && !is_compound_credential_label(label) {
        return None;
    }
    let (row, col) = (store.get_row(h), store.get_col(h));
    let names = |cells: [(Option<u32>, Option<u32>); 2]| {
        cells
            .into_iter()
            .filter(|&(r, c)| matches!((r, c), (Some(r), Some(c)) if variable_name_at(store, r, c)))
            .count()
    };
    let vertical = names([
        (row.checked_sub(1), Some(col)),
        (row.checked_add(1), Some(col)),
    ]);
    let horizontal = names([
        (Some(row), col.checked_sub(1)),
        (Some(row), col.checked_add(1)),
    ]);

    let values: Vec<u32> = if horizontal > vertical {
        let mut gap = 0;
        (row + 1..=row.saturating_add(TABLE_MAX_ROWS))
            .map(|r| filled(store, r, col))
            .take_while(|cell| {
                gap = if cell.is_some() { 0 } else { gap + 1 };
                gap <= TABLE_MAX_GAP && !cell.is_some_and(|id| is_table_label(store, id as usize))
            })
            .flatten()
            .collect()
    } else {
        match filled(store, row, col + 1) {
            Some(id) => vec![id],
            None if vertical > 0 => Vec::new(),
            None => return None,
        }
    };
    Some(
        values
            .into_iter()
            .filter(|&id| {
                let id = id as usize;
                is_credential_value(store, id)
                    && (!key_label || is_key_material(store.get_value(id)))
            })
            .collect(),
    )
}

/// Walk up to 5 cells directly below `header_id` in the same column.
/// Each cell must have `CellType::Value`, be non-empty, not be a password
/// header, not end in `:`, and have no horizontal neighbour. If >= 2 parts
/// found, each < 30 chars, and the combined string >= 6 chars, returns
/// `(combined_value, first_below_cell_id)`.
pub(crate) fn detect_split_password(store: &CellStore, header_id: u32) -> Option<(String, u32)> {
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
/// 0. Try `detect_table_column`; if it qualifies, emit one relationship per
///    row with `TABLE_COLUMN_CONFIDENCE` and move to the next header. A label
///    occurring `CATEGORY_REPEAT` or more times in its column is a category
///    value and skips the steps below.
/// 1. Try `detect_split_password`; if found, emit with confidence 200.0
///    A label ending in `:` with nothing in the `FORM_FIELD_REACH` cells to
///    its right is an empty form field and is not paired below.
/// 2. Otherwise, query all neighbors within `NEIGHBOR_RADIUS`
/// 3. Filter to `CellType::Value` only, skip already-used values and
///    formula cells (a formula's text is never the stored secret; string
///    literals inside formulas are handled by `analyze_formulas`)
/// 4. Score each via `score_candidate`, pick best above `RELATIONSHIP_THRESHOLD`
/// 5. Mark claimed values in `used_value_ids` to prevent double-assignment
pub fn infer_relationships(store: &CellStore, dist_table: &DistanceTable) -> Vec<Relationship> {
    let mut results: Vec<Relationship> = Vec::new();
    let mut used_value_ids: AHashSet<u32> = AHashSet::new();

    // A label repeated down one column is usually data (`Category: Password`
    // in a ticket log), not a label with a value beside it. Such a cell may
    // still head a credential table (stacked tables repeat their header row),
    // but it is never paired by the fallback steps.
    let mut per_column: AHashMap<(u32, &str), u32> = AHashMap::new();
    for i in (0..store.len()).filter(|&i| store.feature_flags[i] & FLAG_IS_PASSWORD_HEADER != 0) {
        *per_column
            .entry((store.get_col(i), store.normalized_values[i].as_str()))
            .or_insert(0) += 1;
    }
    let is_category = |i: usize| {
        per_column
            .get(&(store.get_col(i), store.normalized_values[i].as_str()))
            .copied()
            .unwrap_or(0)
            >= CATEGORY_REPEAT
    };
    // Key labels (`..._KEY`, `..._SALT`) are considered only in variable lists.
    let header_ids: Vec<u32> = (0..store.len())
        .filter(|&i| store.feature_flags[i] & (FLAG_IS_PASSWORD_HEADER | FLAG_IS_KEY_LABEL) != 0)
        .map(|i| i as u32)
        .collect();

    for &header_id in &header_ids {
        let h = header_id as usize;
        let password_label = store.feature_flags[h] & FLAG_IS_PASSWORD_HEADER != 0;

        // 0. A credential table: one relationship per row of the column
        let table = if password_label {
            detect_table_column(store, header_id)
        } else {
            Vec::new()
        };
        if !table.is_empty() {
            for value_id in table {
                used_value_ids.insert(value_id);
                results.push(Relationship {
                    header_cell_id: h,
                    value_cell_id: value_id as usize,
                    key: store.get_value(h).to_string(),
                    value: store.get_value(value_id as usize).to_string(),
                    confidence: TABLE_COLUMN_CONFIDENCE,
                    reason: "table_column".into(),
                });
            }
            continue;
        }
        if is_category(h) {
            continue;
        }

        // A compound label in a list of configuration variables: its value is
        // across from it, and it never takes another variable's value.
        if let Some(values) = detect_variable_values(store, header_id) {
            for value_id in values {
                if !used_value_ids.insert(value_id) {
                    continue;
                }
                results.push(Relationship {
                    header_cell_id: h,
                    value_cell_id: value_id as usize,
                    key: store.get_value(h).to_string(),
                    value: store.get_value(value_id as usize).to_string(),
                    confidence: VARIABLE_VALUE_CONFIDENCE,
                    reason: "variable_value".into(),
                });
            }
            continue;
        }

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
        if !password_label {
            continue;
        }

        // A form field (`Password:`) takes its value from the right; with nothing
        // there it is an empty field, and the label below is not its value.
        let (row, col) = (store.get_row(h), store.get_col(h));
        if store.get_value(h).trim_end().ends_with(':')
            && (1..=FORM_FIELD_REACH).all(|d| filled(store, row, col + d).is_none())
        {
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
            if !store.get_formula(n).is_empty() || is_boolean_literal(&store.normalized_values[n]) {
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

    fn values(rels: &[Relationship]) -> Vec<&str> {
        rels.iter().map(|r| r.value.as_str()).collect()
    }

    #[test]
    fn table_column_reports_every_row() {
        let store = pipeline_store(vec![
            raw(1, 1, "System"),
            raw(1, 2, "User ID"),
            raw(1, 3, "Password"),
            raw(2, 1, "Billing"),
            raw(2, 2, "svc_bill"),
            raw(2, 3, "Xy7!fake"),
            raw(3, 1, "Reporting"),
            raw(3, 2, "svc_report"),
            raw(3, 3, "Qw3#fake"),
            raw(4, 1, "Backups"),
            raw(4, 2, "svc_backup"),
            raw(4, 3, "Zz9$fake"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Xy7!fake", "Qw3#fake", "Zz9$fake"]);
        assert!(rels.iter().all(|r| r.reason == "table_column"
            && r.confidence == TABLE_COLUMN_CONFIDENCE
            && r.header_cell_id == 2));
    }

    #[test]
    fn table_column_needs_a_username_label_in_the_header_row() {
        // A help-desk log: "Password" is a category, and the column below it is
        // other rows' categories.
        let store = pipeline_store(vec![
            raw(1, 1, "Customer"),
            raw(1, 2, "Password"),
            raw(2, 1, "Acme"),
            raw(2, 2, "Unlock"),
            raw(3, 1, "Initech"),
            raw(3, 2, "Reset"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.iter().all(|r| r.reason != "table_column"));
    }

    #[test]
    fn table_column_rejects_prose_and_form_labels() {
        let store = pipeline_store(vec![
            raw(1, 1, "User ID"),
            raw(1, 2, "Password"),
            raw(2, 2, "Reset his password"),
            raw(3, 2, "Included Deals:"),
            raw(4, 2, "Called the customer back"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.iter().all(|r| r.reason != "table_column"));
    }

    #[test]
    fn table_column_ends_after_two_empty_rows_and_skips_placeholders() {
        let store = pipeline_store(vec![
            raw(1, 1, "Login"),
            raw(1, 2, "Password"),
            raw(2, 1, "alice"),
            raw(2, 2, "Tr0ub4dor"),
            raw(3, 1, "bob"),
            raw(3, 2, "****"),
            raw(4, 1, "carol"),
            raw(4, 2, "c0rrectH0rse"),
            raw(7, 2, "notInTable1"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor", "c0rrectH0rse"]);
    }

    #[test]
    fn table_column_allows_one_blank_row_between_accounts() {
        let store = pipeline_store(vec![
            raw(1, 1, "User ID"),
            raw(1, 2, "Password"),
            raw(2, 1, "alice"),
            raw(2, 2, "Tr0ub4dor"),
            raw(4, 1, "bob"),
            raw(4, 2, "c0rrectH0rse"),
            raw(7, 2, "afterTwoBlankRows"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor", "c0rrectH0rse"]);
    }

    #[test]
    fn table_column_accepts_a_bare_id_label() {
        let store = pipeline_store(vec![
            raw(1, 1, "ID"),
            raw(1, 2, "Password"),
            raw(2, 1, "abc1234"),
            raw(2, 2, "Tr0ub4dor"),
            raw(3, 1, "abc5678"),
            raw(3, 2, "c0rrectH0rse"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor", "c0rrectH0rse"]);
    }

    #[test]
    fn table_column_finds_a_distant_username_label() {
        // Dashlane's export: username in column 1, password in column 5.
        let mut cells = ["username", "username2", "username3", "title", "password"]
            .iter()
            .enumerate()
            .map(|(c, h)| raw(1, c as u32 + 1, h))
            .collect::<Vec<_>>();
        for (r, (user, pw)) in [("ada", "Tr0ub4dor"), ("bob", "c0rrectH0rse")]
            .iter()
            .enumerate()
        {
            cells.push(raw(r as u32 + 2, 1, user));
            cells.push(raw(r as u32 + 2, 4, "Portal login"));
            cells.push(raw(r as u32 + 2, 5, pw));
        }
        let rels = infer_relationships(&pipeline_store(cells), &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor", "c0rrectH0rse"]);
    }

    #[test]
    fn repeated_label_in_one_column_is_a_category_value() {
        let mut cells = vec![raw(1, 1, "Category"), raw(1, 2, "Action")];
        for (r, action) in ["Unlock", "Reset", "Unlock"].iter().enumerate() {
            cells.push(raw(r as u32 + 2, 1, "Password"));
            cells.push(raw(r as u32 + 2, 2, action));
        }
        assert!(infer_relationships(&pipeline_store(cells), &DISTANCE_TABLE).is_empty());
    }

    #[test]
    fn two_stacked_login_blocks_are_still_labels() {
        let store = pipeline_store(vec![
            raw(1, 1, "Password"),
            raw(1, 2, "Tr0ub4dor!9"),
            raw(5, 1, "Password"),
            raw(5, 2, "c0rrectH0rse#2"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor!9", "c0rrectH0rse#2"]);
    }

    #[test]
    fn stacked_credential_tables_repeat_their_header() {
        let mut cells = Vec::new();
        for block in 0..3u32 {
            let top = block * 5 + 1;
            cells.push(raw(top, 1, "User ID"));
            cells.push(raw(top, 2, "Password"));
            cells.push(raw(top + 1, 1, "svc_a"));
            cells.push(raw(top + 1, 2, &format!("Tr0ub4dor{block}")));
            cells.push(raw(top + 2, 1, "svc_b"));
            cells.push(raw(top + 2, 2, &format!("c0rrectH0rse{block}")));
        }
        let rels = infer_relationships(&pipeline_store(cells), &DISTANCE_TABLE);
        assert_eq!(rels.len(), 6);
        assert!(rels.iter().all(|r| r.reason == "table_column"));
    }

    #[test]
    fn table_rows_follow_the_account_column_past_empty_secrets() {
        let mut cells = vec![raw(1, 1, "Username"), raw(1, 2, "Password")];
        let secrets = ["Tr0ub4dor", "", "", "", "c0rrectH0rse", "Xy7!fake9"];
        for (i, secret) in secrets.iter().enumerate() {
            let r = i as u32 + 2;
            cells.push(raw(r, 1, &format!("user{i}")));
            if !secret.is_empty() {
                cells.push(raw(r, 2, secret));
            }
        }
        let rels = infer_relationships(&pipeline_store(cells), &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor", "c0rrectH0rse", "Xy7!fake9"]);
    }

    #[test]
    fn sparse_secret_column_uses_the_account_column_not_another_secret() {
        // username | password | totp: the password column has gaps, the TOTP
        // column has a single secret; neither may cut the table short.
        let mut cells = vec![
            raw(1, 1, "username"),
            raw(1, 2, "password"),
            raw(1, 3, "totp"),
        ];
        for r in 2..=6u32 {
            cells.push(raw(r, 1, &format!("user{r}")));
        }
        cells.push(raw(2, 2, "Tr0ub4dor"));
        cells.push(raw(6, 2, "c0rrectH0rse"));
        cells.push(raw(6, 3, "JBSWY3DPEHPK3PXP"));
        let rels = infer_relationships(&pipeline_store(cells), &DISTANCE_TABLE);
        let mut found = values(&rels);
        found.sort_unstable();
        assert_eq!(found, ["JBSWY3DPEHPK3PXP", "Tr0ub4dor", "c0rrectH0rse"]);
    }

    #[test]
    fn table_column_skips_rubric_placeholders() {
        let store = pipeline_store(vec![
            raw(1, 1, "Login"),
            raw(1, 2, "Password"),
            raw(2, 1, "alice"),
            raw(2, 2, "N/A"),
            raw(3, 1, "bob"),
            raw(3, 2, "TBD"),
            raw(4, 1, "carol"),
            raw(4, 2, "c0rrectH0rse"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["c0rrectH0rse"]);
    }

    #[test]
    fn empty_form_field_does_not_pair_with_the_label_below() {
        let store = pipeline_store(vec![
            raw(1, 1, "User ID:"),
            raw(2, 1, "Password:"),
            raw(4, 1, "Date"),
            raw(4, 2, "Price"),
            raw(5, 1, "2025-01-01"),
            raw(5, 2, "4.25"),
        ]);
        assert!(infer_relationships(&store, &DISTANCE_TABLE).is_empty());
    }

    #[test]
    fn form_field_value_may_sit_two_columns_right() {
        let store = pipeline_store(vec![raw(1, 1, "Password:"), raw(1, 3, "Tr0ub4dor!9")]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor!9"]);
    }

    #[test]
    fn compound_label_pairs_with_its_value_but_not_a_switch() {
        let store = pipeline_store(vec![
            raw(1, 1, "Variable"),
            raw(1, 2, "Value"),
            raw(2, 1, "MYSQL_ROOT_PASSWORD"),
            raw(2, 2, "Tr0ub4dor!9"),
            raw(3, 1, "MYSQL_RANDOM_ROOT_PASSWORD"),
            raw(3, 2, "yes"),
            raw(4, 1, "MYSQL_ROOT_PASSWORD_FILE"),
            raw(4, 2, "/run/secrets/mysql_root_password"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["Tr0ub4dor!9"]);
    }

    #[test]
    fn key_label_takes_key_material_only() {
        let store = pipeline_store(vec![
            raw(1, 1, "WORDPRESS_AUTH_KEY"),
            raw(1, 2, "q8Zr2Lm9Xa7Pk3Vw#d"),
            raw(2, 1, "PRIMARY_KEY"),
            raw(2, 2, "user_id"),
        ]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert_eq!(values(&rels), ["q8Zr2Lm9Xa7Pk3Vw#d"]);
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
    fn formula_cell_never_paired_as_value() {
        let mut formula_cell = raw(0, 1, "=SUM(N5:N65536)");
        formula_cell.formula = "=SUM(N5:N65536)".into();
        let store = pipeline_store(vec![raw(0, 0, "Password:"), formula_cell]);
        let rels = infer_relationships(&store, &DISTANCE_TABLE);
        assert!(rels.is_empty());
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
