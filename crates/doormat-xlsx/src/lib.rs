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

#[cfg(test)]
pub(crate) mod testutil {
    use std::io::Write;
    use std::path::PathBuf;

    use zip::write::SimpleFileOptions;

    pub const MAIN: &str = "http://schemas.openxmlformats.org/spreadsheetml/2006/main";
    pub const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
    const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
    const XLSX: &str = "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml";
    const SST: &str =
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";

    /// A directory for test files under the build's `target`, not the shared temp directory.
    pub fn scratch_dir() -> PathBuf {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/doormat-xlsx-tests");
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write a zip holding `parts` (stored, uncompressed) under a fresh temp directory.
    pub fn zip_file(name: &str, parts: &[(&str, String)]) -> PathBuf {
        let dir = scratch_dir().join(format!("{}-{name}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("book.xlsx");
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
        let options =
            SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        for (part, data) in parts {
            zip.start_file(*part, options).unwrap();
            zip.write_all(data.as_bytes()).unwrap();
        }
        zip.finish().unwrap();
        path
    }

    /// A worksheet part holding `rows` (the XML inside `<sheetData>`) and `tail` after it.
    pub fn worksheet(rows: &str, tail: &str) -> String {
        format!(
            r#"<worksheet xmlns="{MAIN}" xmlns:r="{REL}"><sheetData>{rows}</sheetData>{tail}</worksheet>"#
        )
    }

    /// A workbook listing `sheets` as (name, state attribute, worksheet XML), plus `extra` parts.
    pub fn workbook(
        name: &str,
        sheets: &[(&str, &str, String)],
        extra: &[(&str, String)],
    ) -> PathBuf {
        let listed: String = sheets
            .iter()
            .enumerate()
            .map(|(i, (n, state, _))| {
                format!(
                    r#"<sheet name="{n}" sheetId="{}" {state} r:id="rId{}"/>"#,
                    i + 1,
                    i + 1
                )
            })
            .collect();
        let rels: String = (1..=sheets.len())
            .map(|i| format!(r#"<Relationship Id="rId{i}" Type="{REL}/worksheet" Target="worksheets/sheet{i}.xml"/>"#))
            .collect();
        let sst = if extra.iter().any(|(p, _)| *p == "xl/sharedStrings.xml") {
            format!(r#"<Override PartName="/xl/sharedStrings.xml" ContentType="{SST}"/>"#)
        } else {
            String::new()
        };
        let mut parts = vec![
            (
                "[Content_Types].xml",
                format!(
                    r#"<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Override PartName="/xl/workbook.xml" ContentType="{XLSX}"/>{sst}</Types>"#
                ),
            ),
            (
                "xl/workbook.xml",
                format!(
                    r#"<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets>{listed}</sheets></workbook>"#
                ),
            ),
            (
                "xl/_rels/workbook.xml.rels",
                format!(r#"<Relationships xmlns="{PKG_REL}">{rels}</Relationships>"#),
            ),
        ];
        let names: Vec<String> = (1..=sheets.len())
            .map(|i| format!("xl/worksheets/sheet{i}.xml"))
            .collect();
        for (part, (_, _, xml)) in names.iter().zip(sheets) {
            parts.push((part.as_str(), xml.clone()));
        }
        parts.extend(extra.iter().map(|(p, d)| (*p, d.clone())));
        zip_file(name, &parts)
    }

    /// A relationships part for a worksheet's comments.
    pub fn comments_rels() -> String {
        format!(
            r#"<Relationships xmlns="{PKG_REL}"><Relationship Id="rId1" Type="{REL}/comments" Target="../comments1.xml"/></Relationships>"#
        )
    }
}

#[cfg(test)]
mod tests {
    use super::testutil::{comments_rels, workbook, worksheet};
    use super::*;

    fn cell(r: &str, text: &str) -> String {
        format!(r#"<c r="{r}" t="inlineStr"><is><t>{text}</t></is></c>"#)
    }

    #[test]
    fn reads_sheets_strings_comments_and_errors() {
        let first = worksheet(
            &format!(
                r#"<row r="1">{}<c r="B1" t="s"><v>0</v></c></row>"#,
                cell("A1", "Password")
            ),
            "",
        );
        let hidden = worksheet(&format!(r#"<row r="2">{}</row>"#, cell("C2", "x")), "");
        let broken = worksheet(r#"<row r="1"><c r="A1"><v>not a number</v></c></row>"#, "");
        let path = workbook(
            "read",
            &[("Main", "", first), ("Secret", r#"state="hidden""#, hidden), ("Bad", "", broken)],
            &[
                ("xl/sharedStrings.xml", format!(r#"<sst xmlns="{}"><si><t>s3cRet!99</t></si></sst>"#, testutil::MAIN)),
                ("xl/worksheets/_rels/sheet1.xml.rels", comments_rels()),
                (
                    "xl/comments1.xml",
                    r#"<comments><authors><author>a</author></authors><commentList><comment ref="A1" authorId="0"><text><t>note</t></text></comment></commentList></comments>"#.into(),
                ),
            ],
        );
        let got = read_xlsx(&path, None).unwrap();
        let cells: Vec<_> = got
            .sheets
            .iter()
            .flatten()
            .map(|c| {
                (
                    c.sheet_name.as_str(),
                    c.row,
                    c.col,
                    c.value.as_str(),
                    c.comment.as_str(),
                )
            })
            .collect();
        assert_eq!(
            cells,
            vec![
                ("Main", 1, 1, "Password", "note"),
                ("Main", 1, 2, "s3cRet!99", ""),
                ("Secret[HIDDEN]", 2, 3, "x", "")
            ]
        );
        assert_eq!(got.errors.len(), 1);
        assert_eq!(
            (
                got.errors[0].sheet.as_str(),
                got.errors[0].error.kind.as_str()
            ),
            ("Bad", "ValueError")
        );
        // The bad sheet's row fails while its cells are parsed, before they are counted.
        assert_eq!(got.parsed, 3);
    }

    #[test]
    fn cell_limit_refuses_the_file() {
        let rows: String = (1..=5)
            .map(|r| format!(r#"<row r="{r}">{}</row>"#, cell(&format!("A{r}"), "v")))
            .collect();
        let path = workbook("limit", &[("S", "", worksheet(&rows, ""))], &[]);
        let err = read_xlsx(&path, Some(3)).unwrap_err();
        assert_eq!(err.kind, "CellLimitError");
        assert!(read_xlsx(&path, Some(5)).is_ok());
    }

    #[test]
    fn process_finds_a_credential() {
        let rows = format!(
            r#"<row r="1">{}{}</row>"#,
            cell("A1", "Password"),
            cell("B1", "s3cRet!99")
        );
        let path = workbook("process", &[("S", "", worksheet(&rows, ""))], &[]);
        let done = process_xlsx(&path, None).unwrap();
        assert_eq!((done.sheet_count, done.cell_count), (1, 2));
        assert!(done
            .findings
            .iter()
            .any(|f| f.value == "s3cRet!99" && f.value_col == 2));
        assert_eq!(XlsxError::new("KeyError", "x").kind, "KeyError");
    }
}
