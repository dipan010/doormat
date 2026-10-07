//! PyO3 bindings for doormat-core.
//!
//! This crate provides Python-callable wrappers around the doormat detection
//! pipeline. It handles type conversion between Python tuples/dicts and Rust
//! types; no detection logic lives here.

use std::panic::{self, AssertUnwindSafe};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use doormat_core::pipeline;
use doormat_core::store::RawCell;
use doormat_core::types::Finding;

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

/// The native extension module.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(process_workbook, m)?)?;
    Ok(())
}
