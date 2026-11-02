"""Read .xlsx/.xlsm files with doormat's Rust reader (design decision 0021).

``load()`` reads and scans a workbook in one call to ``_core.process_xlsx``.
``extract_xlsx`` returns the cell tuples alone, with the same signature as the
other extractors, for tools that need the cells.
"""

from __future__ import annotations

import zipfile
from pathlib import Path
from xml.etree.ElementTree import ParseError

from doormat import _core
from doormat._limits import CellBudget, CellLimitError, sheet_failed
from doormat._types import Sheet, SheetErrorHandler

_ERRORS: dict[str, type[Exception]] = {
    "AttributeError": AttributeError,
    "BadZipFile": zipfile.BadZipFile,
    "CellLimitError": CellLimitError,
    "FileNotFoundError": FileNotFoundError,
    "IndexError": IndexError,
    "KeyError": KeyError,
    "OSError": OSError,
    "OverflowError": OverflowError,
    "ParseError": ParseError,
    "TypeError": TypeError,
}


def read_error(kind: str, message: str) -> Exception:
    """The exception the openpyxl reader raised for a failure the Rust reader reports as ``kind``.

    Kinds without a Python counterpart here (``EntitiesForbidden``,
    ``TokenizerError``, ``TranslatorError``, ``IllegalCharacterError``)
    become ``ValueError``, naming the kind.
    """
    cls = _ERRORS.get(kind)
    if cls is None:
        return ValueError(f"{kind}: {message}")
    return cls(message)


def extract_xlsx(
    filepath: str | Path,
    on_sheet_error: SheetErrorHandler | None = None,
    budget: CellBudget | None = None,
) -> list[Sheet]:
    """Extract all sheets of an .xlsx/.xlsm workbook as raw cell tuples.

    Args:
        filepath: Path to the workbook.
        on_sheet_error: Called with the sheet name and error when a sheet
            cannot be read; extraction continues with the next sheet. If
            None, the error is raised.
        budget: Refuses the file with CellLimitError past its limit. If None,
            nothing is counted.

    Returns:
        One list of (row, col, value, formula, comment, sheet_name,
        is_merged_origin) tuples per sheet read.
    """
    limit = budget.limit if budget is not None else None
    sheets, sheet_errors, parsed, failure = _core._read_xlsx(Path(filepath), limit)
    if failure is not None:
        raise read_error(*failure)
    if budget is not None:
        budget.used += parsed
    for sheet, kind, message in sheet_errors:
        sheet_failed(on_sheet_error, sheet, read_error(kind, message))
    return sheets
