//! Deterministic synthetic CellStore generator for benchmarks.
//!
//! Produces three fixture sizes:
//!   - tiny:    100 cells, ~5 candidates
//!   - typical: 5,000 cells, ~50 candidates
//!   - large:   50,000 cells, ~500 candidates
//!
//! Cell distribution (deterministic, seeded):
//!   - 1% password headers
//!   - 1% high-entropy values
//!   - 5% formulas/comments
//!   - 93% noise

use doormat_core::store::{CellStore, RawCell};

/// Simple deterministic pseudo-random generator (xorshift32).
/// Avoids pulling in a random crate as a dev-dependency.
struct Rng {
    state: u32,
}

impl Rng {
    fn new(seed: u32) -> Self {
        Rng { state: seed }
    }

    fn next_u32(&mut self) -> u32 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        x
    }

    /// Returns a value in [0, 100).
    fn percent(&mut self) -> u32 {
        self.next_u32() % 100
    }
}

const PASSWORD_KEYWORDS: &[&str] = &[
    "Password",
    "Pwd",
    "Passcode",
    "Secret",
    "Token",
    "Passwort",
    "Contraseña",
    "パスワード",
];

const HIGH_ENTROPY_VALUES: &[&str] = &[
    "xK9$mQ2!wP4z",
    "aB3$xY9!pL2#",
    "Qw3rTy!@#456",
    "M7nB5vC3xZ1!",
    "P@ssw0rd!2024",
    "hG6$jK8&mN0!",
    "tR5%yU7*iO9(",
    "eW2!qA4@sD6#",
];

const FORMULA_TEMPLATES: &[&str] = &[
    "=SUM(A1:B5)",
    "=VLOOKUP(C1,D:D,1,FALSE)",
    "=IF(A1>10,\"yes\",\"no\")",
    "=CONCAT(A1,B1)",
    "=AVERAGE(E1:E100)",
];

const COMMENT_TEMPLATES: &[&str] = &[
    "Updated by admin",
    "Review pending",
    "Needs verification",
    "Approved 2024-01",
    "See ticket #1234",
];

/// Generate a Vec<RawCell> with the specified number of cells and
/// a realistic distribution of cell types.
pub fn generate_raw_cells(n: usize, seed: u32) -> Vec<RawCell> {
    let cols = 10u32;
    let mut rng = Rng::new(seed);
    let mut cells = Vec::with_capacity(n);

    for i in 0..n {
        let row = (i / cols as usize) as u32;
        let col = (i % cols as usize) as u32;
        let pct = rng.percent();

        let (value, formula, comment) = if pct < 1 {
            // 1%: password header
            let kw = PASSWORD_KEYWORDS[i % PASSWORD_KEYWORDS.len()];
            (kw.to_string(), String::new(), String::new())
        } else if pct < 2 {
            // 1%: high-entropy value
            let v = HIGH_ENTROPY_VALUES[i % HIGH_ENTROPY_VALUES.len()];
            (v.to_string(), String::new(), String::new())
        } else if pct < 4 {
            // 2%: formula cell
            let f = FORMULA_TEMPLATES[i % FORMULA_TEMPLATES.len()];
            (format!("result_{i}"), f.to_string(), String::new())
        } else if pct < 7 {
            // 3%: comment cell
            let c = COMMENT_TEMPLATES[i % COMMENT_TEMPLATES.len()];
            (format!("data_{i}"), String::new(), c.to_string())
        } else {
            // 93%: noise
            (format!("data_{row}_{col}"), String::new(), String::new())
        };

        cells.push(RawCell {
            row,
            col,
            value,
            formula,
            comment,
            sheet_name: "Sheet1".into(),
            is_merged_origin: false,
        });
    }

    cells
}

/// Pre-built fixture sizes.
pub fn tiny_cells() -> Vec<RawCell> {
    generate_raw_cells(100, 42)
}

pub fn typical_cells() -> Vec<RawCell> {
    generate_raw_cells(5_000, 42)
}

pub fn large_cells() -> Vec<RawCell> {
    generate_raw_cells(50_000, 42)
}

/// Build a CellStore with features precomputed (ready for candidate phases).
pub fn build_store(cells: Vec<RawCell>) -> CellStore {
    let mut store = CellStore::from_raw(cells);
    doormat_core::features::precompute_features(&mut store);
    store
}
