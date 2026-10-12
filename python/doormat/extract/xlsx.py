"""Extract raw cell tuples from xlsx/xlsm files via openpyxl.

The workbook is opened exactly once, with data_only=False so formulas are kept.
Only cells present in the file are visited, never the declared sheet range.
"""

from __future__ import annotations

from pathlib import Path

import openpyxl
from openpyxl.worksheet.worksheet import Worksheet

from doormat._types import Sheet


def clean_comment(raw_text: str) -> str:
    """Strip whitespace and remove the 'Comment:\\n' prefix openpyxl adds.

    Args:
        raw_text: Raw comment text from openpyxl.

    Returns:
        Cleaned comment string with prefix and surrounding whitespace removed.
    """
    text = raw_text.strip()
    if text.startswith("Comment:\n"):
        text = text[len("Comment:\n"):]
    return text.strip()


def collect_merge_origins(worksheet: Worksheet) -> set[tuple[int, int]]:
    """Return the set of (min_row, min_col) for every merged cell range.

    Args:
        worksheet: An openpyxl Worksheet object.

    Returns:
        A set of (row, col) tuples identifying the origin cell of each
        merged range in the worksheet.
    """
    origins: set[tuple[int, int]] = set()
    for merge_range in worksheet.merged_cells.ranges:
        origins.add((merge_range.min_row, merge_range.min_col))
    return origins


def extract_single_sheet(
    worksheet: Worksheet,
    sheet_name: str,
    sheet_state: str,
) -> Sheet:
    """Extract one worksheet into a list of raw cell tuples.

    Each tuple contains:
        (row, col, value, formula, comment, sheet_name, is_merged_origin)

    Hidden or veryHidden sheets get '[HIDDEN]' appended to sheet_name.
    Empty cells (value is None and no formula and no comment) are skipped.
    Cells are returned in row-major order.

    Args:
        worksheet: An openpyxl Worksheet object.
        sheet_name: Display name of the sheet.
        sheet_state: Sheet visibility state ('visible', 'hidden', or 'veryHidden').

    Returns:
        A list of 7-element tuples, one per non-empty cell.
    """
    if sheet_state in ("hidden", "veryHidden"):
        sheet_name = f"{sheet_name}[HIDDEN]"

    merge_origins = collect_merge_origins(worksheet)

    cells: Sheet = []

    # Walk only the cells present in the file, in row-major order. iter_rows()
    # would visit every position in the declared range instead, which a sheet
    # with formatting on a far corner (A1:XFD1048576) inflates to a billion
    # positions: a 100 KB file that never finishes. openpyxl keeps the parsed
    # cells in Worksheet._cells, keyed by (row, column); openpyxl is pinned
    # below 4 and a regression test guards this.
    for (r, c), cell in sorted(worksheet._cells.items()):
        # Extract value and formula
        raw_value = cell.value
        formula = ""
        if isinstance(raw_value, str) and raw_value.startswith("="):
            formula = raw_value
            value = raw_value
        elif raw_value is not None:
            value = str(raw_value)
        else:
            value = ""

        # Extract comment
        comment = ""
        if cell.comment is not None:
            comment = clean_comment(cell.comment.text)

        # Skip truly empty cells
        if not value and not formula and not comment:
            continue

        is_merged_origin = (r, c) in merge_origins
        cells.append((r, c, value, formula, comment, sheet_name, is_merged_origin))

    return cells


def extract_xlsx(filepath: str | Path) -> list[Sheet]:
    """Open an xlsx/xlsm workbook and extract all sheets as raw cell tuples.

    The workbook is opened exactly once with data_only=False to
    preserve formula strings.

    Args:
        filepath: Path to the xlsx or xlsm file.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet.
        Each tuple contains (row, col, value, formula, comment,
        sheet_name, is_merged_origin).
    """
    filepath = Path(filepath)
    # keep_links=False skips cached external-link parts: doormat never reads
    # them, and openpyxl raises KeyError when a workbook's link parts are broken.
    wb = openpyxl.load_workbook(filepath, data_only=False, keep_links=False)

    try:
        sheets: list[Sheet] = []
        # wb.worksheets excludes chartsheets, which hold no cells
        for ws in wb.worksheets:
            cells = extract_single_sheet(ws, ws.title, ws.sheet_state)
            sheets.append(cells)
    finally:
        wb.close()

    return sheets
