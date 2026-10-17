"""Tests for doormat.extract.xls: legacy .xls extraction via xlrd."""

from __future__ import annotations

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
    pytest.importorskip("xlrd")
    # xlrd v2 can only read, not write .xls files.
    # We need xlwt or a fixture file. Use a minimal BIFF fixture.
    try:
        import xlwt
    except ImportError:
        pytest.skip("xlwt not installed; needed to create .xls test fixtures")
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


def test_numeric_password_keeps_its_digits(tmp_path):
    """xlrd returns numbers as floats; 12345678 must not become '12345678.0'."""
    pytest.importorskip("xlrd")
    xlwt = pytest.importorskip("xlwt")
    import doormat

    wb = xlwt.Workbook()
    ws = wb.add_sheet("Logins")
    ws.write(0, 0, "Password")
    ws.write(0, 1, 12345678)
    ws.write(1, 0, "Rate")
    ws.write(1, 1, 2.5)
    path = tmp_path / "numbers.xls"
    wb.save(str(path))

    found = doormat.load(path).credentials(120)
    assert [(c.sheet, c.coordinate, c.value) for c in found] == [("Logins", "B1", "12345678")]
    cells = {(r, c): v for r, c, v, *_ in doormat.extract.extract_xls(path)[0]}
    assert cells[(2, 2)] == "2.5"


def _dir_entry(data: bytes, name: str) -> int:
    """Offset of the OLE2 directory entry called ``name``."""
    offset = data.find(name.encode("utf-16-le") + b"\x00\x00")
    assert offset != -1 and data[offset + 64 : offset + 66] == ((len(name) + 1) * 2).to_bytes(2, "little")
    return offset


def test_workbook_sharing_a_sector_with_another_stream_is_read(sample_xls):
    """xlrd's strict check refuses it; about a third of web .xls files look like this."""
    import struct

    import xlrd

    data = bytearray(sample_xls.read_bytes())
    workbook_sid = struct.unpack("<i", data[_dir_entry(data, "Workbook") + 116 : _dir_entry(data, "Workbook") + 120])[0]
    root = _dir_entry(data, "Root Entry")
    data[root + 116 : root + 124] = struct.pack("<iI", workbook_sid, 64)  # mini stream over the workbook
    sample_xls.write_bytes(bytes(data))
    with pytest.raises(xlrd.compdoc.CompDocError):
        xlrd.open_workbook(sample_xls, logfile=__import__("io").StringIO())
    assert [c[2] for c in extract_xls(sample_xls)[0]] == ["Password", "s3cret!!", "Token", "abc123"]


def test_xlrd_writes_nothing_to_stdout(sample_xls, capsys):
    import struct

    data = bytearray(sample_xls.read_bytes())
    workbook_sid = struct.unpack("<i", data[_dir_entry(data, "Workbook") + 116 : _dir_entry(data, "Workbook") + 120])[0]
    root = _dir_entry(data, "Root Entry")
    data[root + 116 : root + 124] = struct.pack("<iI", workbook_sid, 64)
    sample_xls.write_bytes(bytes(data) + b"\x00" * 7)  # also a size that is not a whole sector
    extract_xls(sample_xls)
    assert capsys.readouterr().out == ""


def test_workbook_stream_larger_than_the_file_is_refused(sample_xls):
    """Without the strict check, reused sectors could build a stream far larger than the file."""
    data = bytearray(sample_xls.read_bytes())
    entry = _dir_entry(data, "Workbook")
    data[entry + 120 : entry + 124] = (len(data) * 100).to_bytes(4, "little")
    sample_xls.write_bytes(bytes(data))
    with pytest.raises(ValueError, match="larger than the file"):
        extract_xls(sample_xls)
