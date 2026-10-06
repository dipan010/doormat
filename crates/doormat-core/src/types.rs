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

// ---------- Spatial / scoring constants ----------

/// Maximum distance (in cells) for neighbor queries and BFS expansion.
pub const NEIGHBOR_RADIUS: i32 = 3;
/// Minimum confidence score for a relationship to be accepted.
pub const RELATIONSHIP_THRESHOLD: f32 = 120.0;
/// Minimum value length to be considered a credential candidate.
pub const MIN_CANDIDATE_LENGTH: usize = 4;

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

#[cfg(test)]
mod tests {
    use super::*;

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
