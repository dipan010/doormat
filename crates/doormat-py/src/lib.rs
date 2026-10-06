//! PyO3 bindings for doormat-core.
//!
//! This crate provides Python-callable wrappers around the doormat detection
//! pipeline. It handles type conversion between Python tuples/dicts and Rust
//! types — no detection logic lives here.

// PyO3 0.22 proc macros generate code that triggers this clippy lint
#![allow(clippy::useless_conversion)]

use std::panic::{self, AssertUnwindSafe};

use pyo3::exceptions::PyRuntimeError;
use pyo3::prelude::*;
use pyo3::types::PyDict;

use doormat_core::pipeline;
use doormat_core::store::RawCell;
use doormat_core::types::Relationship;

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

/// Convert a Rust Relationship into a Python dict.
fn relationship_to_dict(py: Python<'_>, rel: &Relationship) -> PyResult<PyObject> {
    let dict = PyDict::new_bound(py);
    dict.set_item("header_cell_id", rel.header_cell_id)?;
    dict.set_item("value_cell_id", rel.value_cell_id)?;
    dict.set_item("key", &rel.key)?;
    dict.set_item("value", &rel.value)?;
    dict.set_item("confidence", rel.confidence)?;
    dict.set_item("reason", &rel.reason)?;
    Ok(dict.into())
}

/// Convert Vec<Relationship> to a Python list of dicts.
fn relationships_to_py(py: Python<'_>, rels: Vec<Relationship>) -> PyResult<Vec<PyObject>> {
    rels.iter()
        .map(|rel| relationship_to_dict(py, rel))
        .collect()
}

/// Returns the doormat-core version string.
#[pyfunction]
fn version() -> &'static str {
    doormat_core::version()
}

/// Run the full detection pipeline on a single sheet's cells.
///
/// Args:
///     cells: list of tuples (row, col, value, formula, comment, sheet_name, is_merged_origin)
///
/// Returns:
///     list of dicts with keys: header_cell_id, value_cell_id, key, value, confidence, reason
#[pyfunction]
fn process_sheet(py: Python<'_>, cells: Vec<Bound<'_, pyo3::types::PyTuple>>) -> PyResult<Vec<PyObject>> {
    let raw_cells: Vec<RawCell> = cells
        .iter()
        .map(tuple_to_raw_cell)
        .collect::<PyResult<Vec<_>>>()?;

    let results = panic::catch_unwind(AssertUnwindSafe(|| pipeline::process_sheet(raw_cells)))
        .map_err(|_| PyRuntimeError::new_err("doormat-core panicked during process_sheet"))?;

    relationships_to_py(py, results)
}

/// Run the full detection pipeline on multiple sheets in parallel.
///
/// Args:
///     sheets: list of lists of tuples (row, col, value, formula, comment, sheet_name, is_merged_origin)
///
/// Returns:
///     list of dicts with keys: header_cell_id, value_cell_id, key, value, confidence, reason
#[pyfunction]
fn process_workbook(py: Python<'_>, sheets: Vec<Vec<Bound<'_, pyo3::types::PyTuple>>>) -> PyResult<Vec<PyObject>> {
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

    relationships_to_py(py, results)
}

/// The native extension module.
#[pymodule]
fn _core(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(version, m)?)?;
    m.add_function(wrap_pyfunction!(process_sheet, m)?)?;
    m.add_function(wrap_pyfunction!(process_workbook, m)?)?;
    Ok(())
}
