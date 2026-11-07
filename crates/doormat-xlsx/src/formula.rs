//! Shared formulas: openpyxl 3.1's formula `Tokenizer` and `Translator`,
//! ported so a dependent cell gets exactly the formula text openpyxl gives it.
//!
//! The tokenizer keeps only the first character of a run of spaces or
//! newlines, and the translator writes moved column letters in upper case;
//! both are reproduced, since they change the text doormat sees.

use crate::values::python_float;

/// Why a shared formula could not be tokenized or translated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormulaError {
    /// Python exception name: `TokenizerError`, `TranslatorError` or `IndexError`.
    pub kind: &'static str,
    /// Description.
    pub message: String,
}

fn err(kind: &'static str, message: impl Into<String>) -> FormulaError {
    FormulaError {
        kind,
        message: message.into(),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Literal,
    Operand,
    Func,
    Array,
    Paren,
    Sep,
    OpPre,
    OpIn,
    OpPost,
    Space,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    value: String,
    kind: Kind,
    /// `Range` operand, or the `Open`/`Close` side of a subexpression.
    range: bool,
    close: bool,
}

impl Token {
    fn new(value: impl Into<String>, kind: Kind) -> Self {
        Token {
            value: value.into(),
            kind,
            range: false,
            close: false,
        }
    }

    fn operand(value: String) -> Self {
        let range = !(value.starts_with('"')
            || value.starts_with('#')
            || value == "TRUE"
            || value == "FALSE"
            || python_float(&value).is_some());
        Token {
            value,
            kind: Kind::Operand,
            range,
            close: false,
        }
    }

    fn subexp(value: String, func: bool) -> Self {
        let kind = if func {
            Kind::Func
        } else if value == "{" || value == "}" {
            Kind::Array
        } else if value == "(" || value == ")" {
            Kind::Paren
        } else {
            Kind::Func
        };
        let close = value == ")" || value == "}";
        Token {
            value,
            kind,
            range: false,
            close,
        }
    }
}

const ERROR_CODES: [&str; 8] = [
    "#NULL!",
    "#DIV/0!",
    "#VALUE!",
    "#REF!",
    "#NAME?",
    "#NUM!",
    "#N/A",
    "#GETTING_DATA",
];
const TOKEN_ENDERS: &str = ",;}) +-*/^&=><%";

/// Tokenize a formula as openpyxl's `Tokenizer` does.
fn tokenize(formula: &str) -> Result<Vec<Token>, FormulaError> {
    let chars: Vec<char> = formula.chars().collect();
    let mut items: Vec<Token> = Vec::new();
    if chars.is_empty() {
        return Ok(items);
    }
    if chars[0] != '=' {
        items.push(Token::new(formula, Kind::Literal));
        return Ok(items);
    }
    let mut stack: Vec<Token> = Vec::new();
    let mut token: Vec<char> = Vec::new();
    let mut offset = 1;
    let at = |offset: usize| format!("Unexpected character at position {offset} in '{formula}'");

    let save = |token: &mut Vec<char>, items: &mut Vec<Token>| {
        if !token.is_empty() {
            items.push(Token::operand(token.iter().collect()));
            token.clear();
        }
    };

    while offset < chars.len() {
        let c = chars[offset];
        // Scientific notation: a sign after `1.5E` belongs to the number.
        if (c == '+' || c == '-') && !token.is_empty() && is_sci_prefix(&token) {
            token.push(c);
            offset += 1;
            continue;
        }
        if TOKEN_ENDERS.contains(c) {
            save(&mut token, &mut items);
        }
        offset += match c {
            '"' | '\'' => {
                if token.last().is_some_and(|&t| t != ':') {
                    return Err(err("TokenizerError", at(offset)));
                }
                let len = string_len(&chars[offset..]).ok_or_else(|| {
                    let what = if c == '"' { "string" } else { "link" };
                    err(
                        "TokenizerError",
                        format!("Reached end of formula while parsing {what}"),
                    )
                })?;
                let text: String = chars[offset..offset + len].iter().collect();
                if c == '"' {
                    items.push(Token::operand(text));
                } else {
                    token.extend(text.chars());
                }
                len
            }
            '[' => {
                let mut depth = 0;
                let end = chars[offset..]
                    .iter()
                    .position(|&ch| {
                        depth += match ch {
                            '[' => 1,
                            ']' => -1,
                            _ => 0,
                        };
                        ch == ']' && depth == 0
                    })
                    .ok_or_else(|| err("TokenizerError", "Encountered unmatched '['"))?;
                token.extend(&chars[offset..=offset + end]);
                end + 1
            }
            '#' => {
                if token.last().is_some_and(|&t| t != '!') {
                    return Err(err("TokenizerError", at(offset)));
                }
                let rest: String = chars[offset..].iter().collect();
                let code = ERROR_CODES
                    .iter()
                    .find(|code| rest.starts_with(*code))
                    .ok_or_else(|| {
                        err(
                            "TokenizerError",
                            format!("Invalid error code at position {offset}"),
                        )
                    })?;
                let value: String = token.drain(..).collect::<String>() + code;
                items.push(Token::operand(value));
                code.chars().count()
            }
            ' ' | '\n' => {
                items.push(Token::new(c.to_string(), Kind::Space));
                chars[offset..]
                    .iter()
                    .take_while(|&&ch| ch == ' ' || ch == '\n')
                    .count()
            }
            '+' | '-' | '*' | '/' | '^' | '&' | '=' | '>' | '<' | '%' => {
                let two: String = chars[offset..chars.len().min(offset + 2)].iter().collect();
                if two == ">=" || two == "<=" || two == "<>" {
                    items.push(Token::new(two, Kind::OpIn));
                    2
                } else {
                    let kind = match c {
                        '%' => Kind::OpPost,
                        '*' | '/' | '^' | '&' | '=' | '>' | '<' => Kind::OpIn,
                        _ => {
                            let prev = items.iter().rev().find(|t| t.kind != Kind::Space);
                            let infix = prev.is_some_and(|p| {
                                p.close || p.kind == Kind::OpPost || p.kind == Kind::Operand
                            });
                            if infix {
                                Kind::OpIn
                            } else {
                                Kind::OpPre
                            }
                        }
                    };
                    items.push(Token::new(c.to_string(), kind));
                    1
                }
            }
            '{' | '(' => {
                let t = if c == '{' {
                    if !token.is_empty() {
                        return Err(err("TokenizerError", at(offset)));
                    }
                    Token::subexp("{".into(), false)
                } else if !token.is_empty() {
                    let value = token.drain(..).collect::<String>() + "(";
                    Token::subexp(value, false)
                } else {
                    Token::subexp("(".into(), false)
                };
                items.push(t.clone());
                stack.push(t);
                1
            }
            ')' | '}' => {
                let open = stack
                    .pop()
                    .ok_or_else(|| err("IndexError", "pop from empty list"))?;
                let value = if open.kind == Kind::Array { "}" } else { ")" };
                let closer = Token::subexp(value.into(), open.kind == Kind::Func);
                if closer.value != c.to_string() {
                    return Err(err("TokenizerError", "Mismatched ( and { pair"));
                }
                items.push(closer);
                1
            }
            ';' | ',' => {
                let kind = if c == ',' && stack.last().is_none_or(|t| t.kind == Kind::Paren) {
                    Kind::OpIn
                } else {
                    Kind::Sep
                };
                items.push(Token::new(c.to_string(), kind));
                1
            }
            _ => {
                token.push(c);
                1
            }
        };
    }
    save(&mut token, &mut items);
    Ok(items)
}

/// openpyxl's `SN_RE`: `^[1-9](\.[0-9]+)?[Ee]$`.
fn is_sci_prefix(token: &[char]) -> bool {
    let n = token.len();
    if n < 2 || !matches!(token[0], '1'..='9') || !matches!(token[n - 1], 'e' | 'E') {
        return false;
    }
    let middle = &token[1..n - 1];
    middle.is_empty()
        || (middle.len() >= 2 && middle[0] == '.' && middle[1..].iter().all(char::is_ascii_digit))
}

/// Length of a `"`- or `'`-quoted string where a doubled quote escapes it.
fn string_len(chars: &[char]) -> Option<usize> {
    let quote = chars[0];
    let mut i = 1;
    loop {
        let j = i + chars[i..].iter().position(|&c| c == quote)?;
        if chars.get(j + 1) == Some(&quote) {
            i = j + 2;
        } else {
            return Some(j + 1);
        }
    }
}

/// Column letters to a 1-based index, as `column_index_from_string`.
pub fn column_index(letters: &str) -> Option<i64> {
    if letters.chars().count() > 3 {
        return None;
    }
    let idx = letters.chars().try_fold(0_i64, |acc, c| {
        let c = c.to_ascii_uppercase();
        c.is_ascii_uppercase()
            .then(|| acc * 26 + i64::from(c as u8 - b'A' + 1))
    })?;
    (1..=18278).contains(&idx).then_some(idx)
}

/// A 1-based column index to letters, as `get_column_letter`.
pub fn column_letters(mut idx: i64) -> Option<String> {
    if !(1..=18278).contains(&idx) {
        return None;
    }
    let mut out = Vec::new();
    while idx > 0 {
        let rem = (idx - 1) % 26;
        out.push((b'A' + rem as u8) as char);
        idx = (idx - 1) / 26;
    }
    Some(out.iter().rev().collect())
}

/// openpyxl's `coordinate_to_tuple`: letters up to the first digit, then the row.
pub fn coordinate_to_tuple(coordinate: &str) -> Option<(i64, i64)> {
    let split = coordinate
        .char_indices()
        .find(|(_, c)| c.is_ascii_digit())
        .map(|(i, _)| i)
        .unwrap_or_else(|| coordinate.char_indices().last().map_or(0, |(i, _)| i));
    let (col, row) = coordinate.split_at(split);
    let (negative, digits) = crate::values::python_int_digits(row)?;
    let row: i64 = digits.parse().ok()?;
    let row = if negative { -row } else { row };
    let col = column_index(col)?;
    Some((row, col))
}

fn split_dollar(s: &str) -> (&str, &str) {
    match s.strip_prefix('$') {
        Some(rest) => ("$", rest),
        None => ("", s),
    }
}

fn is_row_part(s: &str) -> bool {
    let (_, digits) = split_dollar(s);
    (1..=7).contains(&digits.len())
        && digits.bytes().all(|b| b.is_ascii_digit())
        && digits.as_bytes()[0] != b'0'
}

fn is_col_part(s: &str) -> bool {
    let (_, letters) = split_dollar(s);
    (1..=3).contains(&letters.len()) && letters.bytes().all(|b| b.is_ascii_alphabetic())
}

fn translate_row(row: &str, delta: i64) -> Result<String, FormulaError> {
    if row.starts_with('$') {
        return Ok(row.to_string());
    }
    let new_row = row.parse::<i64>().expect("row part is digits") + delta;
    if new_row <= 0 {
        return Err(err("TranslatorError", "Formula out of range"));
    }
    Ok(new_row.to_string())
}

fn translate_col(col: &str, delta: i64) -> Result<String, FormulaError> {
    if col.starts_with('$') {
        return Ok(col.to_string());
    }
    column_index(col)
        .and_then(|idx| column_letters(idx + delta))
        .ok_or_else(|| err("TranslatorError", "Formula out of range"))
}

fn translate_range(range: &str, rdelta: i64, cdelta: i64) -> Result<String, FormulaError> {
    let (ws, range) = match range.rsplit_once('!') {
        Some((sheet, rest)) => (format!("{sheet}!"), rest),
        None => (String::new(), range),
    };
    if let Some((a, b)) = range.split_once(':').filter(|(_, b)| !b.contains(':')) {
        if is_row_part(a) && is_row_part(b) {
            return Ok(format!(
                "{ws}{}:{}",
                translate_row(a, rdelta)?,
                translate_row(b, rdelta)?
            ));
        }
        if is_col_part(a) && is_col_part(b) {
            return Ok(format!(
                "{ws}{}:{}",
                translate_col(a, cdelta)?,
                translate_col(b, cdelta)?
            ));
        }
    }
    if range.contains(':') {
        let pieces = range
            .split(':')
            .map(|piece| translate_range(piece, rdelta, cdelta))
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(format!("{ws}{}", pieces.join(":")));
    }
    let letters = range
        .trim_start_matches('$')
        .chars()
        .take_while(char::is_ascii_alphabetic)
        .count();
    let col_end = range.len() - range.trim_start_matches('$').len() + letters;
    let (col, row) = range.split_at(col_end);
    if is_col_part(col) && is_row_part(row) {
        return Ok(format!(
            "{ws}{}{}",
            translate_col(col, cdelta)?,
            translate_row(row, rdelta)?
        ));
    }
    Ok(range.to_string())
}

/// A shared formula's master: its tokens and the cell it was written in.
#[derive(Debug, Clone)]
pub struct Translator {
    tokens: Vec<Token>,
    row: i64,
    col: i64,
}

impl Translator {
    /// Tokenize `formula` (with its leading `=`) written at `origin`, e.g. `"B2"`.
    pub fn new(formula: &str, origin: Option<&str>) -> Result<Self, FormulaError> {
        let origin = origin.ok_or_else(|| err("TypeError", "shared formula master without r"))?;
        let (row, col) = coordinate_to_tuple(origin)
            .ok_or_else(|| err("ValueError", format!("invalid cell reference {origin}")))?;
        Ok(Translator {
            tokens: tokenize(formula)?,
            row,
            col,
        })
    }

    /// The formula as it reads at `dest`; references are not moved when
    /// `dest` is absent or empty.
    pub fn translate(&self, dest: Option<&str>) -> Result<String, FormulaError> {
        let Some(first) = self.tokens.first() else {
            return Ok(String::new());
        };
        if first.kind == Kind::Literal {
            return Ok(first.value.clone());
        }
        let (rdelta, cdelta) = match dest.filter(|d| !d.is_empty()) {
            Some(d) => {
                let (row, col) = coordinate_to_tuple(d)
                    .ok_or_else(|| err("ValueError", format!("invalid cell reference {d}")))?;
                (row - self.row, col - self.col)
            }
            None => (0, 0),
        };
        let mut out = String::from("=");
        for token in &self.tokens {
            if token.kind == Kind::Operand && token.range {
                out.push_str(&translate_range(&token.value, rdelta, cdelta)?);
            } else {
                out.push_str(&token.value);
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn moved(formula: &str, from: &str, to: &str) -> Result<String, FormulaError> {
        Translator::new(formula, Some(from))?.translate(Some(to))
    }

    #[test]
    fn translates_relative_references() {
        assert_eq!(moved("=B1+C1", "A1", "A2").unwrap(), "=B2+C2");
        assert_eq!(moved("=$B$1+C$1+$D1", "A1", "B3").unwrap(), "=$B$1+D$1+$D3");
        assert_eq!(moved("=SUM(b1:c1)", "A1", "A2").unwrap(), "=SUM(B2:C2)");
        assert_eq!(moved("=Sheet2!A1", "A1", "A2").unwrap(), "=Sheet2!A2");
        assert_eq!(
            moved("='My Sheet'!A1", "A1", "B1").unwrap(),
            "='My Sheet'!B1"
        );
        assert_eq!(moved("=SUM(1:2)", "A1", "A3").unwrap(), "=SUM(3:4)");
        assert_eq!(moved("=SUM(A:B)", "A1", "C1").unwrap(), "=SUM(C:D)");
        assert_eq!(moved("=Total*2", "A1", "A9").unwrap(), "=Total*2");
        assert_eq!(moved("=\"A1\"&A1", "A1", "A2").unwrap(), "=\"A1\"&A2");
    }

    #[test]
    fn collapses_whitespace_runs() {
        assert_eq!(moved("=A1  +\n\n B1", "A1", "A1").unwrap(), "=A1 +\nB1");
    }

    #[test]
    fn scientific_numbers_and_errors() {
        assert_eq!(moved("=1.5E+3*A1", "A1", "A2").unwrap(), "=1.5E+3*A2");
        assert_eq!(
            moved("=IF(A1,#N/A,1)", "A1", "A2").unwrap(),
            "=IF(A2,#N/A,1)"
        );
        assert_eq!(moved("={1,2;3,4}", "A1", "A2").unwrap(), "={1,2;3,4}");
    }

    #[test]
    fn errors_match_openpyxl() {
        assert_eq!(moved("=A2", "B2", "B1").unwrap(), "=A1");
        assert_eq!(
            moved("=A1", "B2", "B1").unwrap_err().kind,
            "TranslatorError"
        );
        assert_eq!(
            moved("=A1", "B1", "A1").unwrap_err().kind,
            "TranslatorError"
        );
        assert_eq!(
            moved("=\"abc", "A1", "A2").unwrap_err().kind,
            "TokenizerError"
        );
        assert_eq!(moved("=A1)", "A1", "A2").unwrap_err().kind, "IndexError");
        assert_eq!(
            moved("=(A1}", "A1", "A2").unwrap_err().kind,
            "TokenizerError"
        );
        assert_eq!(
            moved("=#BAD", "A1", "A2").unwrap_err().kind,
            "TokenizerError"
        );
        assert_eq!(Translator::new("=A1", None).unwrap_err().kind, "TypeError");
    }

    #[test]
    fn missing_destination_keeps_references() {
        let t = Translator::new("=A1  +  1", Some("A1")).unwrap();
        assert_eq!(t.translate(None).unwrap(), "=A1 + 1");
        assert_eq!(
            Translator::new("=", Some("A1"))
                .unwrap()
                .translate(Some("A2"))
                .unwrap(),
            ""
        );
    }

    #[test]
    fn coordinates_and_columns() {
        assert_eq!(coordinate_to_tuple("B12"), Some((12, 2)));
        assert_eq!(coordinate_to_tuple("12"), None);
        assert_eq!(coordinate_to_tuple("$A$1"), None);
        assert_eq!(coordinate_to_tuple("ABCD1"), None);
        assert_eq!(column_index("zz"), Some(702));
        assert_eq!(column_letters(18278).as_deref(), Some("ZZZ"));
        assert_eq!(column_letters(18279), None);
    }
}
