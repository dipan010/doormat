// ---------- Cell classification ----------

/// Classification of a cell's role in the spatial document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum CellType {
    /// Cell has no meaningful content.
    #[default]
    Empty = 0,
    /// Cell contains a keyword header (password, username, URL).
    Header = 1,
    /// Cell contains a potential credential value.
    Value = 2,
    /// Cell is a section title (low row population, mostly alphabetic).
    Section = 3,
}

impl CellType {
    /// Convert a `u8` back to `CellType`. Returns `CellType::Empty` for unknown values.
    #[inline]
    pub fn from_u8(v: u8) -> Self {
        match v {
            0 => CellType::Empty,
            1 => CellType::Header,
            2 => CellType::Value,
            3 => CellType::Section,
            _ => CellType::Empty,
        }
    }
}

// ---------- Relationship ----------

/// A detected key-value relationship between two cells.
#[derive(Debug, Clone, PartialEq)]
pub struct Relationship {
    /// Index of the header/label cell in the CellStore.
    pub header_cell_id: usize,
    /// Index of the value cell in the CellStore.
    pub value_cell_id: usize,
    /// The header text (e.g., "Password").
    pub key: String,
    /// The detected credential value.
    pub value: String,
    /// Heuristic confidence score.
    pub confidence: f32,
    /// Semicolon-separated breakdown of scoring factors.
    pub reason: String,
}

// ---------- Finding ----------

/// A detected credential with its location, as returned by the pipeline.
///
/// Rows and columns are the coordinates the caller supplied in `RawCell`
/// (1-based in the Python package). The value cell is where the secret
/// lives; the header cell is the label it was paired with, and equals the
/// value cell for inline, formula and comment detections.
#[derive(Debug, Clone, PartialEq)]
pub struct Finding {
    /// Name of the sheet holding the value cell, as supplied by the caller.
    pub sheet: String,
    /// Row of the header/label cell.
    pub header_row: u32,
    /// Column of the header/label cell.
    pub header_col: u32,
    /// Row of the value cell.
    pub value_row: u32,
    /// Column of the value cell.
    pub value_col: u32,
    /// The header text (e.g., "Password").
    pub key: String,
    /// The detected credential value.
    pub value: String,
    /// Heuristic confidence score.
    pub confidence: f32,
    /// Semicolon-separated breakdown of scoring factors.
    pub reason: String,
}

// ---------- Region ----------

/// A cluster of spatially adjacent candidate cells.
#[derive(Debug, Clone)]
pub struct Region {
    /// Sequential region identifier.
    pub region_id: usize,
    /// All cell indices belonging to this region.
    pub cell_ids: Vec<usize>,
    /// Indices of cells classified as Header within this region.
    pub header_ids: Vec<usize>,
    /// Indices of cells classified as Value within this region.
    pub value_ids: Vec<usize>,
}

// ---------- Research candidate export ----------

/// The rule that proposed a research candidate (see `research.rs`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum Pathway {
    /// A value near a password label, at any score.
    Spatial = 0,
    /// A cell in the column below a password label, whether or not the column
    /// qualifies as a credential table.
    Table = 1,
    /// `key: value`, `key=value` or `key - value` inside one cell.
    Inline = 2,
    /// `the password is x` or `pwd x` inside one cell.
    Prose = 3,
    /// The value of a configuration variable (`MYSQL_ROOT_PASSWORD`).
    Variable = 4,
    /// A string literal inside a formula.
    Formula = 5,
    /// A credential in a cell comment, or a comment on a label.
    Comment = 6,
    /// A password split over the cells below its label.
    Split = 7,
    /// The first value right of or below a cell holding a credential word, up
    /// to `GAP_MAX_EMPTY` empty cells away (from two cells away for a
    /// password label, whose neighbours are spatial candidates).
    Gap = 8,
}

impl Pathway {
    /// The pathway's name, as used in research exports.
    pub fn as_str(self) -> &'static str {
        match self {
            Pathway::Spatial => "spatial",
            Pathway::Table => "table",
            Pathway::Inline => "inline",
            Pathway::Prose => "prose",
            Pathway::Variable => "variable",
            Pathway::Formula => "formula",
            Pathway::Comment => "comment",
            Pathway::Split => "split",
            Pathway::Gap => "gap",
        }
    }
}

/// Number of features on each research candidate.
pub const FEATURE_COUNT: usize = 36;

/// Names of the research candidate features, in vector order.
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    // value
    "value_len",
    "value_upper",
    "value_lower",
    "value_digit",
    "value_special",
    "value_space",
    "value_entropy",
    "value_is_amount",
    "value_is_date",
    "value_is_url_or_email",
    "value_is_placeholder",
    "value_is_keyword",
    "value_is_formula",
    // label
    "label_keyword_class",
    "label_compound",
    "label_exact",
    "label_ambiguous",
    "label_len",
    "label_assignment_end",
    "label_in_header_row",
    "label_non_secret",
    // geometry
    "row_offset",
    "col_offset",
    "empty_between",
    "same_region",
    "distance_score",
    "in_table_column",
    // context
    "username_nearby",
    "url_nearby",
    "column_len",
    "column_distinct_share",
    "column_shape_share",
    "sheet_hidden",
    "sheet_labels",
    // pathway
    "pathway",
    "baseline_confidence",
];

/// A credential candidate for the learned-scorer experiment.
///
/// It locates the value and its label and carries features, never the text
/// of the value.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    /// Name of the sheet holding the value cell, as supplied by the caller.
    pub sheet: String,
    /// Row of the label cell (the value cell for single-cell pathways).
    pub label_row: u32,
    /// Column of the label cell.
    pub label_col: u32,
    /// Row of the value cell.
    pub value_row: u32,
    /// Column of the value cell.
    pub value_col: u32,
    /// The rule that proposed the candidate.
    pub pathway: Pathway,
    /// Character range of the value within the value cell's text, when the
    /// value is a part of that text (inline and prose); `None` otherwise.
    pub span: Option<(u32, u32)>,
    /// Feature vector, named by `FEATURE_NAMES`.
    pub features: [f32; FEATURE_COUNT],
}

/// Most empty cells allowed between a label and its value for the gap
/// pathway, and empty rows within a research table column.
pub const GAP_MAX_EMPTY: u32 = 3;
/// Label words that also have common meanings outside credentials (`Pwd` for
/// powder, a swim `Pass`, a machine `PIN`, a `Secret` title, a `Login` topic).
pub const AMBIGUOUS_KEYWORDS: &[&str] = &["pass", "pwd", "pin", "secret", "login", "key"];
/// Label word fragments that name a credential without ambiguity.
pub const STRONG_KEYWORDS: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "passcode",
    "passwort",
    "token",
    "totp",
];
/// Label words that mark a setting about a secret rather than the secret
/// (`*_PASSWORD_FILE`, `..._PATH`, `..._HASH`, `RANDOM_ROOT_PASSWORD`,
/// `ALLOW_EMPTY_PASSWORD`).
pub const NON_SECRET_LABEL_PARTS: &[&str] = &["file", "path", "hash", "random", "allow"];
/// Suffix the Python package adds to the name of a hidden sheet
/// (`python/doormat/api.py`, `_HIDDEN_SUFFIX`).
pub const HIDDEN_SHEET_SUFFIX: &str = "[HIDDEN]";

// ---------- Spatial / scoring constants ----------

/// Maximum distance (in cells) for neighbor queries and BFS expansion.
pub const NEIGHBOR_RADIUS: i32 = 3;
/// Minimum confidence score for a relationship to be accepted.
pub const RELATIONSHIP_THRESHOLD: f32 = 120.0;
/// Longest identifier-style label (`MYSQL_ROOT_PASSWORD`) checked as a
/// compound credential label.
pub const COMPOUND_LABEL_MAX_LEN: usize = 64;
/// Credential keywords recognised at the end of a label's final part even
/// when joined to another word (`ADMINPASSWORD`, `COOKIESECRET`). Shorter
/// keywords (`pass`, `pwd`) must be a whole part, so `bypass` never matches.
pub const COMPOUND_SUFFIXES: &[&str] = &[
    "password",
    "passwd",
    "passphrase",
    "passcode",
    "secret",
    "token",
];
/// Final label parts that name key material (`WORDPRESS_AUTH_KEY`,
/// `..._SALT`). They are weaker than credential words (`PRIMARY_KEY` is a
/// column name), so their value must also look like key material.
pub const KEY_PARTS: &[&str] = &["key", "salt"];
/// Shortest value accepted as key material for a `KEY_PARTS` label; it must
/// also mix upper, lower and digits with entropy of at least `ENTROPY_THRESHOLD`.
pub const KEY_MATERIAL_MIN_LEN: usize = 16;
/// Values (normalized) that are switches, not secrets: `MYSQL_RANDOM_ROOT_PASSWORD=yes`.
pub const BOOLEAN_LITERALS: &[&str] = &[
    "yes", "no", "true", "false", "on", "off", "y", "n", "0", "1",
];
/// Minimum value length to be considered a credential candidate.
pub const MIN_CANDIDATE_LENGTH: usize = 4;
/// A password label whose exact text repeats this many times in its own column
/// is a category value in a data column (a help-desk log), not a label.
pub const CATEGORY_REPEAT: u32 = 3;
/// A form label ending in `:` takes its value from the right; if none of this
/// many cells to its right is filled, the field is empty.
pub const FORM_FIELD_REACH: u32 = 2;
/// Confidence of the value of a compound credential label in a list of
/// configuration variables (`MYSQL_ROOT_PASSWORD | x`).
pub const VARIABLE_VALUE_CONFIDENCE: f32 = 190.0;
/// Confidence of a value found in a credential table column.
pub const TABLE_COLUMN_CONFIDENCE: f32 = 190.0;
/// A credential table needs at least this many rows below its header.
pub const TABLE_MIN_ROWS: usize = 2;
/// Longest run of rows read below a credential table header.
pub const TABLE_MAX_ROWS: u32 = 1000;
/// Extra labels (normalized) that name the account column of a credential
/// table, beside the general username labels. A bare `ID` is too common to
/// count as a username anywhere else.
pub const TABLE_USERNAME_LABELS: &[&str] = &["id", "uid", "loginid", "logonid"];
/// How many columns either side of a password header the header row is
/// searched for the account (username) label of a credential table.
pub const TABLE_HEADER_REACH: u32 = 10;
/// Values (normalized) that stand in for a missing secret in a credential
/// table, taken from the corpus labelling rubric.
pub const TABLE_PLACEHOLDERS: &[&str] = &["n/a", "na", "tbd", "none", "?", "xxxx", "****"];
/// Empty rows allowed between two rows of a credential table column.
pub const TABLE_MAX_GAP: u32 = 1;
/// Share of a column's cells that must look like single tokens.
pub const TABLE_TOKEN_SHARE: f32 = 0.8;
/// Shortest cell reported from a credential table column.
pub const TABLE_MIN_VALUE_LEN: usize = 3;

// ---------- Entropy constants ----------

/// Shannon entropy threshold for high-entropy bonus in scoring.
pub const ENTROPY_THRESHOLD: f32 = 3.5;
/// Minimum string length before entropy is computed.
pub const ENTROPY_MIN_LENGTH: usize = 8;

// ---------- Feature-flag bitmasks (u16, 14 bits used) ----------

/// Cell value contains at least one uppercase letter.
pub const FLAG_HAS_UPPER: u16 = 0b0000_0001;
/// Cell value contains at least one lowercase letter.
pub const FLAG_HAS_LOWER: u16 = 0b0000_0010;
/// Cell value contains at least one digit.
pub const FLAG_HAS_DIGIT: u16 = 0b0000_0100;
/// Cell value contains at least one non-alphanumeric, non-whitespace character.
pub const FLAG_HAS_SPECIAL: u16 = 0b0000_1000;
/// Cell value is shorter than `MIN_CANDIDATE_LENGTH`.
pub const FLAG_IS_SHORT: u16 = 0b0001_0000;
/// Cell value consists entirely of digits.
pub const FLAG_IS_NUMERIC: u16 = 0b0010_0000;
/// Cell value consists entirely of alphabetic characters.
pub const FLAG_IS_ALPHA: u16 = 0b0100_0000;
/// Cell value contains a colon character.
pub const FLAG_HAS_COLON: u16 = 0b1000_0000;
/// Cell value is empty.
pub const FLAG_IS_EMPTY: u16 = 0b0_0001_0000_0000;
/// Cell has a formula string.
pub const FLAG_HAS_FORMULA: u16 = 0b0_0010_0000_0000;
/// Normalized value matches a password header keyword.
pub const FLAG_IS_PASSWORD_HEADER: u16 = 0b0_0100_0000_0000;
/// Normalized value matches a username header keyword.
pub const FLAG_IS_USERNAME_HEADER: u16 = 0b0_1000_0000_0000;
/// Normalized value matches a URL header keyword.
pub const FLAG_IS_URL_HEADER: u16 = 0b01_0000_0000_0000;
/// Cell has a comment annotation.
pub const FLAG_HAS_COMMENT: u16 = 0b10_0000_0000_0000;
/// Normalized value contains a password keyword anywhere (`mysqlrootpassword`);
/// set in the same automaton pass as `FLAG_IS_PASSWORD_HEADER`.
pub const FLAG_HAS_CREDENTIAL_WORD: u16 = 0b100_0000_0000_0000;
/// Value is a compound key label (`WORDPRESS_AUTH_KEY`, `..._SALT`).
pub const FLAG_IS_KEY_LABEL: u16 = 0b1000_0000_0000_0000;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finding_field_access() {
        let f = Finding {
            sheet: "Servers".into(),
            header_row: 3,
            header_col: 1,
            value_row: 3,
            value_col: 2,
            key: "Password".into(),
            value: "s3cret!!".into(),
            confidence: 160.0,
            reason: "distance=100".into(),
        };
        assert_eq!(f.sheet, "Servers");
        assert_eq!((f.value_row, f.value_col), (3, 2));
        assert_eq!((f.header_row, f.header_col), (3, 1));
    }

    #[test]
    fn relationship_field_access() {
        let rel = Relationship {
            header_cell_id: 0,
            value_cell_id: 1,
            key: "Name".into(),
            value: "Alice".into(),
            confidence: 0.95,
            reason: "adjacent".into(),
        };
        assert_eq!(rel.header_cell_id, 0);
        assert_eq!(rel.value_cell_id, 1);
        assert_eq!(rel.key, "Name");
        assert_eq!(rel.value, "Alice");
        assert!((rel.confidence - 0.95).abs() < f32::EPSILON);
        assert_eq!(rel.reason, "adjacent");
    }

    #[test]
    fn cell_type_repr() {
        assert_eq!(CellType::Empty as u8, 0);
        assert_eq!(CellType::Header as u8, 1);
        assert_eq!(CellType::Value as u8, 2);
        assert_eq!(CellType::Section as u8, 3);
    }

    #[test]
    fn cell_type_default_is_empty() {
        assert_eq!(CellType::default(), CellType::Empty);
    }

    #[test]
    fn pathway_names_are_distinct() {
        let all = [
            Pathway::Spatial,
            Pathway::Table,
            Pathway::Inline,
            Pathway::Prose,
            Pathway::Variable,
            Pathway::Formula,
            Pathway::Comment,
            Pathway::Split,
            Pathway::Gap,
        ];
        let names: std::collections::BTreeSet<&str> = all.iter().map(|p| p.as_str()).collect();
        assert_eq!(names.len(), all.len());
        assert_eq!(Pathway::Gap as u8, 8);
    }

    #[test]
    fn feature_names_are_distinct() {
        let names: std::collections::BTreeSet<&str> = FEATURE_NAMES.iter().copied().collect();
        assert_eq!(names.len(), FEATURE_COUNT);
        assert_eq!(FEATURE_NAMES[FEATURE_COUNT - 1], "baseline_confidence");
    }

    #[test]
    fn cell_type_from_u8_roundtrip() {
        assert_eq!(CellType::from_u8(0), CellType::Empty);
        assert_eq!(CellType::from_u8(1), CellType::Header);
        assert_eq!(CellType::from_u8(2), CellType::Value);
        assert_eq!(CellType::from_u8(3), CellType::Section);
        assert_eq!(CellType::from_u8(255), CellType::Empty);
    }

    #[test]
    fn flags_are_distinct_bits() {
        let all = FLAG_HAS_UPPER
            | FLAG_HAS_LOWER
            | FLAG_HAS_DIGIT
            | FLAG_HAS_SPECIAL
            | FLAG_IS_SHORT
            | FLAG_IS_NUMERIC
            | FLAG_IS_ALPHA
            | FLAG_HAS_COLON
            | FLAG_IS_EMPTY
            | FLAG_HAS_FORMULA
            | FLAG_IS_PASSWORD_HEADER
            | FLAG_IS_USERNAME_HEADER
            | FLAG_IS_URL_HEADER
            | FLAG_HAS_COMMENT;
        assert_eq!(all, 0b11_1111_1111_1111);
    }
}
