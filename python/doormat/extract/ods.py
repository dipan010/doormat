"""Extract raw cell tuples from .ods (OpenDocument Spreadsheet) files via odfpy.

odfpy >= 1.4 is an optional dependency. If not installed, a clear
ImportError is raised when this extractor is called.
"""

from __future__ import annotations

from pathlib import Path


def extract_ods(filepath: str | Path) -> list[list[tuple]]:
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
        from odf.opendocument import load as odf_load
        from odf import table as odf_table
        from odf import text as odf_text
        from odf import office as odf_office
    except ImportError:
        raise ImportError(
            "odfpy is required for .ods support. "
            "Install it with: pip install doormat[ods]"
        ) from None

    filepath = Path(filepath)
    doc = odf_load(str(filepath))

    sheets: list[list[tuple]] = []

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

        cells: list[tuple] = []
        row_idx = 0

        for row in tbl.getElementsByType(odf_table.TableRow):
            # Handle row repetition
            row_repeat = row.getAttribute("numberrowsrepeated")
            row_repeat_count = int(row_repeat) if row_repeat else 1

            # Collect cells in this row
            row_cells: list[tuple[str, str, bool, int]] = []
            for cell in row.childNodes:
                if cell.qname[1] not in ("table-cell", "covered-table-cell"):
                    continue

                # Handle column repetition
                col_repeat = cell.getAttribute("numbercolumnsrepeated")
                col_repeat_count = int(col_repeat) if col_repeat else 1

                # Extract value
                text_content = ""
                for p in cell.getElementsByType(odf_text.P):
                    # Recursively get text content
                    t = _get_text(p)
                    if t:
                        text_content = f"{text_content}\n{t}" if text_content else t

                # Extract formula
                formula = cell.getAttribute("formula") or ""

                # Check for merged cell (spanned columns/rows)
                col_span = cell.getAttribute("numbercolumnsspanned")
                row_span = cell.getAttribute("numberrowsspanned")
                is_merged = (col_span is not None and int(col_span) > 1) or \
                            (row_span is not None and int(row_span) > 1)

                # Extract annotation (comment)
                comment = ""
                annotations = cell.getElementsByType(odf_office.Annotation)
                for ann in annotations:
                    for p in ann.getElementsByType(odf_text.P):
                        t = _get_text(p)
                        if t:
                            comment = f"{comment}\n{t}" if comment else t

                for _ in range(col_repeat_count):
                    row_cells.append((text_content, formula, is_merged, comment))
                    # Only the first cell in a repeated run is the merge origin
                    is_merged = False

            # Emit cells for each repeated row
            for _ in range(min(row_repeat_count, 1000)):  # cap to avoid ODS empty-row explosion
                row_idx += 1
                col_idx = 0
                for value, formula, is_merged_origin, comment in row_cells:
                    col_idx += 1
                    if not value and not formula and not comment:
                        continue
                    cells.append(
                        (row_idx, col_idx, value, formula, comment, sheet_name, is_merged_origin)
                    )
                # Stop emitting repeated rows if the row was empty
                if not any(v or f or c for v, f, _, c in row_cells):
                    row_idx += row_repeat_count - 1
                    break

        sheets.append(cells)

    return sheets


def _get_text(element) -> str:
    """Recursively extract text from an ODF element and its children."""
    parts: list[str] = []
    if hasattr(element, "data"):
        parts.append(element.data)
    if hasattr(element, "childNodes"):
        for child in element.childNodes:
            parts.append(_get_text(child))
    return "".join(parts)
