"""Extract raw cell tuples from xlsx/xlsm files via openpyxl.

The workbook is opened exactly once, with data_only=False so formulas are kept.
Only cells present in the file are visited, never the declared sheet range.

openpyxl's loader also parses workbook parts doormat never uses, and a
malformed one makes the whole file unreadable. ``_CellReader`` reads only what
extraction needs, one sheet at a time, so a malformed sheet can be reported
while the others are still read. It relies on openpyxl internals, so openpyxl
is pinned below 4 and regression tests build each malformed part.
"""

from __future__ import annotations

from collections.abc import Iterator
from pathlib import Path
from typing import Any

from openpyxl.cell.cell import MergedCell
from openpyxl.comments.comment_sheet import CommentSheet
from openpyxl.packaging.relationship import (
    RelationshipList,
    get_dependents,
    get_rels_path,
)
from openpyxl.reader.excel import ExcelReader
from openpyxl.styles.stylesheet import apply_stylesheet
from openpyxl.worksheet._reader import WorksheetReader
from openpyxl.worksheet.formula import ArrayFormula, DataTableFormula
from openpyxl.worksheet.worksheet import Worksheet
from openpyxl.xml.constants import COMMENTS_NS
from openpyxl.xml.functions import fromstring

from doormat._limits import CellBudget, sheet_failed
from doormat._types import SHEET_ERRORS, Sheet, SheetErrorHandler


class _CellReader(ExcelReader):
    """openpyxl reader limited to the parts that hold cells.

    ``open()`` reads shared strings, the sheet list and styles (to recognise
    dates); ``read_worksheet()`` then reads one sheet with its comments. Nothing
    else is parsed: not document properties, the theme, defined names or print
    settings (an invalid print-titles name such as ``#N/A`` makes openpyxl refuse
    the file), pivot tables and their caches (a ``-`` where a number belongs
    raises ``TypeError``), charts, images or external-link caches (broken link
    parts raise ``KeyError``).
    """

    def __init__(self, filepath: Path) -> None:
        super().__init__(filepath, data_only=False, keep_links=False)

    def open(self) -> None:
        """Read everything the worksheets depend on."""
        self.read_manifest()
        self.read_strings()
        self.read_workbook()
        apply_stylesheet(self.archive, self.wb)

    def worksheet_parts(self) -> Iterator[tuple[Any, Any]]:
        """Yield ``(sheet, relationship)`` for each worksheet, skipping chartsheets."""
        for sheet, rel in self.parser.find_sheets():
            if rel.target in self.valid_files and "chartsheet" not in rel.Type:
                yield sheet, rel

    def read_worksheet(self, sheet: Any, rel: Any, budget: CellBudget | None = None) -> Worksheet:
        """Read one worksheet's cells, merged ranges, comments and visibility.

        With a budget, cells are counted row by row while the sheet is parsed,
        so an oversized sheet is refused before it is fully in memory.
        """
        rels_path = get_rels_path(rel.target)  # type: ignore[no-untyped-call]
        rels = RelationshipList()
        if rels_path in self.valid_files:
            rels = get_dependents(self.archive, rels_path)

        ws = Worksheet(self.wb, title=sheet.name)
        ws._rels = rels  # type: ignore[attr-defined]  # hyperlinks resolve through it
        with self.archive.open(rel.target) as fh:
            ws_reader = WorksheetReader(ws, fh, self.shared_strings, self.data_only, self.rich_text)
            if budget is not None:
                parse = ws_reader.parser.parse

                def counted() -> Iterator[Any]:
                    for row in parse():
                        budget.take(len(row[1]))
                        yield row

                # Wrap this reader's parser only; openpyxl itself is not patched.
                setattr(ws_reader.parser, "parse", counted)  # noqa: B010
            ws_reader.bind_all()

        for comments_rel in rels.find(COMMENTS_NS):
            tree = fromstring(self.archive.read(comments_rel.target))
            comment_sheet = CommentSheet.from_tree(tree)  # type: ignore[arg-type]
            if comment_sheet is None:
                continue
            for ref, comment in comment_sheet.comments:
                cell = ws[ref]
                # A merged cell other than the range's origin cannot hold a comment.
                if not isinstance(cell, MergedCell):
                    cell.comment = comment

        ws.sheet_state = sheet.state
        return ws


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
        # Extract value and formula. Array formulas arrive as objects holding the
        # formula text; a what-if data table cell has no text of its own.
        raw_value = cell.value
        if isinstance(raw_value, ArrayFormula):
            raw_value = raw_value.text
        elif isinstance(raw_value, DataTableFormula):
            raw_value = None
        formula = ""
        # Only a formula cell holds a formula; text that merely starts with "="
        # (a password such as "=k9X...") is a value.
        if cell.data_type == "f" and isinstance(raw_value, str):
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


def extract_xlsx(
    filepath: str | Path,
    on_sheet_error: SheetErrorHandler | None = None,
    budget: CellBudget | None = None,
) -> list[Sheet]:
    """Open an xlsx/xlsm workbook and extract all sheets as raw cell tuples.

    The workbook is opened exactly once with data_only=False to
    preserve formula strings.

    Args:
        filepath: Path to the xlsx or xlsm file.
        on_sheet_error: Called with the sheet name and error when a sheet
            cannot be read; extraction continues with the next sheet. If
            None, the error is raised.
        budget: Counts cells as they are read and raises CellLimitError
            (a ValueError) past its limit. If None, nothing is counted.

    Returns:
        A list of lists, one inner list of 7-element tuples per sheet that
        was read. Each tuple contains (row, col, value, formula, comment,
        sheet_name, is_merged_origin).
    """
    reader = _CellReader(Path(filepath))
    try:
        reader.open()
        sheets: list[Sheet] = []
        for sheet, rel in reader.worksheet_parts():
            try:
                ws = reader.read_worksheet(sheet, rel, budget)
                sheets.append(extract_single_sheet(ws, ws.title, ws.sheet_state))
            except SHEET_ERRORS as exc:
                sheet_failed(on_sheet_error, sheet.name, exc)
    finally:
        reader.archive.close()

    return sheets
