//! The workbook package: the zip archive, its content types, relationships,
//! and the list of worksheets, resolved the way openpyxl 3.1 resolves them.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

use zip::ZipArchive;

use crate::xml::{parse_tree, Element, REL_NS};
use crate::XlsxError;

const SHARED_STRINGS: &str =
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml";
/// Workbook content types in the order openpyxl looks for them.
const WORKBOOK_TYPES: [&str; 4] = [
    "application/vnd.ms-excel.template.macroEnabled.main+xml",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.template.main+xml",
    "application/vnd.ms-excel.sheet.macroEnabled.main+xml",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml",
];
/// Relationship type of a worksheet's comments part.
pub const COMMENTS_REL: &str =
    "http://schemas.openxmlformats.org/officeDocument/2006/relationships/comments";

/// A zip archive whose parts are read no further than their declared size.
pub struct Package {
    archive: ZipArchive<File>,
    names: HashSet<String>,
}

/// A part's bytes stop at its declared size: a part that holds more fails,
/// as Python's `zipfile` does (design decision 0013).
struct Bounded<R> {
    inner: R,
    left: u64,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n as u64 > self.left {
            return Err(std::io::Error::other(
                "part is larger than its declared size",
            ));
        }
        self.left -= n as u64;
        Ok(n)
    }
}

fn bad_zip(e: impl std::fmt::Display) -> XlsxError {
    XlsxError::new("BadZipFile", e.to_string())
}

impl Package {
    /// Open the archive.
    pub fn open(path: &Path) -> Result<Self, XlsxError> {
        let file =
            File::open(path).map_err(|e| XlsxError::new("FileNotFoundError", e.to_string()))?;
        let archive = ZipArchive::new(file).map_err(bad_zip)?;
        let names = archive
            .file_names()
            .map(|name| name.map(|n| n.into_owned()))
            .collect::<Result<HashSet<_>, _>>()
            .map_err(bad_zip)?;
        Ok(Package { archive, names })
    }

    /// True if the archive has a part with this exact name.
    pub fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    /// Stream one part; `KeyError` if it does not exist.
    pub fn open_part(&mut self, name: &str) -> Result<impl std::io::BufRead + '_, XlsxError> {
        let entry = self.archive.by_name(name).map_err(|_| {
            XlsxError::new(
                "KeyError",
                format!("There is no item named '{name}' in the archive"),
            )
        })?;
        let left = entry.size();
        Ok(BufReader::with_capacity(
            1 << 16,
            Bounded { inner: entry, left },
        ))
    }

    /// Read one part completely; `KeyError` if it does not exist.
    pub fn read(&mut self, name: &str) -> Result<Vec<u8>, XlsxError> {
        let mut data = Vec::new();
        self.open_part(name)?
            .read_to_end(&mut data)
            .map_err(bad_zip)?;
        Ok(data)
    }

    /// Read and parse one small XML part.
    pub fn tree(&mut self, name: &str) -> Result<Element, XlsxError> {
        parse_tree(&self.read(name)?)
    }
}

/// One `<Relationship>` with its target resolved to a part name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    /// `Id` attribute.
    pub id: Option<String>,
    /// `Type` attribute.
    pub kind: String,
    /// Target part, resolved unless the target mode is `External`.
    pub target: String,
}

/// openpyxl `get_rels_path`: `dir/name` -> `dir/_rels/name.rels`.
pub fn rels_path(part: &str) -> String {
    match part.rsplit_once('/') {
        Some((dir, name)) => format!("{dir}/_rels/{name}.rels"),
        None => format!("_rels/{part}.rels"),
    }
}

/// `posixpath.normpath`.
fn normpath(path: &str) -> String {
    if path.is_empty() {
        return ".".into();
    }
    let absolute = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            ".." if absolute => {}
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    let initial = if !absolute {
        ""
    } else if path.starts_with("//") && !path.starts_with("///") {
        "//"
    } else {
        "/"
    };
    let out = format!("{initial}{joined}");
    if out.is_empty() {
        ".".into()
    } else {
        out
    }
}

/// `posixpath.dirname`.
fn dirname(path: &str) -> &str {
    match path.rfind('/') {
        Some(i) => {
            let head = &path[..=i];
            let trimmed = head.trim_end_matches('/');
            if trimmed.is_empty() {
                head
            } else {
                trimmed
            }
        }
        None => "",
    }
}

/// `posixpath.join(a, b)`.
fn join(a: &str, b: &str) -> String {
    if b.starts_with('/') || a.is_empty() {
        b.to_string()
    } else if a.ends_with('/') {
        format!("{a}{b}")
    } else {
        format!("{a}/{b}")
    }
}

/// openpyxl `get_dependents`: read a `.rels` part and resolve each target
/// against the folder above the `_rels` folder.
pub fn read_rels(pkg: &mut Package, rels: &str) -> Result<Vec<Relationship>, XlsxError> {
    let root = pkg.tree(rels)?;
    let parent = dirname(dirname(rels));
    Ok(root
        .children_named("Relationship")
        .map(|r| {
            let target = r.get("Target").unwrap_or("").to_string();
            let target = if r.get("TargetMode") == Some("External") {
                target
            } else if let Some(stripped) = target.strip_prefix('/') {
                stripped.to_string()
            } else {
                normpath(&join(parent, &target))
            };
            Relationship {
                id: r.get("Id").map(str::to_string),
                kind: r.get("Type").unwrap_or("").to_string(),
                target,
            }
        })
        .collect())
}

/// What the content-types manifest names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The shared strings part, if any.
    pub shared_strings: Option<String>,
    /// The workbook part.
    pub workbook: String,
}

/// Read `[Content_Types].xml`: the shared strings part and the workbook part.
pub fn read_manifest(pkg: &mut Package) -> Result<Manifest, XlsxError> {
    let manifest = pkg.tree("[Content_Types].xml")?;
    let overrides: Vec<(String, String)> = manifest
        .children_named("Override")
        .map(|o| {
            (
                o.get("PartName").unwrap_or("").to_string(),
                o.get("ContentType").unwrap_or("").to_string(),
            )
        })
        .collect();
    let find = |content_type: &str| -> Option<String> {
        overrides
            .iter()
            .find(|(_, ct)| ct == content_type)
            .map(|(part, _)| part.chars().skip(1).collect())
    };
    let workbook = match WORKBOOK_TYPES.iter().find_map(|ct| find(ct)) {
        Some(part) => part,
        None => {
            let defaults: Vec<&str> = manifest
                .children_named("Default")
                .filter_map(|d| d.get("ContentType"))
                .collect();
            if WORKBOOK_TYPES.iter().any(|ct| defaults.contains(ct)) {
                "xl/workbook.xml".to_string()
            } else {
                return Err(XlsxError::new(
                    "OSError",
                    "File contains no valid workbook part",
                ));
            }
        }
    };
    Ok(Manifest {
        shared_strings: find(SHARED_STRINGS),
        workbook,
    })
}

/// A `<sheet>` of the workbook.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedSheet {
    /// The `name` attribute.
    pub name: String,
    /// True for `hidden` and `veryHidden` sheets.
    pub hidden: bool,
    /// The `r:id` attribute; sheets without one are skipped.
    pub id: Option<String>,
}

/// The workbook part: the date epoch and the sheets it lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkbookPart {
    /// True if dates count from 1904.
    pub date1904: bool,
    /// The sheets, in order.
    pub sheets: Vec<ListedSheet>,
}

/// openpyxl's `Bool` descriptor on a string attribute.
fn bool_attr(value: Option<&str>) -> bool {
    value.is_some_and(|v| !matches!(v, "" | "false" | "f" | "0"))
}

/// Read the workbook part. An invalid sheet `state` refuses the file, as
/// openpyxl's descriptors do.
pub fn read_workbook(pkg: &mut Package, part: &str) -> Result<WorkbookPart, XlsxError> {
    let root = pkg.tree(part)?;
    let date1904 = bool_attr(
        root.last_named("workbookPr")
            .and_then(|p| p.get("date1904")),
    );
    let mut sheets = Vec::new();
    for sheet in root
        .last_named("sheets")
        .map(|s| s.children.as_slice())
        .unwrap_or_default()
    {
        let state = sheet.get("state");
        if !matches!(
            state,
            None | Some("visible" | "hidden" | "veryHidden" | "none")
        ) {
            return Err(XlsxError::new("ValueError", "invalid sheet state"));
        }
        sheets.push(ListedSheet {
            name: sheet.get("name").unwrap_or("").to_string(),
            hidden: matches!(state, Some("hidden" | "veryHidden")),
            id: sheet
                .get_ns(REL_NS, "id")
                .filter(|id| !id.is_empty())
                .map(str::to_string),
        });
    }
    Ok(WorkbookPart { date1904, sheets })
}

/// A worksheet to read: its listed name, visibility and part.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetEntry {
    /// The `name` attribute.
    pub name: String,
    /// True for `hidden` and `veryHidden` sheets.
    pub hidden: bool,
    /// The worksheet part.
    pub part: String,
}

/// Resolve each listed sheet's part through the workbook relationships.
/// Chartsheets and parts missing from the archive are skipped; an `r:id`
/// with no relationship refuses the file (`KeyError`).
pub fn resolve_sheets(
    pkg: &mut Package,
    workbook_part: &str,
    listed: &[ListedSheet],
) -> Result<Vec<SheetEntry>, XlsxError> {
    let mut rels: Option<HashMap<String, Relationship>> = None;
    let mut entries = Vec::new();
    for sheet in listed {
        let Some(id) = &sheet.id else { continue };
        if rels.is_none() {
            let list = read_rels(pkg, &rels_path(workbook_part))?;
            rels = Some(
                list.into_iter()
                    .filter_map(|r| Some((r.id.clone()?, r)))
                    .collect(),
            );
        }
        let rel = rels
            .as_ref()
            .and_then(|map| map.get(id))
            .ok_or_else(|| XlsxError::new("KeyError", format!("'{id}'")))?;
        if pkg.has(&rel.target) && !rel.kind.contains("chartsheet") {
            entries.push(SheetEntry {
                name: sheet.name.clone(),
                hidden: sheet.hidden,
                part: rel.target.clone(),
            });
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_like_posixpath() {
        assert_eq!(rels_path("xl/workbook.xml"), "xl/_rels/workbook.xml.rels");
        assert_eq!(rels_path("workbook.xml"), "_rels/workbook.xml.rels");
        assert_eq!(dirname(dirname("xl/_rels/workbook.xml.rels")), "xl");
        assert_eq!(
            dirname(dirname("xl/worksheets/_rels/sheet1.xml.rels")),
            "xl/worksheets"
        );
        assert_eq!(
            normpath(&join("xl/worksheets", "../comments1.xml")),
            "xl/comments1.xml"
        );
        assert_eq!(normpath(&join("", "../x.xml")), "../x.xml");
        assert_eq!(normpath("a//b/./c/"), "a/b/c");
        assert_eq!(normpath("/../a"), "/a");
        assert_eq!(normpath(""), ".");
        assert_eq!(join("a", "/b"), "/b");
    }

    #[test]
    fn booleans_like_openpyxl() {
        assert!(bool_attr(Some("1")));
        assert!(bool_attr(Some("true")));
        assert!(bool_attr(Some("False")));
        assert!(!bool_attr(Some("false")));
        assert!(!bool_attr(Some("0")));
        assert!(!bool_attr(Some("")));
        assert!(!bool_attr(None));
    }
}
