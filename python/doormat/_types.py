"""Type aliases shared by the extractors, the API and the extension stub."""

from __future__ import annotations

CellTuple = tuple[int, int, str, str, str, str, bool]
"""One extracted cell: (row, col, value, formula, comment, sheet_name, is_merged_origin)."""

Sheet = list[CellTuple]
"""All non-empty cells of one sheet."""
