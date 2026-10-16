"""Extract raw cell tuples from legacy .xls (BIFF) files via xlrd.

xlrd >= 2.0 is an optional dependency. If not installed, a clear
ImportError is raised when this extractor is called.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from doormat._limits import CellBudget, sheet_failed
from doormat._types import SHEET_ERRORS, Sheet, SheetErrorHandler


def extract_xls(
    filepath: str | Path,
    on_sheet_error: SheetErrorHandler | None = None,
    budget: CellBudget | None = None,
) -> list[Sheet]:
    """Extract cells from a legacy .xls file as raw cell tuples.

    Requires ``xlrd >= 2.0``. Installs via ``pip install doormat[xls]``.

    Args:
        filepath: Path to the .xls file.
        on_sheet_error: Called with the sheet name and error when a sheet
            cannot be read; extraction continues with the next sheet. If
            None, the error is raised.
        budget: Counts cells as they are read and raises CellLimitError
            (a ValueError) past its limit. If None, nothing is counted.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet that
        was read.
        Each tuple is
        ``(row, col, value, formula, comment, sheet_name, is_merged_origin)``.
        Formula and comment are always ``""`` (xlrd v2 does not expose
        these for .xls files).

    Raises:
        ImportError: If xlrd is not installed.
    """
    try:
        import xlrd
    except ImportError:
        raise ImportError(
            "xlrd is required for .xls support. "
            "Install it with: pip install doormat[xls]"
        ) from None

    filepath = Path(filepath)
    # on_demand parses each sheet only when it is requested, so one malformed
    # sheet does not stop the others from being read.
    book = xlrd.open_workbook(filepath, formatting_info=False, on_demand=True)
    sheet_errors = (*SHEET_ERRORS, xlrd.XLRDError)

    try:
        sheets: list[Sheet] = []
        for sheet_idx, name in enumerate(book.sheet_names()):
            try:
                sheets.append(_extract_sheet(book.sheet_by_index(sheet_idx), budget))
            except sheet_errors as exc:
                sheet_failed(on_sheet_error, name, exc)
    finally:
        book.release_resources()

    return sheets


def _extract_sheet(sheet: Any, budget: CellBudget | None = None) -> Sheet:
    """Extract one xlrd sheet as raw cell tuples."""
    import xlrd

    sheet_name = sheet.name

    # Hidden sheets: visibility 0=visible, 1=hidden, 2=very hidden
    if sheet.visibility in (1, 2):
        sheet_name = f"{sheet_name}[HIDDEN]"

    # Build set of merged-cell origins
    merge_origins: set[tuple[int, int]] = set()
    for rlo, _rhi, clo, _chi in sheet.merged_cells:
        # xlrd uses 0-based, convert to 1-based
        merge_origins.add((rlo + 1, clo + 1))

    cells: Sheet = []
    for row_idx in range(sheet.nrows):
        if budget is not None:
            budget.take(sheet.ncols)
        for col_idx in range(sheet.ncols):
            cell = sheet.cell(row_idx, col_idx)
            if cell.ctype == xlrd.XL_CELL_EMPTY:
                continue

            raw = cell.value
            if cell.ctype == xlrd.XL_CELL_NUMBER and float(raw).is_integer():
                # xlrd returns every number as a float; a numeric
                # password 12345678 must not be reported as "12345678.0"
                value = str(int(raw))
            else:
                value = str(raw) if raw != "" else ""
            if not value:
                continue

            # Convert to 1-based indices
            r = row_idx + 1
            c = col_idx + 1
            is_merged_origin = (r, c) in merge_origins

            cells.append(
                (r, c, value, "", "", sheet_name, is_merged_origin)
            )

    return cells
