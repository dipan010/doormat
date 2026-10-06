"""Extraction subpackage: format-specific extractors for doormat.

Each extractor reads a spreadsheet file and returns a normalized
list[list[tuple]] where each tuple is:
    (row, col, value, formula, comment, sheet_name, is_merged_origin)
"""

from __future__ import annotations

from doormat.extract.csv_tsv import extract_csv
from doormat.extract.ods import extract_ods
from doormat.extract.xls import extract_xls
from doormat.extract.xlsx import (
    clean_comment,
    collect_merge_origins,
    extract_single_sheet,
    extract_xlsx,
)

# Backward-compatible alias used by existing code
extract_workbook = extract_xlsx

__all__ = [
    "clean_comment",
    "collect_merge_origins",
    "extract_csv",
    "extract_ods",
    "extract_single_sheet",
    "extract_workbook",
    "extract_xls",
    "extract_xlsx",
]
