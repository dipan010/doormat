//! Cell styles, as far as they change a value's text: which style indexes
//! format numbers as dates or durations (openpyxl's `_date_formats` and
//! `_timedelta_formats`).

use crate::xml::Element;

/// openpyxl's `BUILTIN_FORMATS`; ids missing here have no format code.
fn builtin_format(id: i64) -> Option<&'static str> {
    Some(match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        3 => "#,##0",
        4 => "#,##0.00",
        5 => "\"$\"#,##0_);(\"$\"#,##0)",
        6 => "\"$\"#,##0_);[Red](\"$\"#,##0)",
        7 => "\"$\"#,##0.00_);(\"$\"#,##0.00)",
        8 => "\"$\"#,##0.00_);[Red](\"$\"#,##0.00)",
        9 => "0%",
        10 => "0.00%",
        11 => "0.00E+00",
        12 => "# ?/?",
        13 => "# ??/??",
        14 => "mm-dd-yy",
        15 => "d-mmm-yy",
        16 => "d-mmm",
        17 => "mmm-yy",
        18 => "h:mm AM/PM",
        19 => "h:mm:ss AM/PM",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        37 => "#,##0_);(#,##0)",
        38 => "#,##0_);[Red](#,##0)",
        39 => "#,##0.00_);(#,##0.00)",
        40 => "#,##0.00_);[Red](#,##0.00)",
        41 => r#"_(* #,##0_);_(* \(#,##0\);_(* "-"_);_(@_)"#,
        42 => r#"_("$"* #,##0_);_("$"* \(#,##0\);_("$"* "-"_);_(@_)"#,
        43 => r#"_(* #,##0.00_);_(* \(#,##0.00\);_(* "-"??_);_(@_)"#,
        44 => r#"_("$"* #,##0.00_)_("$"* \(#,##0.00\)_("$"* "-"??_)_(@_)"#,
        45 => "mm:ss",
        46 => "[h]:mm:ss",
        47 => "mmss.0",
        48 => "##0.0E+0",
        49 => "@",
        _ => return None,
    })
}

/// openpyxl `is_date_format`: in the first section, with quoted text and
/// bracketed groups other than `[h]`, `[hh]`, `[m]`, `[mm]`, `[s]`, `[ss]`
/// removed, a `d`, `m`, `h`, `y` or `s` not preceded by `_` or `\`.
pub fn is_date_format(fmt: &str) -> bool {
    let first = fmt.split(';').next().unwrap_or("");
    let stripped = strip_literals(first);
    let chars: Vec<char> = stripped.chars().collect();
    chars
        .iter()
        .enumerate()
        .any(|(i, c)| "dmhysDMHYS".contains(*c) && (i == 0 || !matches!(chars[i - 1], '_' | '\\')))
}

/// Remove `".*?"` (no newlines inside) and `\[(?!hh?\]|mm?\]|ss?\])[^\]]*\]`,
/// scanning left to right as Python's `re.sub` does.
fn strip_literals(fmt: &str) -> String {
    let chars: Vec<char> = fmt.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let skip = match chars[i] {
            '"' => chars[i + 1..]
                .iter()
                .take_while(|&&c| c != '\n')
                .position(|&c| c == '"')
                .map(|p| p + 2),
            '[' if !is_elapsed_unit(&chars[i + 1..]) => {
                chars[i + 1..].iter().position(|&c| c == ']').map(|p| p + 2)
            }
            _ => None,
        };
        match skip {
            Some(n) => i += n,
            None => {
                out.push(chars[i]);
                i += 1;
            }
        }
    }
    out
}

/// True if `rest` (after a `[`) starts `h]`, `hh]`, `m]`, `mm]`, `s]` or `ss]`.
fn is_elapsed_unit(rest: &[char]) -> bool {
    let Some(&unit) = rest.first() else {
        return false;
    };
    if !matches!(unit, 'h' | 'm' | 's') {
        return false;
    }
    match rest.get(1) {
        Some(']') => true,
        Some(&c) if c == unit => rest.get(2) == Some(&']'),
        _ => false,
    }
}

/// openpyxl `is_timedelta_format`: the first section holds `[h]`, `[hh]`,
/// `[m]`, `[mm]`, `[s]` or `[ss]` (any case).
pub fn is_timedelta_format(fmt: &str) -> bool {
    let first = fmt.split(';').next().unwrap_or("").to_ascii_lowercase();
    let chars: Vec<char> = first.chars().collect();
    chars
        .iter()
        .enumerate()
        .any(|(i, &c)| c == '[' && is_elapsed_unit(&chars[i + 1..]))
}

/// The style table worksheets index into with a cell's `s` attribute.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Styles {
    /// Number of cell styles (`cellXfs`); a cell's style index must fall inside it.
    pub count: usize,
    /// Style indexes whose number format is a date.
    pub dates: Vec<bool>,
    /// Style indexes whose number format is a duration.
    pub durations: Vec<bool>,
}

impl Default for Styles {
    /// openpyxl's defaults for a workbook without styles: one style, no dates.
    fn default() -> Self {
        Styles {
            count: 1,
            dates: vec![false],
            durations: vec![false],
        }
    }
}

impl Styles {
    /// Read `xl/styles.xml`. A stylesheet without cell styles keeps the defaults.
    pub fn from_tree(root: &Element) -> Self {
        let custom: Vec<(i64, String)> = root
            .last_named("numFmts")
            .map(|list| {
                list.children_named("numFmt")
                    .filter_map(|f| {
                        let id = f.get("numFmtId").and_then(int_attr)?;
                        Some((id, f.get("formatCode").unwrap_or("").to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let ids: Vec<i64> = root
            .last_named("cellXfs")
            .map(|xfs| {
                xfs.children_named("xf")
                    .map(|xf| xf.get("numFmtId").and_then(int_attr).unwrap_or(0))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Styles::default();
        }
        let format = |id: i64| -> Option<&str> {
            custom
                .iter()
                .rev()
                .find(|(cid, _)| *cid == id)
                .map(|(_, code)| code.as_str())
                .or_else(|| builtin_format(id))
        };
        Styles {
            count: ids.len(),
            dates: ids
                .iter()
                .map(|&id| format(id).is_some_and(is_date_format))
                .collect(),
            durations: ids
                .iter()
                .map(|&id| format(id).is_some_and(is_timedelta_format))
                .collect(),
        }
    }

    /// The style a cell's `s` index names, as a Python list index (negative
    /// counts from the end); `None` if it is out of range (`IndexError`).
    pub fn resolve(&self, index: i64) -> Option<usize> {
        let n = self.count as i64;
        let i = if index < 0 { index + n } else { index };
        (0..n).contains(&i).then_some(i as usize)
    }
}

/// An integer attribute as openpyxl's `Integer` descriptor reads it.
fn int_attr(text: &str) -> Option<i64> {
    let (negative, digits) = crate::values::python_int_digits(text)?;
    let v: i64 = digits.parse().ok()?;
    Some(if negative { -v } else { v })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::xml::parse_tree;

    #[test]
    fn date_formats_like_openpyxl() {
        for fmt in [
            "mm-dd-yy",
            "h:mm AM/PM",
            "yyyy\"年\"m\"月\"",
            "[h]:mm:ss",
            "d;@",
            "[$-409]d-mmm",
        ] {
            assert!(is_date_format(fmt), "{fmt}");
        }
        for fmt in [
            "General",
            "0.00",
            "\"days\" 0",
            "[Red]0",
            "_d0",
            "\\d0",
            "#,##0;[Red]d",
            "0;mm",
        ] {
            assert!(!is_date_format(fmt), "{fmt}");
        }
        assert!(is_timedelta_format("[H]:mm"));
        assert!(is_timedelta_format("[ss].0"));
        assert!(!is_timedelta_format("h:mm"));
        assert!(!is_timedelta_format("[hhh]"));
        assert_eq!(builtin_format(50), None);
    }

    #[test]
    fn stylesheet_indexes() {
        let xml = br#"<styleSheet xmlns="urn:x"><numFmts><numFmt numFmtId="164" formatCode="0.0"/><numFmt numFmtId="164" formatCode="yyyy"/></numFmts><cellXfs><xf numFmtId="0"/><xf numFmtId="14"/><xf numFmtId="164"/><xf numFmtId="46"/><xf/></cellXfs></styleSheet>"#;
        let styles = Styles::from_tree(&parse_tree(xml).unwrap());
        assert_eq!(styles.count, 5);
        assert_eq!(styles.dates, vec![false, true, true, true, false]);
        assert_eq!(styles.durations, vec![false, false, false, true, false]);
        assert_eq!(styles.resolve(-1), Some(4));
        assert_eq!(styles.resolve(5), None);
        let empty = Styles::from_tree(&parse_tree(b"<styleSheet/>").unwrap());
        assert_eq!(empty, Styles::default());
    }
}
