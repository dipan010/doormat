use std::sync::LazyLock;

use ahash::AHashMap;
use regex::Regex;

use crate::store::CellStore;
use crate::types::*;

// ---------- WIN 3: compile regex patterns ONCE ----------

/// Inline `key: value` credential pattern.
///
/// The keyword must be a whole word, so `spin`, `compass` and `by-pass` do not
/// match. The separator must be `:`, `=` or a dash with whitespace on at least
/// one side (`Password - X`), so prose like `pass through` does not match.
/// Password-family keywords also accept a bare whitespace separator when the
/// value is a single token containing a digit (`Password X333#`).
///
/// The value is in capture group 1 or 2; read it with [`inline_value`].
pub(crate) static INLINE_CREDENTIAL_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"(?i)\b(?:password|pwd|pass|passwd|passwort|contraseña|motdepasse|senha|пароль|secret|pin)\b",
        r"(?:\s*[:=]\s*|\s+[-–]\s*|\s*[-–]\s+)(.+)",
        r"|\b(?:password|passwd|pwd|passwort|contraseña|motdepasse|senha|пароль)\s+(\S*\d\S*)\s*$",
    ))
    .expect("inline credential regex is valid")
});

/// Return the value matched by [`INLINE_CREDENTIAL_REGEX`], whichever branch matched.
pub(crate) fn inline_value<'h>(caps: &regex::Captures<'h>) -> Option<regex::Match<'h>> {
    caps.get(1).or_else(|| caps.get(2))
}

pub(crate) static FORMULA_STRING_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""([^"]+)""#).unwrap());

pub(crate) static FORMULA_KEYWORD_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:password|pwd|pass|passwd|passwort|contraseña|motdepasse|senha|пароль|secret|token|key|pin)[\s:=]*$").unwrap()
});

// ---------- Section-title special chars ----------

const SECTION_SPECIAL_CHARS: &[char] =
    &['=', '@', '#', '$', '%', '^', '&', '*', '{', '}', '[', ']'];

// ---------- Candidate space reduction ----------

/// Returns cell_ids of cells that are suspicious and warrant further analysis.
pub fn reduce_candidate_space(store: &CellStore) -> Vec<u32> {
    let mut candidates = Vec::new();

    for i in 0..store.len() {
        let flags = store.feature_flags[i];

        // Password/username/url header
        if flags & FLAG_IS_PASSWORD_HEADER != 0 {
            candidates.push(i as u32);
            continue;
        }

        // Has formula
        if flags & FLAG_HAS_FORMULA != 0 {
            candidates.push(i as u32);
            continue;
        }

        // Has comment
        if flags & FLAG_HAS_COMMENT != 0 {
            candidates.push(i as u32);
            continue;
        }

        // High entropy (already pre-filtered by GAP 3 in features phase)
        if store.entropy[i] >= ENTROPY_THRESHOLD {
            candidates.push(i as u32);
            continue;
        }

        // Inline credential regex match
        if INLINE_CREDENTIAL_REGEX.is_match(store.get_value(i)) {
            candidates.push(i as u32);
            continue;
        }
    }

    candidates
}

// ---------- Cell classification ----------

fn is_section_title(store: &CellStore, cell_id: u32, row_pop: &AHashMap<u32, u32>) -> bool {
    let id = cell_id as usize;
    let value = store.get_value(id);

    // Row population <= 2
    let row = store.get_row(id);
    let pop = row_pop.get(&row).copied().unwrap_or(0);
    if pop > 2 {
        return false;
    }

    // Length <= 60
    if value.len() > 60 {
        return false;
    }

    // No special chars like =@#$%^&*{}[]
    if value.chars().any(|c| SECTION_SPECIAL_CHARS.contains(&c)) {
        return false;
    }

    // Alpha ratio > 50%
    if value.is_empty() {
        return false;
    }
    let alpha_count = value.chars().filter(|c| c.is_alphabetic()).count();
    let total = value.chars().count();
    let alpha_ratio = alpha_count as f32 / total as f32;

    alpha_ratio > 0.5
}

/// Classify cells as Header, Section, or Value. Updates cell_types in place.
pub fn classify_cells(store: &mut CellStore, candidate_ids: &[u32]) {
    // Build row population from ALL cells
    let mut row_pop: AHashMap<u32, u32> = AHashMap::new();
    for i in 0..store.len() {
        *row_pop.entry(store.get_row(i)).or_insert(0) += 1;
    }

    for &cell_id in candidate_ids {
        let id = cell_id as usize;
        let flags = store.feature_flags[id];

        if flags & (FLAG_IS_PASSWORD_HEADER | FLAG_IS_USERNAME_HEADER | FLAG_IS_URL_HEADER) != 0 {
            store.cell_types[id] = CellType::Header as u8;
        } else if is_section_title(store, cell_id, &row_pop) {
            store.cell_types[id] = CellType::Section as u8;
        } else {
            store.cell_types[id] = CellType::Value as u8;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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

    // ---------- reduce_candidate_space ----------

    #[test]
    fn candidate_password_header() {
        let store = build_store(vec![raw(0, 0, "Password", "", "")]);
        let candidates = reduce_candidate_space(&store);
        assert!(candidates.contains(&0));
    }

    #[test]
    fn candidate_plain_text_excluded() {
        let store = build_store(vec![raw(0, 0, "Hello", "", "")]);
        let candidates = reduce_candidate_space(&store);
        assert!(!candidates.contains(&0));
    }

    #[test]
    fn candidate_inline_regex() {
        let store = build_store(vec![raw(0, 0, "password: s3cret!", "", "")]);
        let candidates = reduce_candidate_space(&store);
        assert!(candidates.contains(&0));
    }

    #[test]
    fn candidate_formula() {
        let store = build_store(vec![raw(0, 0, "result", "=SUM(A1)", "")]);
        let candidates = reduce_candidate_space(&store);
        assert!(candidates.contains(&0));
    }

    #[test]
    fn candidate_comment() {
        let store = build_store(vec![raw(0, 0, "data", "", "some note")]);
        let candidates = reduce_candidate_space(&store);
        assert!(candidates.contains(&0));
    }

    #[test]
    fn candidate_high_entropy() {
        // "xK9$mQ2!wP4z" has high entropy and mixed chars
        let store = build_store(vec![raw(0, 0, "xK9$mQ2!wP4z", "", "")]);
        // Verify entropy was computed and is above threshold
        assert!(store.entropy[0] >= ENTROPY_THRESHOLD);
        let candidates = reduce_candidate_space(&store);
        assert!(candidates.contains(&0));
    }

    // ---------- classify_cells ----------

    #[test]
    fn classify_password_header() {
        let mut store = build_store(vec![raw(0, 0, "Password", "", "")]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        assert_eq!(store.get_cell_type(0), CellType::Header);
    }

    #[test]
    fn classify_section_title() {
        // Single cell in its row, short text, mostly alpha
        let mut store = build_store(vec![raw(0, 0, "Account Settings", "", "")]);
        // Not a header keyword, but alone in row with alpha text
        let candidates = vec![0u32];
        classify_cells(&mut store, &candidates);
        assert_eq!(store.get_cell_type(0), CellType::Section);
    }

    #[test]
    fn classify_value_cell() {
        // Multiple cells in the row → not a section title; no header flag → Value
        let mut store = build_store(vec![
            raw(0, 0, "some data", "", "note"),
            raw(0, 1, "more data", "", ""),
            raw(0, 2, "extra", "", ""),
        ]);
        let candidates = reduce_candidate_space(&store);
        classify_cells(&mut store, &candidates);
        // Cell 0 was a candidate (has comment) but row has 3 cells → not section
        // No header flag → Value
        assert_eq!(store.get_cell_type(0), CellType::Value);
    }

    #[test]
    fn classify_section_not_if_special_chars() {
        let mut store = build_store(vec![raw(0, 0, "Config @#$", "", "")]);
        let candidates = vec![0u32];
        classify_cells(&mut store, &candidates);
        // Has special chars → not a section title → Value
        assert_eq!(store.get_cell_type(0), CellType::Value);
    }

    #[test]
    fn classify_section_not_if_too_long() {
        let long = "A".repeat(61);
        let mut store = build_store(vec![raw(0, 0, &long, "", "")]);
        let candidates = vec![0u32];
        classify_cells(&mut store, &candidates);
        assert_eq!(store.get_cell_type(0), CellType::Value);
    }

    // ---------- regex patterns ----------

    #[test]
    fn inline_regex_captures() {
        assert!(INLINE_CREDENTIAL_REGEX.is_match("password: s3cret!"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("Password=hunter2"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("pwd: test123"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("contraseña: abc"));
    }

    #[test]
    fn inline_regex_secret_and_pin() {
        assert!(INLINE_CREDENTIAL_REGEX.is_match("secret: api_key_123"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("Secret=my_token"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("pin: 8472"));
        assert!(INLINE_CREDENTIAL_REGEX.is_match("PIN=9999"));
    }

    #[test]
    fn inline_regex_no_match() {
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Hello World"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("12345"));
    }

    #[test]
    fn inline_regex_requires_whole_word_keyword() {
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Spin Reserves 7%"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Compass Bank: Inc"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Humpass Bonds Intl"));
    }

    #[test]
    fn inline_regex_rejects_prose_separator() {
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("100% pass through of premium costs"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Grants Pass Lateral"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Fuel Expense Pass-Through Plug"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Please enter the Password for the User ID"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Main Pass 99/100"));
        assert!(!INLINE_CREDENTIAL_REGEX.is_match("Pipeline PIN 1234"));
    }

    #[test]
    fn inline_regex_accepts_spaced_dash_and_digit_token() {
        let value = |s: &str| {
            let caps = INLINE_CREDENTIAL_REGEX.captures(s).unwrap();
            inline_value(&caps).unwrap().as_str().to_string()
        };
        assert_eq!(value("Password - K7QWERTY"), "K7QWERTY");
        assert_eq!(value("Password -4821"), "4821");
        assert_eq!(value("UID=svc;PWD=hunter22"), "hunter22");
        assert_eq!(value("Password Q9w8e7#"), "Q9w8e7#");
    }

    #[test]
    fn formula_string_regex_captures() {
        let caps = FORMULA_STRING_REGEX.captures(r#"=IF(A1="secret","yes","no")"#);
        assert!(caps.is_some());
        assert_eq!(caps.unwrap().get(1).unwrap().as_str(), "secret");
    }

    #[test]
    fn formula_keyword_regex_matches() {
        assert!(FORMULA_KEYWORD_REGEX.is_match("database password"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("API_KEY"));
        assert!(!FORMULA_KEYWORD_REGEX.is_match("hello world"));
    }

    #[test]
    fn formula_keyword_regex_multilingual() {
        assert!(FORMULA_KEYWORD_REGEX.is_match("admin passwort"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("user contraseña"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("motdepasse"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("db senha"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("пароль"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("user pin"));
    }

    #[test]
    fn formula_keyword_regex_colon_equals_terminator() {
        assert!(FORMULA_KEYWORD_REGEX.is_match("password:"));
        assert!(FORMULA_KEYWORD_REGEX.is_match("secret="));
        assert!(FORMULA_KEYWORD_REGEX.is_match("pin: "));
    }
}
