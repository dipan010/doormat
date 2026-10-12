"""Type aliases shared by the extractors, the API and the extension stub."""

from __future__ import annotations

import zipfile
import zlib
from collections.abc import Callable

CellTuple = tuple[int, int, str, str, str, str, bool]
"""One extracted cell: (row, col, value, formula, comment, sheet_name, is_merged_origin)."""

Sheet = list[CellTuple]
"""All non-empty cells of one sheet."""

SheetErrorHandler = Callable[[str, Exception], None]
"""Called with a sheet's name and the error that stopped it from being read."""

SHEET_ERRORS: tuple[type[Exception], ...] = (
    AttributeError,
    EOFError,
    IndexError,
    KeyError,
    OverflowError,
    SyntaxError,  # xml.etree.ElementTree.ParseError
    TypeError,
    ValueError,  # includes defusedxml's refusals and UnicodeDecodeError
    zipfile.BadZipFile,
    zlib.error,
)
"""Errors a malformed sheet can raise while it is parsed. An extractor given a
``SheetErrorHandler`` reports these per sheet and continues with the next one."""
