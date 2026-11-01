//! Cells-only reader for `.xlsx` and `.xlsm` workbooks (design decision 0021).
//!
//! Reads the parts that hold cells and produces exactly the cell tuples the
//! openpyxl-based Python reader produced.

pub mod formula;
pub mod package;
pub mod sheet;
pub mod styles;
pub mod values;
pub mod xml;

/// A file or sheet that could not be read. `kind` is the name of the Python
/// exception the openpyxl reader raised in the same case.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxError {
    /// Python exception name, e.g. `KeyError` or `ParseError`.
    pub kind: String,
    /// Description.
    pub message: String,
}

impl XlsxError {
    /// An error of the given kind.
    pub fn new(kind: &str, message: impl Into<String>) -> Self {
        XlsxError {
            kind: kind.to_string(),
            message: message.into(),
        }
    }
}

use std::path::Path;

use doormat_core::store::RawCell;
use doormat_core::types::Finding;

use package::{
    read_manifest, read_rels, read_workbook, rels_path, resolve_sheets, Package, COMMENTS_REL,
};
use sheet::{read_shared_strings, sheet_title, Budget, Context, Sheet};
use styles::Styles;

/// A sheet that could not be read; the other sheets were still read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetError {
    /// The sheet's name as listed in the workbook.
    pub sheet: String,
    /// What stopped it.
    pub error: XlsxError,
}

/// The cells of a workbook, one list per sheet read.
#[derive(Debug, Clone)]
pub struct Extracted {
    /// Cells of each sheet read, in workbook order; hidden sheets' names end in `[HIDDEN]`.
    pub sheets: Vec<Vec<RawCell>>,
    /// Sheets that could not be read.
    pub errors: Vec<SheetError>,
    /// Cells counted against the budget.
    pub parsed: u64,
}

fn read_sheet(
    pkg: &mut Package,
    ctx: &Context<'_>,
    entry: &package::SheetEntry,
    budget: &mut Budget,
) -> Result<Vec<RawCell>, XlsxError> {
    let title = sheet_title(&entry.name)?;
    let rels = rels_path(&entry.part);
    let rels = if pkg.has(&rels) {
        read_rels(pkg, &rels)?
    } else {
        Vec::new()
    };
    let mut sheet = Sheet::new(ctx);
    sheet.parse(pkg.open_part(&entry.part)?, budget)?;
    sheet.bind_hyperlinks(&rels)?;
    for rel in rels.iter().filter(|r| r.kind == COMMENTS_REL) {
        sheet.add_comments(&pkg.tree(&rel.target)?)?;
    }
    let name = if entry.hidden {
        format!("{title}[HIDDEN]")
    } else {
        title
    };
    sheet.into_cells(&name)
}

/// Read every worksheet of an `.xlsx`/`.xlsm` file into doormat-core cells.
///
/// A sheet that cannot be read is listed in [`Extracted::errors`] and the
/// others are read. The file as a whole fails when its package structure
/// cannot be read, or when more than `max_cells` cells are counted
/// (`CellLimitError`).
pub fn read_xlsx(path: &Path, max_cells: Option<u64>) -> Result<Extracted, XlsxError> {
    let filename = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut pkg = Package::open(path)?;
    let manifest = read_manifest(&mut pkg)?;
    let strings = match &manifest.shared_strings {
        Some(part) => read_shared_strings(pkg.open_part(part)?)?,
        None => Vec::new(),
    };
    let workbook = read_workbook(&mut pkg, &manifest.workbook)?;
    let styles = if pkg.has("xl/styles.xml") {
        Styles::from_tree(&pkg.tree("xl/styles.xml")?)
    } else {
        Styles::default()
    };
    let entries = resolve_sheets(&mut pkg, &manifest.workbook, &workbook.sheets)?;
    let ctx = Context {
        strings: &strings,
        styles: &styles,
        date1904: workbook.date1904,
    };
    let mut budget = Budget::new(max_cells, &filename);
    let mut extracted = Extracted {
        sheets: Vec::new(),
        errors: Vec::new(),
        parsed: 0,
    };
    for entry in &entries {
        match read_sheet(&mut pkg, &ctx, entry, &mut budget) {
            Ok(cells) => extracted.sheets.push(cells),
            Err(error) if error.kind == "CellLimitError" => return Err(error),
            Err(error) => extracted.errors.push(SheetError {
                sheet: entry.name.clone(),
                error,
            }),
        }
    }
    extracted.parsed = budget.used;
    Ok(extracted)
}

/// A workbook read and run through the detection pipeline in one call.
#[derive(Debug, Clone)]
pub struct Processed {
    /// Every finding, in sheet order.
    pub findings: Vec<Finding>,
    /// Number of sheets read.
    pub sheet_count: usize,
    /// Number of cells read.
    pub cell_count: usize,
    /// Sheets that could not be read.
    pub errors: Vec<SheetError>,
}

/// Read an `.xlsx`/`.xlsm` file and run `doormat_core::pipeline::process_workbook` on it.
pub fn process_xlsx(path: &Path, max_cells: Option<u64>) -> Result<Processed, XlsxError> {
    let extracted = read_xlsx(path, max_cells)?;
    let sheet_count = extracted.sheets.len();
    let cell_count = extracted.sheets.iter().map(Vec::len).sum();
    let findings = doormat_core::pipeline::process_workbook(extracted.sheets);
    Ok(Processed {
        findings,
        sheet_count,
        cell_count,
        errors: extracted.errors,
    })
}
