"""Extract raw cell tuples from CSV and TSV files.

Uses the stdlib csv module — no additional dependencies required.
CSV/TSV files are treated as single-sheet documents with no formulas,
comments, or merged cells.
"""

from __future__ import annotations

import csv
from pathlib import Path


def extract_csv(filepath: str | Path) -> list[list[tuple]]:
    """Extract cells from a CSV or TSV file as raw cell tuples.

    The delimiter is inferred from the file extension: ``.csv`` uses
    comma, ``.tsv`` uses tab. Row and column indices are 1-based to
    match the openpyxl convention used by the rest of the pipeline.

    The sheet name is set to the filename stem (e.g., ``"data"`` for
    ``data.csv``).

    Args:
        filepath: Path to the CSV or TSV file.

    Returns:
        A single-element list containing one list of 7-element tuples
        (one "sheet"). Each tuple is
        ``(row, col, value, formula, comment, sheet_name, is_merged_origin)``.
        Formula and comment are always ``""``, is_merged_origin is
        always ``False``.

    Raises:
        ValueError: If the file cannot be decoded as UTF-8.
    """
    filepath = Path(filepath)
    sheet_name = filepath.stem

    delimiter = "\t" if filepath.suffix.lower() == ".tsv" else ","

    try:
        with open(filepath, newline="", encoding="utf-8") as f:
            reader = csv.reader(f, delimiter=delimiter)
            cells: list[tuple] = []
            for row_idx, row in enumerate(reader, start=1):
                for col_idx, value in enumerate(row, start=1):
                    if not value:
                        continue
                    cells.append(
                        (row_idx, col_idx, value, "", "", sheet_name, False)
                    )
    except UnicodeDecodeError as exc:
        raise ValueError(
            f"Failed to decode {filepath.name} as UTF-8: {exc}"
        ) from exc

    return [cells]
