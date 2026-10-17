"""Work limits shared by the extractors: a cell budget per file."""

from __future__ import annotations

from doormat._types import SheetErrorHandler

DEFAULT_MAX_CELLS = 2_000_000
"""Default ``load(max_cells=...)``. Extraction holds about 0.6 KB per cell, so
this is roughly 1.1 GiB and under a minute. Cells holding only a style are not
counted; the largest of the 15,929 Enron workbooks has about 1.9 million cells
with content."""


class CellLimitError(ValueError):
    """A file holds more cells than the caller's ``max_cells`` allows."""


class CellBudget:
    """Counts cells as they are parsed and refuses the file past a limit."""

    def __init__(self, limit: int | None, filename: str) -> None:
        self.limit = limit
        self.filename = filename
        self.used = 0

    def take(self, count: int) -> None:
        """Account for ``count`` more cells; raise CellLimitError past the limit."""
        self.used += count
        if self.limit is not None and self.used > self.limit:
            raise CellLimitError(
                f"Refusing {self.filename}: it holds more than {self.limit:,} cells "
                f"(raise or disable the limit with max_cells)"
            )


def sheet_failed(on_sheet_error: SheetErrorHandler | None, sheet: str, exc: Exception) -> None:
    """Report a sheet that could not be read, or re-raise.

    The error is re-raised when there is no handler, and always for a
    CellLimitError, which refuses the whole file rather than one sheet.
    """
    if on_sheet_error is None or isinstance(exc, CellLimitError):
        raise exc
    on_sheet_error(sheet, exc)
