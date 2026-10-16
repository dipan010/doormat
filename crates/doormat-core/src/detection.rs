use crate::candidates::{
    inline_value, FORMULA_KEYWORD_REGEX, FORMULA_STRING_REGEX, INLINE_CREDENTIAL_REGEX,
};
use crate::features::{is_boolean_literal, normalize, secret_assignment};
use crate::store::CellStore;
use crate::types::*;

// ---------- Helpers ----------

/// Strip leading/trailing `:`, `=`, `-`, `>`, whitespace from a key string.
fn strip_key(s: &str) -> String {
    s.trim_matches(|c: char| c == ':' || c == '=' || c == '-' || c == '>' || c.is_whitespace())
        .to_string()
}

/// Strip surrounding quotes from a value string.
fn strip_quotes(s: &str) -> String {
    let trimmed = s.trim();
    if (trimmed.starts_with('"') && trimmed.ends_with('"'))
        || (trimmed.starts_with('\'') && trimmed.ends_with('\''))
    {
        trimmed[1..trimmed.len() - 1].to_string()
    } else {
        trimmed.to_string()
    }
}

// ---------- Inline credential detection ----------

/// Detect credentials embedded directly in cell values using the inline
/// credential regex. Returns relationships with confidence 250.0.
///
/// Formula cells are skipped: their value is the formula text, and
/// `analyze_formulas` already extracts credentials from formula strings.
pub fn detect_inline_credentials(store: &CellStore, candidate_ids: &[u32]) -> Vec<Relationship> {
    candidate_ids
        .iter()
        .filter_map(|&cell_id| {
            let id = cell_id as usize;
            if !store.get_formula(id).is_empty() {
                return None;
            }
            let value = store.get_value(id);

            // `key: value` around a credential keyword, or a variable assignment
            // whose name is a compound credential or key label
            // (`ME_CONFIG_MONGODB_ADMINPASSWORD=x`, `WORDPRESS_AUTH_KEY=x`).
            let (key, raw_value) = match INLINE_CREDENTIAL_REGEX
                .captures(value)
                .and_then(|caps| inline_value(&caps))
            {
                // Key: everything before the captured group in the original value
                Some(captured) => (
                    strip_key(&value[..captured.start()]),
                    strip_quotes(captured.as_str()),
                ),
                None => {
                    let (name, secret) = secret_assignment(value)?;
                    (name.to_string(), secret.to_string())
                }
            };
            if raw_value.len() < MIN_CANDIDATE_LENGTH || is_boolean_literal(&normalize(&raw_value))
            {
                return None;
            }

            Some(Relationship {
                header_cell_id: id,
                value_cell_id: id,
                key,
                value: raw_value,
                confidence: 250.0,
                reason: "inline_same_cell".into(),
            })
        })
        .collect()
}

// ---------- Formula analysis ----------

/// Analyze formula strings for embedded credentials.
///
/// Strategy 1: concatenate all quoted strings and try the inline regex.
/// Strategy 2: if one quoted string is a keyword, the next string(s) are values.
pub fn analyze_formulas(store: &CellStore, candidate_ids: &[u32]) -> Vec<Relationship> {
    let mut results = Vec::new();

    for &cell_id in candidate_ids {
        let id = cell_id as usize;
        let flags = store.feature_flags[id];

        // Early exit: skip cells without formula flag
        if flags & FLAG_HAS_FORMULA == 0 {
            continue;
        }

        let formula = store.get_formula(id);
        let strings: Vec<String> = FORMULA_STRING_REGEX
            .captures_iter(formula)
            .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
            .collect();

        if strings.is_empty() {
            continue;
        }

        // Strategy 1: concatenate all strings, try inline match
        let concatenated = strings.join(" ");
        if let Some(caps) = INLINE_CREDENTIAL_REGEX.captures(&concatenated) {
            if let Some(captured) = inline_value(&caps) {
                let raw_value = strip_quotes(captured.as_str());
                if raw_value.len() >= MIN_CANDIDATE_LENGTH {
                    let key = strip_key(&concatenated[..captured.start()]);
                    results.push(Relationship {
                        header_cell_id: id,
                        value_cell_id: id,
                        key,
                        value: raw_value,
                        confidence: 240.0,
                        reason: "formula_concatenated".into(),
                    });
                    continue;
                }
            }
        }

        // Strategy 2: keyword string followed by value string(s)
        for (i, s) in strings.iter().enumerate() {
            if FORMULA_KEYWORD_REGEX.is_match(s) && i + 1 < strings.len() {
                let key = strip_key(s);
                let raw_value = strip_quotes(&strings[i + 1]);
                if raw_value.len() >= MIN_CANDIDATE_LENGTH {
                    results.push(Relationship {
                        header_cell_id: id,
                        value_cell_id: id,
                        key,
                        value: raw_value,
                        confidence: 230.0,
                        reason: "formula_split_strings".into(),
                    });
                }
            }
        }
    }

    results
}

// ---------- Comment analysis ----------

/// Analyze cell comments for credentials.
///
/// Mode 1: cell is a password header and the comment contains the credential.
/// Mode 2: comment text itself contains a credential pattern.
pub fn analyze_comments(store: &CellStore, candidate_ids: &[u32]) -> Vec<Relationship> {
    let mut results = Vec::new();

    for &cell_id in candidate_ids {
        let id = cell_id as usize;
        let flags = store.feature_flags[id];

        // Early exit: skip cells without comment flag
        if flags & FLAG_HAS_COMMENT == 0 {
            continue;
        }

        let comment = store.get_comment(id);

        // Mode 1: cell is a password header → comment is the credential value
        if flags & FLAG_IS_PASSWORD_HEADER != 0 {
            let raw_value = strip_quotes(comment.trim());
            if raw_value.len() >= MIN_CANDIDATE_LENGTH {
                results.push(Relationship {
                    header_cell_id: id,
                    value_cell_id: id,
                    key: store.get_value(id).to_string(),
                    value: raw_value,
                    confidence: 230.0,
                    reason: "comment_on_header_cell".into(),
                });
            }
            continue;
        }

        // Mode 2: comment text contains a credential pattern
        if let Some(caps) = INLINE_CREDENTIAL_REGEX.captures(comment) {
            if let Some(captured) = inline_value(&caps) {
                let raw_value = strip_quotes(captured.as_str());
                if raw_value.len() >= MIN_CANDIDATE_LENGTH {
                    let key = strip_key(&comment[..captured.start()]);
                    results.push(Relationship {
                        header_cell_id: id,
                        value_cell_id: id,
                        key,
                        value: raw_value,
                        confidence: 220.0,
                        reason: "comment_contains_credential".into(),
                    });
                }
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidates::reduce_candidate_space;
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

    // ---------- detect_inline_credentials ----------

    #[test]
    fn inline_password_s3cret() {
        let store = build_store(vec![raw(0, 0, "password: s3cret!", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = detect_inline_credentials(&store, &candidates);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "s3cret!");
        assert_eq!(rels[0].confidence, 250.0);
        assert_eq!(rels[0].reason, "inline_same_cell");
    }

    #[test]
    fn inline_skips_formula_cells() {
        let formula = r#"=CONCAT("password: ","S3cure#1")"#;
        let store = build_store(vec![raw(0, 0, formula, formula, "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = detect_inline_credentials(&store, &candidates);
        assert!(rels.is_empty());
    }

    #[test]
    fn inline_key_extracted() {
        let store = build_store(vec![raw(0, 0, "password: s3cret!", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = detect_inline_credentials(&store, &candidates);
        assert_eq!(rels.len(), 1);
        // Key should be "password" (stripped of : and whitespace)
        assert_eq!(rels[0].key, "password");
    }

    #[test]
    fn inline_value_in_a_url_query_ends_at_the_next_parameter() {
        let store = build_store(vec![raw(
            1,
            1,
            "otpauth://totp/ada?secret=JBSWY3DPEHPK3PXP&issuer=Sample",
            "",
            "",
        )]);
        let rels = detect_inline_credentials(&store, &[0]);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn inline_switch_is_not_a_credential() {
        let store = build_store(vec![raw(1, 1, "MYSQL_ALLOW_EMPTY_PASSWORD=true", "", "")]);
        assert!(detect_inline_credentials(&store, &[0]).is_empty());
    }

    #[test]
    fn inline_assignment_with_a_glued_credential_word() {
        let store = build_store(vec![raw(
            1,
            1,
            "ME_CONFIG_MONGODB_ADMINPASSWORD=Tr0ub4dor!9",
            "",
            "",
        )]);
        let rels = detect_inline_credentials(&store, &[0]);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].key, "ME_CONFIG_MONGODB_ADMINPASSWORD");
        assert_eq!(rels[0].value, "Tr0ub4dor!9");
    }

    #[test]
    fn inline_value_too_short() {
        let store = build_store(vec![raw(0, 0, "password = abc", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = detect_inline_credentials(&store, &candidates);
        // "abc" is 3 chars < MIN_CANDIDATE_LENGTH (4)
        assert!(rels.is_empty());
    }

    #[test]
    fn inline_equals_format() {
        let store = build_store(vec![raw(0, 0, "Password=hunter2", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = detect_inline_credentials(&store, &candidates);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "hunter2");
    }

    // ---------- analyze_formulas ----------

    #[test]
    fn formula_concatenated_match() {
        let store = build_store(vec![raw(
            0,
            0,
            "result",
            r#"=CONCAT("password: ","MyP@ss1")"#,
            "",
        )]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_formulas(&store, &candidates);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "MyP@ss1");
        assert_eq!(rels[0].confidence, 240.0);
        assert_eq!(rels[0].reason, "formula_concatenated");
    }

    #[test]
    fn formula_split_strings_match() {
        // Use a keyword that's in FORMULA_KEYWORD but NOT in INLINE_CREDENTIAL_REGEX
        // so Strategy 1 doesn't fire and Strategy 2 catches it.
        let store = build_store(vec![raw(
            0,
            0,
            "result",
            r#"=IF(A1,"token","s3cr3t!val")"#,
            "",
        )]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_formulas(&store, &candidates);
        // "token" is a FORMULA_KEYWORD but not in INLINE_CREDENTIAL_REGEX
        assert!(rels.iter().any(|r| r.confidence == 230.0
            && r.reason == "formula_split_strings"
            && r.value == "s3cr3t!val"));
    }

    #[test]
    fn formula_no_strings_skipped() {
        let store = build_store(vec![raw(0, 0, "result", "=SUM(A1:B5)", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_formulas(&store, &candidates);
        assert!(rels.is_empty());
    }

    #[test]
    fn no_formula_flag_skipped() {
        // Cell with no formula → skipped via early flag check
        let store = build_store(vec![raw(0, 0, "Password", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_formulas(&store, &candidates);
        assert!(rels.is_empty());
    }

    // ---------- analyze_comments ----------

    #[test]
    fn comment_on_password_header() {
        let store = build_store(vec![raw(0, 0, "Password", "", "s3cr3t!")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_comments(&store, &candidates);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "s3cr3t!");
        assert_eq!(rels[0].confidence, 230.0);
        assert_eq!(rels[0].reason, "comment_on_header_cell");
        assert_eq!(rels[0].key, "Password");
    }

    #[test]
    fn comment_contains_credential_pattern() {
        let store = build_store(vec![raw(0, 0, "data", "", "password: abc123")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_comments(&store, &candidates);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].value, "abc123");
        assert_eq!(rels[0].confidence, 220.0);
        assert_eq!(rels[0].reason, "comment_contains_credential");
    }

    #[test]
    fn comment_too_short_value_skipped() {
        let store = build_store(vec![raw(0, 0, "Password", "", "abc")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_comments(&store, &candidates);
        // "abc" is 3 chars < MIN_CANDIDATE_LENGTH (4)
        assert!(rels.is_empty());
    }

    #[test]
    fn no_comment_flag_skipped() {
        let store = build_store(vec![raw(0, 0, "Password", "", "")]);
        let candidates = reduce_candidate_space(&store);
        let rels = analyze_comments(&store, &candidates);
        assert!(rels.is_empty());
    }

    // ---------- helpers ----------

    #[test]
    fn strip_key_removes_separators() {
        assert_eq!(strip_key("password:"), "password");
        assert_eq!(strip_key("  key = "), "key");
        assert_eq!(strip_key("-->label"), "label");
    }

    #[test]
    fn strip_quotes_removes_double() {
        assert_eq!(strip_quotes(r#""hello""#), "hello");
    }

    #[test]
    fn strip_quotes_removes_single() {
        assert_eq!(strip_quotes("'hello'"), "hello");
    }

    #[test]
    fn strip_quotes_no_quotes() {
        assert_eq!(strip_quotes("hello"), "hello");
    }
}
