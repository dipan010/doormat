//! One worksheet: its cells as openpyxl 3.1 binds them (counters, value
//! casting, shared formulas, duplicate coordinates), then merged ranges and
//! comments, in openpyxl's order.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::BufRead;

use crate::formula::{column_index, coordinate_to_tuple, Translator};
use crate::styles::Styles;
use crate::values::{
    cast_number, excel_datetime_text, excel_timedelta_text, iso8601_text, number_text,
    python_float, python_int, Number,
};
use crate::xml::{text_content, Element, Node, Parser};
use crate::XlsxError;

/// Read the shared strings part: each `<si>`'s plain text, with `x005F_` removed.
pub fn read_shared_strings<R: BufRead>(source: R) -> Result<Vec<String>, XlsxError> {
    let mut parser = Parser::new(source);
    let mut strings = Vec::new();
    while let Node::Open(el) = parser.next_node()? {
        if el.is_main("si") {
            let si = parser.subtree(el)?;
            strings.push(text_content(&si).replace("x005F_", ""));
        }
    }
    Ok(strings)
}

/// Counts cells as rows are parsed and refuses the file past a limit.
#[derive(Debug, Clone)]
pub struct Budget {
    limit: Option<u64>,
    filename: String,
    /// Cells counted so far.
    pub used: u64,
}

impl Budget {
    /// A budget of `limit` cells (`None`: unlimited) for the file `filename`.
    pub fn new(limit: Option<u64>, filename: &str) -> Self {
        Budget {
            limit,
            filename: filename.to_string(),
            used: 0,
        }
    }

    /// Count `n` more cells; `CellLimitError` past the limit.
    pub fn take(&mut self, n: u64) -> Result<(), XlsxError> {
        self.used += n;
        match self.limit {
            Some(limit) if self.used > limit => Err(XlsxError::new(
                "CellLimitError",
                format!(
                    "Refusing {}: it holds more than {} cells (raise or disable the limit with max_cells)",
                    self.filename,
                    thousands(limit)
                ),
            )),
            _ => Ok(()),
        }
    }
}

fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// What the workbook supplies to every sheet.
pub struct Context<'a> {
    /// Shared strings.
    pub strings: &'a [String],
    /// Cell styles.
    pub styles: &'a Styles,
    /// True if dates count from 1904.
    pub date1904: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Empty,
    Text(String),
    Formula(String),
    DataTable,
}

#[derive(Debug, Clone)]
struct Cell {
    value: Value,
    comment: Option<String>,
}

enum StyleRef {
    Index(i64),
    EmptyText,
}

/// A merged range: (min_row, min_col, max_row, max_col).
type Range = (i64, i64, i64, i64);

/// A worksheet being read.
pub struct Sheet<'a> {
    ctx: &'a Context<'a>,
    cells: BTreeMap<(i64, i64), Cell>,
    shared: HashMap<Option<String>, Translator>,
    merges: Vec<Range>,
    hyperlinks: Vec<Element>,
    row_counter: i64,
}

fn value_error(message: impl Into<String>) -> XlsxError {
    XlsxError::new("ValueError", message)
}

impl<'a> Sheet<'a> {
    /// An empty sheet reading against `ctx`.
    pub fn new(ctx: &'a Context<'a>) -> Self {
        Sheet {
            ctx,
            cells: BTreeMap::new(),
            shared: HashMap::new(),
            merges: Vec::new(),
            hyperlinks: Vec::new(),
            row_counter: 0,
        }
    }

    /// Parse the worksheet part: rows and merged ranges, counting cells in `budget`.
    pub fn parse<R: BufRead>(&mut self, source: R, budget: &mut Budget) -> Result<(), XlsxError> {
        let mut parser = Parser::new(source);
        let mut merges: Option<Vec<String>> = None;
        while let Node::Open(el) = parser.next_node()? {
            if el.is_main("row") {
                let row = parser.row(el)?;
                self.parse_row(&row, budget)?;
            } else if el.is_main("hyperlinks") {
                let list = parser.subtree(el)?;
                let links: Vec<Element> = list.children_named("hyperlink").cloned().collect();
                if links.iter().any(|link| link.get("ref").is_none()) {
                    return Err(XlsxError::new("TypeError", "expected <class 'str'>"));
                }
                self.hyperlinks = links;
            } else if el.is_main("mergeCells") {
                let list = parser.subtree(el)?;
                merges = Some(
                    list.children_named("mergeCell")
                        .map(|m| m.get("ref").unwrap_or("").to_string())
                        .collect(),
                );
            }
        }
        for reference in merges.unwrap_or_default() {
            let range = merge_range(&reference)?;
            self.merges.push(range);
        }
        Ok(())
    }

    fn parse_row(&mut self, row: &Element, budget: &mut Budget) -> Result<(), XlsxError> {
        self.row_counter = match row.get("r") {
            None => self.row_counter + 1,
            Some(r) => match python_int(r) {
                Some(Number::Int(v)) => {
                    i64::try_from(v).map_err(|_| value_error("row number too large"))?
                }
                Some(_) => return Err(value_error("row number too large")),
                None => python_float(r)
                    .filter(|f| f.fract() == 0.0 && f.abs() < 9e15)
                    .map(|f| f as i64)
                    .ok_or_else(|| value_error(format!("{r} is not a valid row number")))?,
            },
        };
        let mut col_counter = 0_i64;
        let parsed = row
            .children
            .iter()
            .map(|el| self.parse_cell(el, &mut col_counter))
            .collect::<Result<Vec<_>, _>>()?;
        budget.take(row.children.len() as u64)?;
        for (row_n, col, value, style) in parsed {
            match style {
                StyleRef::EmptyText => {
                    return Err(XlsxError::new(
                        "TypeError",
                        "list indices must be integers or slices, not str",
                    ))
                }
                StyleRef::Index(i) => {
                    if self.ctx.styles.resolve(i).is_none() {
                        return Err(XlsxError::new("IndexError", "list index out of range"));
                    }
                }
            }
            self.cells.insert(
                (row_n, col),
                Cell {
                    value,
                    comment: None,
                },
            );
        }
        Ok(())
    }

    fn parse_cell(
        &mut self,
        el: &Element,
        col_counter: &mut i64,
    ) -> Result<(i64, i64, Value, StyleRef), XlsxError> {
        let data_type = el.get("t").unwrap_or("n");
        let coordinate = el.get("r");
        let style = match el.get("s") {
            None => StyleRef::Index(0),
            Some("") => StyleRef::EmptyText,
            Some(s) => match python_int(s) {
                Some(Number::Int(v)) => StyleRef::Index(i64::try_from(v).unwrap_or(i64::MAX)),
                Some(_) => StyleRef::Index(i64::MAX),
                None => {
                    return Err(value_error(format!(
                        "invalid literal for int() with base 10: '{s}'"
                    )))
                }
            },
        };
        let raw = if data_type == "inlineStr" {
            None
        } else {
            el.find_main("v")
                .map(|v| v.text.clone())
                .filter(|t| !t.is_empty())
        };
        let (row, col) = match coordinate.filter(|c| !c.is_empty()) {
            Some(c) => {
                let (row, col) = coordinate_to_tuple(c)
                    .ok_or_else(|| value_error(format!("invalid cell coordinate {c}")))?;
                *col_counter = col;
                (row, col)
            }
            None => {
                *col_counter += 1;
                (self.row_counter, *col_counter)
            }
        };
        let value = if let Some(f) = el.find_main("f") {
            self.formula(f, coordinate)?
        } else if let Some(text) = raw {
            match data_type {
                "n" => {
                    let n = cast_number(&text).ok_or_else(|| {
                        value_error(format!("could not convert string to number: '{text}'"))
                    })?;
                    let index = match style {
                        StyleRef::Index(i) if i >= 0 => Some(i as usize),
                        _ => None,
                    };
                    let is = |flags: &[bool]| {
                        index.is_some_and(|i| flags.get(i).copied().unwrap_or(false))
                    };
                    let text = if is(&self.ctx.styles.dates) {
                        let converted = if is(&self.ctx.styles.durations) {
                            excel_timedelta_text(&n)
                        } else {
                            excel_datetime_text(&n, self.ctx.date1904)
                        };
                        converted.unwrap_or_else(|| "#VALUE!".to_string())
                    } else {
                        number_text(&n)
                    };
                    Value::Text(text)
                }
                "s" => {
                    let index = python_int(&text).ok_or_else(|| {
                        value_error(format!("invalid literal for int() with base 10: '{text}'"))
                    })?;
                    let n = self.ctx.strings.len() as i128;
                    let i = match index {
                        Number::Int(i) if i < 0 => i + n,
                        Number::Int(i) => i,
                        _ => -1,
                    };
                    if !(0..n).contains(&i) {
                        return Err(XlsxError::new("IndexError", "list index out of range"));
                    }
                    Value::Text(self.ctx.strings[i as usize].clone())
                }
                "b" => {
                    let n = python_int(&text).ok_or_else(|| {
                        value_error(format!("invalid literal for int() with base 10: '{text}'"))
                    })?;
                    let truthy = !matches!(n, Number::Int(0));
                    Value::Text(if truthy { "True" } else { "False" }.to_string())
                }
                "d" => Value::Text(
                    iso8601_text(&text)
                        .ok_or_else(|| value_error(format!("Invalid datetime value {text}")))?,
                ),
                _ => Value::Text(text),
            }
        } else if data_type == "inlineStr" {
            match el.find_main("is") {
                Some(is) => Value::Text(text_content(is)),
                None => Value::Empty,
            }
        } else {
            Value::Empty
        };
        Ok((row, col, value, style))
    }

    fn formula(&mut self, f: &Element, coordinate: Option<&str>) -> Result<Value, XlsxError> {
        let text = format!("={}", f.text);
        let to_error = |e: crate::formula::FormulaError| XlsxError::new(e.kind, e.message);
        Ok(match f.get("t") {
            Some("array") => Value::Formula(text),
            Some("shared") => {
                let index = f.get("si").map(str::to_string);
                if let Some(master) = self.shared.get(&index) {
                    Value::Formula(master.translate(coordinate).map_err(to_error)?)
                } else {
                    if text != "=" {
                        let master = Translator::new(&text, coordinate).map_err(to_error)?;
                        self.shared.insert(index, master);
                    }
                    Value::Formula(text)
                }
            }
            Some("dataTable") => Value::DataTable,
            _ => Value::Formula(text),
        })
    }

    /// Bind hyperlinks after the merged ranges: openpyxl writes a link's
    /// target (or location) into each linked cell whose value is empty,
    /// creating the cell if needed; a link on a merged cell goes to the
    /// range's first cell.
    pub fn bind_hyperlinks(
        &mut self,
        rels: &[crate::package::Relationship],
    ) -> Result<(), XlsxError> {
        let merged = MergeIndex::new(&self.merges);
        for link in std::mem::take(&mut self.hyperlinks) {
            let reference = link.get("ref").unwrap_or("");
            let id = link
                .get_ns(crate::xml::REL_NS, "id")
                .or_else(|| link.get("id"));
            let target = match id.filter(|id| !id.is_empty()) {
                Some(id) => Some(
                    rels.iter()
                        .find(|r| r.id.as_deref() == Some(id))
                        .map(|r| r.target.clone())
                        .ok_or_else(|| {
                            XlsxError::new("KeyError", format!("Unknown relationship: {id}"))
                        })?,
                ),
                None => None,
            };
            let text = target
                .filter(|t| !t.is_empty())
                .or_else(|| link.get("location").map(str::to_string));
            let [min_col, min_row, max_col, max_row] = range_boundaries(reference)?;
            let attribute_error = || {
                XlsxError::new(
                    "AttributeError",
                    "'tuple' object has no attribute 'hyperlink'",
                )
            };
            if reference.contains(':') {
                let not_iterable = || XlsxError::new("TypeError", "'Cell' object is not iterable");
                let (r1, c1, r2, c2) = match (min_col, min_row, max_col, max_row) {
                    (Some(c1), Some(r1), Some(c2), Some(r2)) => (r1, c1, r2, c2),
                    (Some(c1), None, Some(c2), None) if c1 != c2 => {
                        (1, c1, self.max_extent().0, c2)
                    }
                    (None, Some(r1), None, Some(r2)) if r1 != r2 => {
                        (r1, 1, r2, self.max_extent().1)
                    }
                    _ => return Err(not_iterable()),
                };
                if r1 < 1 || c1 < 1 {
                    return Err(value_error("Row or column values must be at least 1"));
                }
                if (r2 - r1 + 1).saturating_mul(c2 - c1 + 1) > 1 << 24 {
                    return Err(XlsxError::new("MemoryError", "hyperlink range too large"));
                }
                for r in r1..=r2 {
                    for c in c1..=c2 {
                        if !merged.hides(r, c) {
                            self.link_cell(r, c, text.as_deref())?;
                        }
                    }
                }
            } else {
                let (Some(col), Some(row)) = (min_col, min_row) else {
                    if [min_col, min_row].iter().all(Option::is_none) {
                        return Err(XlsxError::new(
                            "IndexError",
                            format!("{reference} is not a valid coordinate or range"),
                        ));
                    }
                    return Err(attribute_error());
                };
                if !(1..=1_048_576).contains(&row) {
                    return Err(value_error(format!(
                        "Row numbers must be between 1 and 1048576. Row number supplied was {row}"
                    )));
                }
                let (row, col) = if merged.hides(row, col) {
                    let origin = self
                        .merges
                        .iter()
                        .find(|&&(r1, c1, r2, c2)| {
                            (r1..=r2).contains(&row) && (c1..=c2).contains(&col)
                        })
                        .map(|&(r1, c1, _, _)| (r1, c1));
                    match origin {
                        Some((r, c)) if !merged.hides(r, c) => (r, c),
                        _ => {
                            return Err(XlsxError::new(
                                "AttributeError",
                                "object has no attribute 'hyperlink'",
                            ))
                        }
                    }
                } else {
                    (row, col)
                };
                self.link_cell(row, col, text.as_deref())?;
            }
        }
        Ok(())
    }

    /// The highest row and column of any cell, merged range included (`max_row`, `max_column`).
    fn max_extent(&self) -> (i64, i64) {
        let rows = self
            .cells
            .keys()
            .map(|k| k.0)
            .chain(self.merges.iter().map(|m| m.2));
        let cols = self
            .cells
            .keys()
            .map(|k| k.1)
            .chain(self.merges.iter().map(|m| m.3));
        (rows.max().unwrap_or(1), cols.max().unwrap_or(1))
    }

    /// openpyxl's hyperlink setter on one cell: an empty cell takes the link text
    /// through the value setter (cut at 32,767 characters, control characters
    /// refused, a leading `=` making it a formula).
    fn link_cell(&mut self, row: i64, col: i64, text: Option<&str>) -> Result<(), XlsxError> {
        let cell = self.cells.entry((row, col)).or_insert(Cell {
            value: Value::Empty,
            comment: None,
        });
        if cell.value != Value::Empty {
            return Ok(());
        }
        let Some(text) = text else { return Ok(()) };
        let text: String = text.chars().take(32_767).collect();
        if text
            .chars()
            .any(|c| matches!(c, '\u{0}'..='\u{8}' | '\u{b}' | '\u{c}' | '\u{e}'..='\u{1f}'))
        {
            return Err(XlsxError::new(
                "IllegalCharacterError",
                "cannot be used in worksheets",
            ));
        }
        cell.value = if text.chars().count() > 1 && text.starts_with('=') {
            Value::Formula(text)
        } else {
            Value::Text(text)
        };
        Ok(())
    }

    /// Attach the comments of one comments part, as openpyxl binds them
    /// after the merged ranges.
    pub fn add_comments(&mut self, root: &Element) -> Result<(), XlsxError> {
        let authors: Vec<&Element> = root
            .last_named("authors")
            .map(|a| a.children_named("author").collect())
            .unwrap_or_default();
        let Some(list) = root.last_named("commentList") else {
            return Ok(());
        };
        let merged = MergeIndex::new(&self.merges);
        for record in &list.children {
            let author_id = match record.get("authorId") {
                None => 0,
                Some(a) => match python_int(a) {
                    Some(Number::Int(v)) => v,
                    _ => return Err(XlsxError::new("TypeError", "expected <class 'int'>")),
                },
            };
            let n = authors.len() as i128;
            let i = if author_id < 0 {
                author_id + n
            } else {
                author_id
            };
            if !(0..n).contains(&i) {
                return Err(XlsxError::new("IndexError", "list index out of range"));
            }
            let text = record
                .last_named("text")
                .map(text_content)
                .unwrap_or_default();
            let reference = record.get("ref").unwrap_or("");
            let (row, col) = comment_cell(reference)?;
            if merged.hides(row, col) {
                continue;
            }
            self.cells
                .entry((row, col))
                .or_insert(Cell {
                    value: Value::Empty,
                    comment: None,
                })
                .comment = Some(clean_comment(&text));
        }
        Ok(())
    }

    /// The sheet's cells as doormat-core input, in row-major order, merged
    /// cells and empty cells left out.
    pub fn into_cells(
        self,
        sheet_name: &str,
    ) -> Result<Vec<doormat_core::store::RawCell>, XlsxError> {
        let merged = MergeIndex::new(&self.merges);
        let origins: HashSet<(i64, i64)> = self.merges.iter().map(|&(r, c, _, _)| (r, c)).collect();
        let mut out = Vec::new();
        for (&(row, col), cell) in &self.cells {
            if merged.hides(row, col) {
                continue;
            }
            let (value, formula) = match &cell.value {
                Value::Empty | Value::DataTable => (String::new(), String::new()),
                Value::Text(t) => (t.clone(), String::new()),
                Value::Formula(f) => (f.clone(), f.clone()),
            };
            let comment = cell.comment.clone().unwrap_or_default();
            if value.is_empty() && formula.is_empty() && comment.is_empty() {
                continue;
            }
            let overflow = || XlsxError::new("OverflowError", "cell coordinate out of range");
            out.push(doormat_core::store::RawCell {
                row: u32::try_from(row).map_err(|_| overflow())?,
                col: u32::try_from(col).map_err(|_| overflow())?,
                value,
                formula,
                comment,
                sheet_name: sheet_name.to_string(),
                is_merged_origin: origins.contains(&(row, col)),
            });
        }
        Ok(out)
    }
}

/// Cells hidden by a merged range: every cell of the range but its first.
/// Small ranges are expanded into a set; large ones are tested directly, so a
/// range covering a whole sheet costs nothing up front.
struct MergeIndex {
    small: HashSet<(i64, i64)>,
    large: Vec<Range>,
}

impl MergeIndex {
    fn new(merges: &[Range]) -> Self {
        let mut index = MergeIndex {
            small: HashSet::new(),
            large: Vec::new(),
        };
        for &(r1, c1, r2, c2) in merges {
            let area = (r2 - r1 + 1).saturating_mul(c2 - c1 + 1);
            if area <= 4096 && index.small.len() < 1_000_000 {
                for r in r1..=r2 {
                    for c in c1..=c2 {
                        if (r, c) != (r1, c1) {
                            index.small.insert((r, c));
                        }
                    }
                }
            } else {
                index.large.push((r1, c1, r2, c2));
            }
        }
        index
    }

    fn hides(&self, r: i64, c: i64) -> bool {
        self.small.contains(&(r, c))
            || self.large.iter().any(|&(r1, c1, r2, c2)| {
                (r1..=r2).contains(&r) && (c1..=c2).contains(&c) && (r, c) != (r1, c1)
            })
    }
}

/// `clean_comment`: strip, drop a leading `Comment:\n`, strip again.
fn clean_comment(raw: &str) -> String {
    let text = raw.trim();
    let text = text.strip_prefix("Comment:\n").unwrap_or(text);
    text.trim().to_string()
}

/// One side of openpyxl's `ABSOLUTE_RE`: `[$]?[A-Za-z]{1,3}?[$]?\d*?`.
fn range_part(s: &str) -> Option<(Option<&str>, Option<&str>)> {
    let s = s.strip_prefix('$').unwrap_or(s);
    let letters = s.chars().take_while(char::is_ascii_alphabetic).count();
    if letters > 3 {
        return None;
    }
    let (col, rest) = s.split_at(letters);
    let rest = rest.strip_prefix('$').unwrap_or(rest);
    if !rest.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    Some((
        (!col.is_empty()).then_some(col),
        (!rest.is_empty()).then_some(rest),
    ))
}

/// openpyxl `range_boundaries`: (min_col, min_row, max_col, max_row).
fn range_boundaries(reference: &str) -> Result<[Option<i64>; 4], XlsxError> {
    let invalid = || value_error(format!("{reference} is not a valid coordinate or range"));
    let (first, second) = match reference.split_once(':') {
        Some((a, b)) => (a, Some(b)),
        None => (reference, None),
    };
    let (min_col, min_row) = range_part(first).ok_or_else(invalid)?;
    let (max_col, max_row) = match second {
        Some(b) => range_part(b).ok_or_else(invalid)?,
        None => (None, None),
    };
    if second.is_some() {
        let all = [min_col, max_col].iter().all(Option::is_some)
            && [min_row, max_row].iter().all(Option::is_some);
        let cols_only =
            min_col.is_some() && max_col.is_some() && min_row.is_none() && max_row.is_none();
        let rows_only =
            min_row.is_some() && max_row.is_some() && min_col.is_none() && max_col.is_none();
        if !(all || cols_only || rows_only) {
            return Err(invalid());
        }
    }
    let col = |c: Option<&str>| -> Result<Option<i64>, XlsxError> {
        c.map(|c| column_index(c).ok_or_else(invalid)).transpose()
    };
    let row = |r: Option<&str>| -> Result<Option<i64>, XlsxError> {
        r.map(|r| r.parse::<i64>().map_err(|_| invalid()))
            .transpose()
    };
    let (min_col, min_row) = (col(min_col)?, row(min_row)?);
    let max_col = col(max_col)?.or(min_col);
    let max_row = row(max_row)?.or(min_row);
    Ok([min_col, min_row, max_col, max_row])
}

/// A merged range as openpyxl's `CellRange` validates it.
fn merge_range(reference: &str) -> Result<Range, XlsxError> {
    let [min_col, min_row, max_col, max_row] = range_boundaries(reference)?;
    let (Some(c1), Some(r1), Some(c2), Some(r2)) = (min_col, min_row, max_col, max_row) else {
        return Err(XlsxError::new("TypeError", "expected <class 'int'>"));
    };
    if !(1..=1_048_576).contains(&r1) || !(1..=1_048_576).contains(&r2) {
        return Err(value_error("Max value is 1048576"));
    }
    if c1 > c2 || r1 > r2 {
        return Err(value_error(format!("{reference} is not a valid range")));
    }
    Ok((r1, c1, r2, c2))
}

/// The cell `ws[reference]` returns for a comment, or the error it raises.
fn comment_cell(reference: &str) -> Result<(i64, i64), XlsxError> {
    let [min_col, min_row, max_col, max_row] = range_boundaries(reference)?;
    if [min_col, min_row, max_col, max_row]
        .iter()
        .all(Option::is_none)
    {
        return Err(XlsxError::new(
            "IndexError",
            format!("{reference} is not a valid coordinate or range"),
        ));
    }
    let (Some(col), Some(row)) = (min_col, min_row) else {
        return Err(XlsxError::new(
            "AttributeError",
            "'tuple' object has no attribute 'comment'",
        ));
    };
    if reference.contains(':') {
        return Err(XlsxError::new(
            "AttributeError",
            "'tuple' object has no attribute 'comment'",
        ));
    }
    if !(1..=1_048_576).contains(&row) {
        return Err(value_error(format!(
            "Row numbers must be between 1 and 1048576. Row number supplied was {row}"
        )));
    }
    Ok((row, col))
}

/// openpyxl's sheet title rules: an empty name becomes `Sheet`; `\ * ? : / [ ]` are refused.
pub fn sheet_title(name: &str) -> Result<String, XlsxError> {
    if let Some(c) = name.chars().find(|c| "\\*?:/[]".contains(*c)) {
        return Err(value_error(format!(
            "Invalid character {c} found in sheet title"
        )));
    }
    Ok(if name.is_empty() {
        "Sheet".to_string()
    } else {
        name.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(
        xml: &str,
        strings: &[String],
        styles: &Styles,
    ) -> Result<Vec<doormat_core::store::RawCell>, XlsxError> {
        let ctx = Context {
            strings,
            styles,
            date1904: false,
        };
        let mut sheet = Sheet::new(&ctx);
        sheet.parse(xml.as_bytes(), &mut Budget::new(None, "t.xlsx"))?;
        sheet.into_cells("S")
    }

    fn values(cells: &[doormat_core::store::RawCell]) -> Vec<(u32, u32, &str, &str)> {
        cells
            .iter()
            .map(|c| (c.row, c.col, c.value.as_str(), c.formula.as_str()))
            .collect()
    }

    const NS: &str = r#"xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main""#;

    #[test]
    fn values_and_counters() {
        let xml = format!(
            r#"<worksheet {NS}><sheetData><row r="2"><c r="B2" t="s"><v>1</v></c><c><v>3.50</v></c><c t="b"><v>1</v></c><c t="inlineStr"><is><t>in</t></is></c><c t="e"><v>#N/A</v></c><c t="str"><v>x</v></c></row><row><c><v>007</v></c></row></sheetData></worksheet>"#
        );
        let strings = vec!["zero".to_string(), "one".to_string()];
        let cells = read(&xml, &strings, &Styles::default()).unwrap();
        assert_eq!(
            values(&cells),
            vec![
                (2, 2, "one", ""),
                (2, 3, "3.5", ""),
                (2, 4, "True", ""),
                (2, 5, "in", ""),
                (2, 6, "#N/A", ""),
                (2, 7, "x", ""),
                (3, 1, "7", "")
            ]
        );
    }

    #[test]
    fn shared_formulas_and_merges() {
        let xml = format!(
            r#"<worksheet {NS}><sheetData><row r="1"><c r="A1"><f t="shared" si="0" ref="A1:A3">B1*2</f></c><c r="C1" t="str"><v>keep</v></c><c r="D1" t="str"><v>gone</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/></c></row><row r="3"><c r="A3"><f t="array" ref="A3">SUM(B1:B2)</f></c></row></sheetData><mergeCells><mergeCell ref="C1:D1"/></mergeCells></worksheet>"#
        );
        let cells = read(&xml, &[], &Styles::default()).unwrap();
        assert_eq!(
            values(&cells),
            vec![
                (1, 1, "=B1*2", "=B1*2"),
                (1, 3, "keep", ""),
                (2, 1, "=B2*2", "=B2*2"),
                (3, 1, "=SUM(B1:B2)", "=SUM(B1:B2)")
            ]
        );
        assert!(cells.iter().all(|c| c.col != 4));
    }

    #[test]
    fn dates_and_errors() {
        let styles = Styles {
            count: 2,
            dates: vec![false, true],
            durations: vec![false, false],
        };
        let xml = format!(
            r#"<worksheet {NS}><sheetData><row><c s="1"><v>36526</v></c><c s="1"><v>1e20</v></c></row></sheetData></worksheet>"#
        );
        let cells = read(&xml, &[], &styles).unwrap();
        assert_eq!(
            values(&cells),
            vec![(1, 1, "2000-01-01 00:00:00", ""), (1, 2, "#VALUE!", "")]
        );
        let bad_style = format!(
            r#"<worksheet {NS}><sheetData><row><c s="5"><v>1</v></c></row></sheetData></worksheet>"#
        );
        assert_eq!(
            read(&bad_style, &[], &styles).unwrap_err().kind,
            "IndexError"
        );
        let bad_string = format!(
            r#"<worksheet {NS}><sheetData><row><c t="s"><v>9</v></c></row></sheetData></worksheet>"#
        );
        assert_eq!(
            read(&bad_string, &[], &styles).unwrap_err().kind,
            "IndexError"
        );
        let bad_number = format!(
            r#"<worksheet {NS}><sheetData><row><c><v>x</v></c></row></sheetData></worksheet>"#
        );
        assert_eq!(
            read(&bad_number, &[], &styles).unwrap_err().kind,
            "ValueError"
        );
    }

    #[test]
    fn comments_bind_after_merges() {
        let ctx_styles = Styles::default();
        let ctx = Context {
            strings: &[],
            styles: &ctx_styles,
            date1904: false,
        };
        let mut sheet = Sheet::new(&ctx);
        let xml = format!(
            r#"<worksheet {NS}><sheetData><row r="1"><c r="A1"><v>1</v></c></row></sheetData><mergeCells><mergeCell ref="A1:B1"/></mergeCells></worksheet>"#
        );
        sheet
            .parse(xml.as_bytes(), &mut Budget::new(None, "t.xlsx"))
            .unwrap();
        let comments = crate::xml::parse_tree(
            br#"<comments><authors><author>me</author></authors><commentList><comment ref="A1" authorId="0"><text><t>Comment:
 note </t></text></comment><comment ref="B1" authorId="0"><text><t>hidden</t></text></comment><comment ref="C5" authorId="0"><text><r><t>a</t></r><r><t>b</t></r></text></comment></commentList></comments>"#,
        )
        .unwrap();
        sheet.add_comments(&comments).unwrap();
        let cells = sheet.into_cells("S").unwrap();
        let got: Vec<_> = cells
            .iter()
            .map(|c| (c.row, c.col, c.comment.as_str(), c.is_merged_origin))
            .collect();
        assert_eq!(got, vec![(1, 1, "note", true), (5, 3, "ab", false)]);
    }

    #[test]
    fn budget_and_titles() {
        let mut budget = Budget::new(Some(2_000_000), "big.xlsx");
        assert!(budget.take(2_000_000).is_ok());
        let e = budget.take(1).unwrap_err();
        assert_eq!(e.kind, "CellLimitError");
        assert!(e.message.contains("more than 2,000,000 cells"));
        assert_eq!(sheet_title("").unwrap(), "Sheet");
        assert_eq!(sheet_title("a/b").unwrap_err().kind, "ValueError");
        assert_eq!(comment_cell("A1:B2").unwrap_err().kind, "AttributeError");
        assert_eq!(merge_range("A:B").unwrap_err().kind, "TypeError");
        assert_eq!(merge_range("B1:A1").unwrap_err().kind, "ValueError");
        let strings = read_shared_strings(
            format!(r#"<sst {NS}><si><t>a_x005F_b</t></si><si><r><t>c</t></r></si></sst>"#)
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(strings, vec!["a_b".to_string(), "c".to_string()]);
    }
}
