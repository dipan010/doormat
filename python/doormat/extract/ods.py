"""Extract raw cell tuples from .ods (OpenDocument Spreadsheet) files via odfpy.

odfpy >= 1.4 is an optional dependency. If not installed, a clear
ImportError is raised when this extractor is called.
"""

from __future__ import annotations

from pathlib import Path
from typing import Any

from doormat._types import Sheet

# LibreOffice's column limit; cells beyond it are ignored.
MAX_COLUMNS = 16_384
# A non-empty row repeated more often than this is emitted this many times.
MAX_ROW_REPEAT = 1_000


def extract_ods(filepath: str | Path) -> list[Sheet]:
    """Extract cells from an .ods file as raw cell tuples.

    Requires ``odfpy >= 1.4``. Installs via ``pip install doormat[ods]``.

    Args:
        filepath: Path to the .ods file.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet.
        Each tuple is
        ``(row, col, value, formula, comment, sheet_name, is_merged_origin)``.

    Raises:
        ImportError: If odfpy is not installed.
    """
    try:
        from odf import office as odf_office
        from odf import table as odf_table
        from odf import text as odf_text
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

        # Check table style for visibility (hidden tables)
        table_style = tbl.getAttribute("stylename")
        is_hidden = False
        if table_style:
            # Check automatic styles for display property
            for auto_style in doc.automaticstyles.childNodes:
                style_name = getattr(auto_style, "getAttribute", lambda _: None)("name")
                if style_name == table_style:
                    for prop in auto_style.childNodes:
                        display = getattr(prop, "getAttribute", lambda _: None)("display")
                        if display == "false":
                            is_hidden = True
                            break

        if is_hidden:
            sheet_name = f"{sheet_name}[HIDDEN]"

        cells: Sheet = []
        row_idx = 0

        for row in tbl.getElementsByType(odf_table.TableRow):
            # Handle row repetition
            row_repeat = row.getAttribute("numberrowsrepeated")
            row_repeat_count = int(row_repeat) if row_repeat else 1

            # Non-empty cells of this row as (col, value, formula, merged_origin, comment).
            # Repeated empty cells only advance the column counter, so a cell that
            # claims to repeat a billion times costs nothing.
            row_cells: list[tuple[int, str, str, bool, str]] = []
            col_idx = 0
            for cell in row.childNodes:
                if cell.qname[1] not in ("table-cell", "covered-table-cell"):
                    continue
                if col_idx >= MAX_COLUMNS:
                    break

                # Handle column repetition
                col_repeat = cell.getAttribute("numbercolumnsrepeated")
                col_repeat_count = int(col_repeat) if col_repeat else 1

                # Extract value
                text_content = ""
                for p in cell.getElementsByType(odf_text.P):
                    t = _get_text(p)
                    if t:
                        text_content = f"{text_content}\n{t}" if text_content else t

                # Extract formula
                formula = cell.getAttribute("formula") or ""

                # Check for merged cell (spanned columns/rows)
                col_span = cell.getAttribute("numbercolumnsspanned")
                row_span = cell.getAttribute("numberrowsspanned")
                is_merged = (col_span is not None and int(col_span) > 1) or (
                    row_span is not None and int(row_span) > 1
                )

                # Extract annotation (comment)
                comment = ""
                for ann in cell.getElementsByType(odf_office.Annotation):
                    for p in ann.getElementsByType(odf_text.P):
                        t = _get_text(p)
                        if t:
                            comment = f"{comment}\n{t}" if comment else t

                if text_content or formula or comment:
                    # Only the first cell in a repeated run is the merge origin
                    row_cells.extend(
                        (col_idx + offset + 1, text_content, formula, is_merged and offset == 0, comment)
                        for offset in range(min(col_repeat_count, MAX_COLUMNS - col_idx))
                    )
                col_idx += col_repeat_count

            # Emit the row once per repetition (capped), but always advance the row
            # counter by the full repeat count so later rows keep their real numbers.
            if row_cells:
                for offset in range(min(row_repeat_count, MAX_ROW_REPEAT)):
                    r = row_idx + offset + 1
                    cells.extend(
                        (r, c, value, formula, comment, sheet_name, merged)
                        for c, value, formula, merged, comment in row_cells
                    )
            row_idx += row_repeat_count

        sheets.append(cells)

    return sheets


def _get_text(element: Any) -> str:
    """Recursively extract text from an ODF element and its children."""
    parts: list[str] = []
    if hasattr(element, "data"):
        parts.append(element.data)
    if hasattr(element, "childNodes"):
        parts.extend(_get_text(child) for child in element.childNodes)
    return "".join(parts)
