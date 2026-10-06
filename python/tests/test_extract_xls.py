"""Tests for doormat.extract.xls — legacy .xls extraction via xlrd."""

from __future__ import annotations

from pathlib import Path
from unittest.mock import patch

import pytest

from doormat.extract.xls import extract_xls


def test_missing_xlrd_raises_import_error(tmp_path):
    """If xlrd is not installed, a helpful ImportError is raised."""
    p = tmp_path / "test.xls"
    p.write_bytes(b"\xD0\xCF\x11\xE0" + b"\x00" * 100)
    with patch.dict("sys.modules", {"xlrd": None}):
        with pytest.raises(ImportError, match=r"xlrd is required"):
            extract_xls(p)


@pytest.fixture
def sample_xls(tmp_path):
    """Create a real .xls file if xlrd is available, skip otherwise."""
    xlrd = pytest.importorskip("xlrd")
    # xlrd v2 can only read, not write .xls files.
    # We need xlwt or a fixture file. Use a minimal BIFF fixture.
    try:
        import xlwt
    except ImportError:
        pytest.skip("xlwt not installed — needed to create .xls test fixtures")
    wb = xlwt.Workbook()
    ws = wb.add_sheet("Sheet1")
    ws.write(0, 0, "Password")
    ws.write(0, 1, "s3cret!!")
    ws.write(1, 0, "Token")
    ws.write(1, 1, "abc123")
    p = tmp_path / "test.xls"
    wb.save(str(p))
    return p


def test_basic_extraction(sample_xls):
    """Basic .xls extraction produces correct tuples."""
    sheets = extract_xls(sample_xls)
    assert len(sheets) == 1
    cells = sheets[0]
    assert len(cells) == 4
    # 1-indexed coords
    assert cells[0][:3] == (1, 1, "Password")
    assert cells[1][:3] == (1, 2, "s3cret!!")
    assert cells[2][:3] == (2, 1, "Token")
    assert cells[3][:3] == (2, 2, "abc123")
    # sheet_name
    assert cells[0][5] == "Sheet1"
    # formula and comment are empty
    assert cells[0][3] == ""
    assert cells[0][4] == ""
