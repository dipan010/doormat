"""Tests for doormat.extract.csv_tsv — CSV and TSV cell extraction."""

from __future__ import annotations

from pathlib import Path

import pytest

from doormat.extract.csv_tsv import extract_csv


def test_basic_csv(tmp_path):
    """Basic CSV produces correct 7-tuples with 1-indexed coords."""
    p = tmp_path / "data.csv"
    p.write_text("Name,Age\nAlice,30\n")
    sheets = extract_csv(p)
    assert len(sheets) == 1
    cells = sheets[0]
    assert len(cells) == 4
    # First cell: row=1, col=1, value="Name"
    assert cells[0] == (1, 1, "Name", "", "", "data", False)
    assert cells[1] == (1, 2, "Age", "", "", "data", False)
    assert cells[2] == (2, 1, "Alice", "", "", "data", False)
    assert cells[3] == (2, 2, "30", "", "", "data", False)


def test_tsv(tmp_path):
    """TSV uses tab delimiter."""
    p = tmp_path / "data.tsv"
    p.write_text("Name\tAge\nAlice\t30\n")
    sheets = extract_csv(p)
    cells = sheets[0]
    assert len(cells) == 4
    assert cells[0][2] == "Name"
    assert cells[1][2] == "Age"


def test_empty_cells_skipped(tmp_path):
    """Empty values in CSV are skipped."""
    p = tmp_path / "sparse.csv"
    p.write_text("a,,b\n,,c\n")
    sheets = extract_csv(p)
    cells = sheets[0]
    values = [c[2] for c in cells]
    assert values == ["a", "b", "c"]


def test_sheet_name_is_stem(tmp_path):
    """sheet_name should be the filename stem."""
    p = tmp_path / "my_report.csv"
    p.write_text("x,y\n")
    sheets = extract_csv(p)
    assert sheets[0][0][5] == "my_report"


def test_formula_comment_merged_defaults(tmp_path):
    """Formula, comment, and is_merged_origin should have empty defaults."""
    p = tmp_path / "test.csv"
    p.write_text("value\n")
    sheets = extract_csv(p)
    cell = sheets[0][0]
    assert cell[3] == ""       # formula
    assert cell[4] == ""       # comment
    assert cell[6] is False    # is_merged_origin


def test_empty_file(tmp_path):
    """An empty CSV file returns one empty sheet."""
    p = tmp_path / "empty.csv"
    p.write_text("")
    sheets = extract_csv(p)
    assert sheets == [[]]


def test_encoding_error(tmp_path):
    """Non-UTF-8 content raises ValueError."""
    p = tmp_path / "bad.csv"
    p.write_bytes(b"\xff\xfe\x00\x01")
    with pytest.raises(ValueError, match=r"UTF-8"):
        extract_csv(p)
