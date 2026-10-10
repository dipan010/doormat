"""Tests for doormat.extract.ods: ODS extraction via odfpy."""

from __future__ import annotations

from unittest.mock import patch

import pytest
from doormat.extract.ods import extract_ods


def test_missing_odfpy_raises_import_error(tmp_path):
    """If odfpy is not installed, a helpful ImportError is raised."""
    p = tmp_path / "test.ods"
    p.write_bytes(b"PK\x03\x04" + b"\x00" * 100)
    with patch.dict("sys.modules", {
        "odf": None,
        "odf.opendocument": None,
        "odf.table": None,
        "odf.text": None,
        "odf.office": None,
    }):
        with pytest.raises(ImportError, match=r"odfpy is required"):
            extract_ods(p)


@pytest.fixture
def sample_ods(tmp_path):
    """Create a real .ods file if odfpy is available, skip otherwise."""
    pytest.importorskip("odf")
    from odf import table as odf_table
    from odf import text as odf_text
    from odf.opendocument import OpenDocumentSpreadsheet

    doc = OpenDocumentSpreadsheet()
    tbl = odf_table.Table(name="Sheet1")

    row1 = odf_table.TableRow()
    cell1 = odf_table.TableCell()
    cell1.addElement(odf_text.P(text="Password"))
    row1.addElement(cell1)
    cell2 = odf_table.TableCell()
    cell2.addElement(odf_text.P(text="s3cret!!"))
    row1.addElement(cell2)
    tbl.addElement(row1)

    row2 = odf_table.TableRow()
    cell3 = odf_table.TableCell()
    cell3.addElement(odf_text.P(text="Token"))
    row2.addElement(cell3)
    cell4 = odf_table.TableCell()
    cell4.addElement(odf_text.P(text="abc123"))
    row2.addElement(cell4)
    tbl.addElement(row2)

    doc.spreadsheet.addElement(tbl)

    p = tmp_path / "test.ods"
    doc.save(str(p))
    return p


def test_basic_extraction(sample_ods):
    """Basic .ods extraction produces correct tuples."""
    sheets = extract_ods(sample_ods)
    assert len(sheets) == 1
    cells = sheets[0]
    assert len(cells) == 4
    assert cells[0][:3] == (1, 1, "Password")
    assert cells[1][:3] == (1, 2, "s3cret!!")
    assert cells[2][:3] == (2, 1, "Token")
    assert cells[3][:3] == (2, 2, "abc123")
    assert cells[0][5] == "Sheet1"
