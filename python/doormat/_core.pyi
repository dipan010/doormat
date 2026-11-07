"""Type stubs for the compiled Rust extension (doormat-py)."""

from pathlib import Path
from typing import TypedDict

from doormat._types import Sheet

class FindingDict(TypedDict):
    sheet: str
    row: int
    col: int
    header_row: int
    header_col: int
    key: str
    value: str
    confidence: float
    reason: str

def version() -> str:
    """The doormat-core crate version (Cargo form, e.g. ``0.2.0`` or ``0.2.0-rc.1``)."""

def process_workbook(sheets: list[Sheet]) -> list[FindingDict]:
    """Run the detection pipeline on every sheet; one call per workbook."""

def process_xlsx(
    path: Path, max_cells: int | None
) -> tuple[
    list[FindingDict], int, int, list[tuple[str, str, str]], tuple[str, str] | None
]:
    """Read an .xlsx/.xlsm file and run detection in one call.

    Returns (findings, sheet_count, cell_count, sheet_errors, file_error); sheet
    errors are (sheet, kind, message) and file_error is None or (kind, message).
    """

def _read_xlsx(
    path: Path, max_cells: int | None
) -> tuple[list[Sheet], list[tuple[str, str, str]], int, tuple[str, str] | None]:
    """Read an .xlsx/.xlsm file into cell tuples: (sheets, sheet_errors, parsed, file_error)."""
