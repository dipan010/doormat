"""Extract raw cell tuples from CSV and TSV files.

Uses the stdlib csv module, so no additional dependencies required.
CSV/TSV files are treated as single-sheet documents with no formulas,
comments, or merged cells.
"""

from __future__ import annotations

import csv
from pathlib import Path

from doormat._limits import CellBudget
from doormat._types import Sheet, SheetErrorHandler


def extract_csv(
    filepath: str | Path,
    on_sheet_error: SheetErrorHandler | None = None,
    budget: CellBudget | None = None,
) -> list[Sheet]:
    """Extract cells from a CSV or TSV file as raw cell tuples.

    The delimiter is inferred from the file extension: ``.csv`` uses
    comma, ``.tsv`` uses tab. Row and column indices are 1-based to
    match the openpyxl convention used by the rest of the pipeline.

    The sheet name is set to the filename stem (e.g., ``"data"`` for
    ``data.csv``).

    Args:
        filepath: Path to the CSV or TSV file.

        on_sheet_error: Called with the sheet name and error when the file
            cannot be decoded or parsed; an empty list is then returned. If
            None, the error is raised.
        budget: Counts cells as they are read and raises CellLimitError
            (a ValueError) past its limit. If None, nothing is counted.

    Returns:
        A single-element list containing one list of 7-element tuples
        (one "sheet"). Each tuple is
        ``(row, col, value, formula, comment, sheet_name, is_merged_origin)``.
        Formula and comment are always ``""``, is_merged_origin is
        always ``False``.

    Raises:
        ValueError: If the file cannot be decoded as UTF-8 or parsed as
            delimited text, and ``on_sheet_error`` is None.
    """
    filepath = Path(filepath)
    sheet_name = filepath.stem

    delimiter = "\t" if filepath.suffix.lower() == ".tsv" else ","

    try:
        with filepath.open(newline="", encoding="utf-8") as f:
            reader = csv.reader(f, delimiter=delimiter)
            cells: Sheet = []
            for row_idx, row in enumerate(reader, start=1):
                if budget is not None:
                    budget.take(len(row))
                for col_idx, value in enumerate(row, start=1):
                    if not value:
                        continue
                    cells.append(
                        (row_idx, col_idx, value, "", "", sheet_name, False)
                    )
    except (UnicodeDecodeError, csv.Error) as exc:
        error = ValueError(f"Failed to read {filepath.name} as UTF-8 delimited text: {exc}")
        if on_sheet_error is None:
            raise error from exc
        on_sheet_error(sheet_name, error)
        return []

    return [cells]
