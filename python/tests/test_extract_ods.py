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


def test_rows_after_a_long_repeated_row_keep_their_numbers(tmp_path):
    """A non-empty row repeated 1,500 times is emitted at most 1,000 times,
    but the rows after it must still be numbered from 1,501."""
    pytest.importorskip("odf")
    from odf.opendocument import OpenDocumentSpreadsheet
    from odf.table import Table, TableCell, TableRow
    from odf.text import P

    import doormat

    doc = OpenDocumentSpreadsheet()
    table = Table(name="S1")
    filler = TableRow(numberrowsrepeated=1500)
    cell = TableCell()
    cell.addElement(P(text="x"))
    filler.addElement(cell)
    table.addElement(filler)
    row = TableRow()
    for text in ("Password", "s3cret!!"):
        cell = TableCell()
        cell.addElement(P(text=text))
        row.addElement(cell)
    table.addElement(row)
    doc.spreadsheet.addElement(table)
    path = tmp_path / "rows.ods"
    doc.save(str(path))

    found = doormat.load(path).credentials(120)
    assert [(c.row, c.coordinate) for c in found] == [(1501, "B1501")]


def test_ods_xml_entity_declarations_are_rejected(tmp_path):
    """odfpy parses content.xml with defusedxml, so entity declarations are refused."""
    import zipfile

    pytest.importorskip("odf")
    from odf.opendocument import OpenDocumentSpreadsheet
    from odf.table import Table, TableCell, TableRow
    from odf.text import P

    import doormat

    doc = OpenDocumentSpreadsheet()
    table = Table(name="S1")
    row = TableRow()
    cell = TableCell()
    cell.addElement(P(text="Password"))
    row.addElement(cell)
    table.addElement(row)
    doc.spreadsheet.addElement(table)
    clean = tmp_path / "clean.ods"
    doc.save(str(clean))

    evil = tmp_path / "entities.ods"
    with zipfile.ZipFile(clean) as src, zipfile.ZipFile(evil, "w") as dst:
        for item in src.infolist():
            data = src.read(item.filename)
            if item.filename == "content.xml":
                marker = b"<office:document-content"
                assert marker in data
                data = data.replace(marker, b'<!DOCTYPE r [<!ENTITY e "x">]>' + marker, 1)
            dst.writestr(item, data)

    with pytest.raises(ValueError):
        doormat.load(evil)


def test_merged_range_with_covered_cells_is_read(tmp_path):
    """LibreOffice writes covered cells for every merged range; they must not fail the sheet."""
    pytest.importorskip("odf")
    from odf.opendocument import OpenDocumentSpreadsheet
    from odf.table import CoveredTableCell, Table, TableCell, TableRow
    from odf.text import P

    from doormat.extract.ods import extract_ods

    doc = OpenDocumentSpreadsheet()
    table = Table(name="S1")
    row = TableRow()
    label = TableCell(numbercolumnsspanned=2)
    label.addElement(P(text="Password"))
    row.addElement(label)
    row.addElement(CoveredTableCell())
    value = TableCell()
    value.addElement(P(text="s3cRet!99"))
    row.addElement(value)
    table.addElement(row)
    doc.spreadsheet.addElement(table)
    path = tmp_path / "merged.ods"
    doc.save(str(path))

    cells = extract_ods(path)[0]
    assert [(c[1], c[2], c[6]) for c in cells] == [(1, "Password", True), (3, "s3cRet!99", False)]
