//! PyO3 bindings for doormat-core.
//!
//! This crate provides Python-callable wrappers around the doormat detection
//! pipeline. It handles type conversion between Python tuples/dicts and Rust
//! types; no detection logic lives here.

use std::panic::{self, AssertUnwindSafe};
use std::path::PathBuf;

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};

use doormat_core::pipeline;
use doormat_core::store::RawCell;
use doormat_core::types::Finding;
use doormat_xlsx::{SheetError, XlsxError};

/// Convert a Python tuple (row, col, value, formula, comment, sheet_name, is_merged_origin)
/// into a Rust RawCell.
fn tuple_to_raw_cell(tuple: &Bound<'_, pyo3::types::PyTuple>) -> PyResult<RawCell> {
    Ok(RawCell {
        row: tuple.get_item(0)?.extract()?,
        col: tuple.get_item(1)?.extract()?,
        value: tuple.get_item(2)?.extract()?,
        formula: tuple.get_item(3)?.extract()?,
        comment: tuple.get_item(4)?.extract()?,
        sheet_name: tuple.get_item(5)?.extract()?,
        is_merged_origin: tuple.get_item(6)?.extract()?,
    })
}

/// Convert a Rust Finding into a Python dict.
fn finding_to_dict(py: Python<'_>, finding: &Finding) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    dict.set_item("sheet", &finding.sheet)?;
    dict.set_item("row", finding.value_row)?;
    dict.set_item("col", finding.value_col)?;
    dict.set_item("header_row", finding.header_row)?;
    dict.set_item("header_col", finding.header_col)?;
    dict.set_item("key", &finding.key)?;
    dict.set_item("value", &finding.value)?;
    dict.set_item("confidence", finding.confidence)?;
    dict.set_item("reason", &finding.reason)?;
    Ok(dict.into_any().unbind())
}

/// Convert Vec<Finding> to a Python list of dicts.
fn findings_to_py(py: Python<'_>, findings: Vec<Finding>) -> PyResult<Vec<Py<PyAny>>> {
    findings
        .iter()
        .map(|finding| finding_to_dict(py, finding))
        .collect()
}

/// Returns the doormat-core version string.
#[pyfunction]
fn version() -> &'static str {
    doormat_core::version()
}

/// Run the full detection pipeline on multiple sheets in parallel.
///
/// Args:
///     sheets: list of lists of tuples (row, col, value, formula, comment, sheet_name, is_merged_origin)
///
/// Returns:
///     list of dicts with keys: sheet, row, col, header_row, header_col, key, value,
///     confidence, reason. Rows and columns are as supplied in the input tuples.
#[pyfunction]
fn process_workbook(
    py: Python<'_>,
    sheets: Vec<Vec<Bound<'_, pyo3::types::PyTuple>>>,
) -> PyResult<Vec<Py<PyAny>>> {
    let raw_sheets: Vec<Vec<RawCell>> = sheets
        .iter()
        .map(|sheet| {
            sheet
                .iter()
                .map(tuple_to_raw_cell)
                .collect::<PyResult<Vec<_>>>()
        })
        .collect::<PyResult<Vec<_>>>()?;

    let results = panic::catch_unwind(AssertUnwindSafe(|| pipeline::process_workbook(raw_sheets)))
        .map_err(|_| PyRuntimeError::new_err("doormat-core panicked during process_workbook"))?;

    findings_to_py(py, results)
}

/// Convert a Rust RawCell into the Python tuple the extractors produce.
fn raw_cell_to_tuple<'py>(py: Python<'py>, cell: &RawCell) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(
        py,
        [
            cell.row.into_pyobject(py)?.into_any(),
            cell.col.into_pyobject(py)?.into_any(),
            cell.value.as_str().into_pyobject(py)?.into_any(),
            cell.formula.as_str().into_pyobject(py)?.into_any(),
            cell.comment.as_str().into_pyobject(py)?.into_any(),
            cell.sheet_name.as_str().into_pyobject(py)?.into_any(),
            pyo3::types::PyBool::new(py, cell.is_merged_origin)
                .to_owned()
                .into_any(),
        ],
    )
}

/// Convert sheet errors to (sheet, kind, message) tuples.
fn sheet_errors(errors: &[SheetError]) -> Vec<(String, String, String)> {
    errors
        .iter()
        .map(|e| {
            (
                e.sheet.clone(),
                e.error.kind.clone(),
                e.error.message.clone(),
            )
        })
        .collect()
}

/// Convert a file-level read error to a (kind, message) tuple.
fn file_error(error: XlsxError) -> (String, String) {
    (error.kind, error.message)
}

/// Read an .xlsx/.xlsm file and run the detection pipeline on it, in one call.
///
/// Args:
///     path: the workbook.
///     max_cells: refuse the file past this many cells (None: no limit).
///
/// Returns:
///     (findings, sheet_count, cell_count, sheet_errors, file_error). findings
///     are dicts as from process_workbook; sheet_errors are (sheet, kind,
///     message); file_error is None, or (kind, message) when the file could
///     not be read, in which case the other fields are empty.
#[pyfunction]
#[allow(clippy::type_complexity)]
fn process_xlsx(
    py: Python<'_>,
    path: PathBuf,
    max_cells: Option<u64>,
) -> PyResult<(
    Vec<Py<PyAny>>,
    usize,
    usize,
    Vec<(String, String, String)>,
    Option<(String, String)>,
)> {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        doormat_xlsx::process_xlsx(&path, max_cells)
    }))
    .map_err(|_| PyRuntimeError::new_err("doormat panicked while reading the workbook"))?;
    match result {
        Ok(done) => Ok((
            findings_to_py(py, done.findings)?,
            done.sheet_count,
            done.cell_count,
            sheet_errors(&done.errors),
            None,
        )),
        Err(error) => Ok((Vec::new(), 0, 0, Vec::new(), Some(file_error(error)))),
    }
}

/// Read an .xlsx/.xlsm file into cell tuples without running detection.
///
/// For the extraction equivalence check and tests; load() uses process_xlsx.
///
/// Returns:
///     (sheets, sheet_errors, parsed, file_error): sheets as lists of
///     (row, col, value, formula, comment, sheet_name, is_merged_origin),
///     sheet_errors as (sheet, kind, message), parsed the cells counted
///     against max_cells, file_error None or (kind, message).
#[pyfunction]
#[allow(clippy::type_complexity)]
fn _read_xlsx<'py>(
    py: Python<'py>,
    path: PathBuf,
    max_cells: Option<u64>,
) -> PyResult<(
    Vec<Vec<Bound<'py, PyTuple>>>,
    Vec<(String, String, String)>,
    u64,
    Option<(String, String)>,
)> {
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        doormat_xlsx::read_xlsx(&path, max_cells)
    }))
    .map_err(|_| PyRuntimeError::new_err("doormat panicked while reading the workbook"))?;
    match result {
        Ok(extracted) => {
            let sheets = extracted
                .sheets
                .iter()
                .map(|sheet| {
                    sheet
                        .iter()
                        .map(|c| raw_cell_to_tuple(py, c))
                        .collect::<PyResult<Vec<_>>>()
                })
                .collect::<PyResult<Vec<_>>>()?;
            Ok((
                sheets,
                sheet_errors(&extracted.errors),
                extracted.parsed,
                None,
            ))
        }
        Err(error) => Ok((Vec::new(), Vec::new(), 0, Some(file_error(error)))),
    }
}

/// The native extension module.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(process_workbook, m)?)?;
    m.add_function(wrap_pyfunction!(process_xlsx, m)?)?;
    m.add_function(wrap_pyfunction!(_read_xlsx, m)?)?;
    Ok(())
}
