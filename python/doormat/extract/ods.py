"""Extract raw cell tuples from .ods (OpenDocument Spreadsheet) files via odfpy.

odfpy >= 1.4 is an optional dependency. If not installed, a clear
ImportError is raised when this extractor is called.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from doormat._limits import CellBudget, sheet_failed
from doormat._types import SHEET_ERRORS, Sheet, SheetErrorHandler

# LibreOffice's column limit; cells beyond it are ignored.
MAX_COLUMNS = 16_384
# A non-empty row repeated more often than this is emitted this many times.
MAX_ROW_REPEAT = 1_000


def extract_ods(
    filepath: str | Path,
    on_sheet_error: SheetErrorHandler | None = None,
    budget: CellBudget | None = None,
) -> list[Sheet]:
    """Extract cells from an .ods file as raw cell tuples.

    Requires ``odfpy >= 1.4``. Installs via ``pip install doormat[ods]``.

    Args:
        filepath: Path to the .ods file.
        on_sheet_error: Called with the sheet name and error when a sheet
            cannot be read; extraction continues with the next sheet. If
            None, the error is raised. All sheets share one XML document, so
            a malformed document still fails the whole file.
        budget: Counts cells as they are read and raises CellLimitError
            (a ValueError) past its limit. If None, nothing is counted.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet that
        was read.
        Each tuple is
        ``(row, col, value, formula, comment, sheet_name, is_merged_origin)``.

    Raises:
        ImportError: If odfpy is not installed.
    """
    try:
        from odf import table as odf_table
        from odf.opendocument import load as odf_load
    except ImportError:
        raise ImportError(
            "odfpy is required for .ods support. "
            "Install it with: pip install doormat[ods]"
        ) from None

    filepath = Path(filepath)
    doc = odf_load(str(filepath))

    sheets: list[Sheet] = []

    for tbl in doc.spreadsheet.getElementsByType(odf_table.Table):
        sheet_name = tbl.getAttribute("name") or "Sheet"
        try:
            sheets.append(_extract_table(doc, tbl, sheet_name, budget))
        except SHEET_ERRORS as exc:
            sheet_failed(on_sheet_error, sheet_name, exc)

    return sheets


def _extract_table(doc: Any, tbl: Any, sheet_name: str, budget: CellBudget | None = None) -> Sheet:
    """Extract one ODF table as raw cell tuples."""
    from odf import table as odf_table

    if _is_hidden(doc, tbl):
        sheet_name = f"{sheet_name}[HIDDEN]"

    cells: Sheet = []
    row_idx = 0
    for row in tbl.getElementsByType(odf_table.TableRow):
        row_repeat_count = _repeat(row, "numberrowsrepeated")
        row_cells = _row_cells(row)
        # Emit the row once per repetition (capped), but always advance the row
        # counter by the full repeat count so later rows keep their real numbers.
        if row_cells:
            emitted = min(row_repeat_count, MAX_ROW_REPEAT)
            if budget is not None:
                budget.take(len(row_cells) * emitted)
            for offset in range(emitted):
                r = row_idx + offset + 1
                cells.extend(
                    (r, c, value, formula, comment, sheet_name, merged)
                    for c, value, formula, merged, comment in row_cells
                )
        row_idx += row_repeat_count
    return cells


def _is_hidden(doc: Any, tbl: Any) -> bool:
    """True when the table's automatic style sets ``display="false"``."""
    table_style = tbl.getAttribute("stylename")
    if not table_style:
        return False
    for auto_style in doc.automaticstyles.childNodes:
        if getattr(auto_style, "getAttribute", lambda _: None)("name") != table_style:
            continue
        if any(getattr(prop, "getAttribute", lambda _: None)("display") == "false"
               for prop in auto_style.childNodes):
            return True
    return False


def _repeat(element: Any, attribute: str) -> int:
    """The element's repeat count (``numberrowsrepeated`` or ``numbercolumnsrepeated``)."""
    value = element.getAttribute(attribute)
    return int(value) if value else 1


def _row_cells(row: Any) -> list[tuple[int, str, str, bool, str]]:
    """Non-empty cells of one row as (col, value, formula, merged_origin, comment).

    Repeated empty cells only advance the column counter, so a cell that claims
    to repeat a billion times costs nothing. Columns stop at ``MAX_COLUMNS``.
    """
    row_cells: list[tuple[int, str, str, bool, str]] = []
    col_idx = 0
    for cell in row.childNodes:
        if cell.qname[1] not in ("table-cell", "covered-table-cell"):
            continue
        if col_idx >= MAX_COLUMNS:
            break
        col_repeat_count = _repeat(cell, "numbercolumnsrepeated")
        text, formula, comment = _cell_text(cell), cell.getAttribute("formula") or "", _annotation(cell)
        merged = _is_merged(cell)
        if text or formula or comment:
            # Only the first cell in a repeated run is the merge origin
            row_cells.extend(
                (col_idx + offset + 1, text, formula, merged and offset == 0, comment)
                for offset in range(min(col_repeat_count, MAX_COLUMNS - col_idx))
            )
        col_idx += col_repeat_count
    return row_cells


def _paragraphs(element: Any) -> str:
    """The text of an element's ``text:p`` paragraphs, joined with newlines."""
    from odf import text as odf_text

    return "\n".join(t for t in (_get_text(p) for p in element.getElementsByType(odf_text.P)) if t)


def _cell_text(cell: Any) -> str:
    """A cell's displayed text."""
    return _paragraphs(cell)


def _annotation(cell: Any) -> str:
    """The text of a cell's annotations (comments)."""
    from odf import office as odf_office

    return "\n".join(t for t in (_paragraphs(a) for a in cell.getElementsByType(odf_office.Annotation)) if t)


def _is_merged(cell: Any) -> bool:
    """True when the cell spans more than one column or row.

    A covered cell (the hidden part of a merged range) is never the origin, and
    odfpy rejects span attributes on it.
    """
    if cell.qname[1] == "covered-table-cell":
        return False
    spans = (cell.getAttribute("numbercolumnsspanned"), cell.getAttribute("numberrowsspanned"))
    return any(span is not None and int(span) > 1 for span in spans)


def _get_text(element: Any) -> str:
    """Recursively extract text from an ODF element and its children."""
    parts: list[str] = []
    if hasattr(element, "data"):
        parts.append(element.data)
    if hasattr(element, "childNodes"):
        parts.extend(_get_text(child) for child in element.childNodes)
    return "".join(parts)
