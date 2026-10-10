"""Extract raw cell tuples from legacy .xls (BIFF) files via xlrd.

xlrd >= 2.0 is an optional dependency. If not installed, a clear
ImportError is raised when this extractor is called.
"""

from __future__ import annotations

from pathlib import Path

from doormat._types import Sheet


def extract_xls(filepath: str | Path) -> list[Sheet]:
    """Extract cells from a legacy .xls file as raw cell tuples.

    Requires ``xlrd >= 2.0``. Installs via ``pip install doormat[xls]``.

    Args:
        filepath: Path to the .xls file.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet.
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
    book = xlrd.open_workbook(filepath, formatting_info=False)

    try:
        sheets: list[Sheet] = []
        for sheet_idx in range(book.nsheets):
            sheet = book.sheet_by_index(sheet_idx)
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
                for col_idx in range(sheet.ncols):
                    cell = sheet.cell(row_idx, col_idx)
                    if cell.ctype == xlrd.XL_CELL_EMPTY:
                        continue

                    value = str(cell.value) if cell.value != "" else ""
                    if not value:
                        continue

                    # Convert to 1-based indices
                    r = row_idx + 1
                    c = col_idx + 1
                    is_merged_origin = (r, c) in merge_origins

                    cells.append(
                        (r, c, value, "", "", sheet_name, is_merged_origin)
                    )

            sheets.append(cells)
    finally:
        book.release_resources()

    return sheets
