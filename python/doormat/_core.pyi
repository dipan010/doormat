"""Type stubs for the compiled Rust extension (doormat-py)."""

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
