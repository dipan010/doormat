"""Tests for doormat.extract.extract_xlsx: .xlsx cell extraction by the Rust reader."""

from __future__ import annotations

import zipfile
from pathlib import Path

import openpyxl
import pytest
from doormat.extract import extract_xlsx
from openpyxl.chart import BarChart, Reference


@pytest.fixture
def tmp_xlsx(tmp_path: Path):
    """Helper that writes an openpyxl Workbook to a temp file and returns the path."""

    def _write(wb: openpyxl.Workbook) -> Path:
        p = tmp_path / "test.xlsx"
        wb.save(p)
        return p

    return _write


# ---------- cells, comments, formulas, merges ----------


def _cells(tmp_xlsx, wb: openpyxl.Workbook) -> list[tuple]:
    path = tmp_xlsx(wb)
    wb.close()
    return extract_xlsx(path)[0]


def test_extract_three_cells(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = "Sheet1"
    ws["A1"] = "Name"
    ws["B1"] = "Alice"
    ws["A2"] = "Age"
    cells = _cells(tmp_xlsx, wb)
    assert [c[:3] for c in cells] == [(1, 1, "Name"), (1, 2, "Alice"), (2, 1, "Age")]
    assert {c[5] for c in cells} == {"Sheet1"}


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("secret note", "secret note"),
        ("  padded  ", "padded"),
        ("Comment:\nactual text", "actual text"),
    ],
)
def test_cell_comment_is_cleaned(tmp_xlsx, text, expected):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["A1"].comment = openpyxl.comments.Comment(text, "author")
    assert [c[4] for c in _cells(tmp_xlsx, wb)] == [expected]


def test_comment_on_empty_cell_is_kept(tmp_xlsx):
    wb = openpyxl.Workbook()
    wb.active["C3"].comment = openpyxl.comments.Comment("note", "author")
    assert [c[:5] for c in _cells(tmp_xlsx, wb)] == [(3, 3, "", "", "note")]


def test_cell_with_formula(tmp_xlsx):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "=SUM(A1:A5)"
    assert [c[2:4] for c in _cells(tmp_xlsx, wb)] == [("=SUM(A1:A5)", "=SUM(A1:A5)")]


def test_merged_cell_origin(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws["A1"] = "Merged"
    ws["B1"] = "NotMerged"
    ws.merge_cells("A1:A3")
    ws.merge_cells("D5:F7")
    flags = {(c[0], c[1]): c[6] for c in _cells(tmp_xlsx, wb)}
    assert flags == {(1, 1): True, (1, 2): False}


def test_empty_cells_skipped(tmp_xlsx):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "data"
    wb.active["D1"] = "more"
    assert len(_cells(tmp_xlsx, wb)) == 2


@pytest.mark.parametrize("state", ["hidden", "veryHidden"])
def test_hidden_sheet_names_are_marked(tmp_xlsx, state):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "open"
    secret = wb.create_sheet("Secret")
    secret["A1"] = "data"
    secret.sheet_state = state
    path = tmp_xlsx(wb)
    wb.close()
    assert extract_xlsx(path)[1][0][5] == "Secret[HIDDEN]"


# ---------- extract_workbook ----------


def test_extract_workbook_one_sheet(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws["A1"] = "Password"
    ws["B1"] = "s3cret!!"
    ws["A2"] = "Token"
    p = tmp_xlsx(wb)
    wb.close()

    sheets = extract_xlsx(p)
    assert len(sheets) == 1
    assert len(sheets[0]) == 3


def test_extract_workbook_multiple_sheets(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws1 = wb.active
    ws1.title = "Sheet1"
    ws1["A1"] = "data1"

    ws2 = wb.create_sheet("Sheet2")
    ws2["A1"] = "data2"
    ws2["B1"] = "data3"

    p = tmp_xlsx(wb)
    wb.close()

    sheets = extract_xlsx(p)
    assert len(sheets) == 2
    assert len(sheets[0]) == 1
    assert len(sheets[1]) == 2


def test_extract_workbook_hidden_sheet(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws1 = wb.active
    ws1.title = "Visible"
    ws1["A1"] = "open"

    ws2 = wb.create_sheet("Hidden")
    ws2["A1"] = "secret"
    ws2.sheet_state = "hidden"

    p = tmp_xlsx(wb)
    wb.close()

    sheets = extract_xlsx(p)
    assert len(sheets) == 2
    # Hidden sheet should have [HIDDEN] suffix
    hidden_cells = sheets[1]
    assert hidden_cells[0][5] == "Hidden[HIDDEN]"


def test_extract_workbook_accepts_path_object(tmp_xlsx):
    wb = openpyxl.Workbook()
    ws = wb.active
    ws["A1"] = "test"
    p = tmp_xlsx(wb)
    wb.close()

    # Should accept both str and Path
    sheets_str = extract_xlsx(str(p))
    sheets_path = extract_xlsx(p)
    assert len(sheets_str) == len(sheets_path)


def test_extract_workbook_skips_chartsheet(tmp_xlsx):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["B1"] = "s3cRet!99"
    chart = BarChart()
    chart.add_data(Reference(wb.active, min_col=2, min_row=1, max_row=1))
    wb.create_chartsheet("Chart1").add_chart(chart)
    p = tmp_xlsx(wb)
    wb.close()

    sheets = extract_xlsx(p)
    assert len(sheets) == 1
    assert [c[2] for c in sheets[0]] == ["Password", "s3cRet!99"]


REL_NS = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"


def _credential_workbook(tmp_xlsx) -> Path:
    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["B1"] = "s3cRet!99"
    path = tmp_xlsx(wb)
    wb.close()
    return path


def _rewrite(src: Path, name: str, edits: dict[str, object]) -> Path:
    """Copy ``src`` to ``name`` beside it. ``edits`` maps a part to new bytes, or
    to a ``(old, new)`` replacement applied once to that part's XML."""
    dst = src.with_name(name)
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w") as zout:
        present = set(zin.namelist())
        for item in zin.infolist():
            data = zin.read(item.filename)
            edit = edits.get(item.filename)
            if isinstance(edit, tuple):
                old, new = edit
                assert old in data, (item.filename, old)
                data = data.replace(old, new, 1)
            elif isinstance(edit, bytes):
                data = edit
            zout.writestr(item.filename, data)
        for part, data in edits.items():
            if part not in present:
                assert isinstance(data, bytes), part
                zout.writestr(part, data)
    return dst


def _assert_only_doormat_reads(path: Path) -> None:
    """openpyxl's own loader refuses the file; doormat still finds the credential."""
    with pytest.raises((KeyError, TypeError, ValueError)):
        openpyxl.load_workbook(path, data_only=False)
    sheets = extract_xlsx(path)
    assert [c[2] for c in sheets[0]] == ["Password", "s3cRet!99"]


def test_extract_workbook_ignores_broken_external_link(tmp_xlsx):
    # The link part exists but its relationships part is missing.
    src = _credential_workbook(tmp_xlsx)
    path = _rewrite(
        src,
        "links.xlsx",
        {
            "xl/_rels/workbook.xml.rels": (
                b"</Relationships>",
                (
                    b'<Relationship Id="rIdLink" Target="externalLinks/externalLink1.xml" '
                    b'Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/'
                    b'externalLink"/></Relationships>'
                ),
            ),
            "xl/workbook.xml": (
                b"</sheets>",
                f'</sheets><externalReferences><externalReference xmlns:r="{REL_NS}" '
                'r:id="rIdLink"/></externalReferences>'.encode(),
            ),
            "xl/externalLinks/externalLink1.xml": (
                '<externalLink xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" '
                f'xmlns:r="{REL_NS}"><externalBook r:id="rId1"/></externalLink>'.encode()
            ),
        },
    )
    _assert_only_doormat_reads(path)


def test_extract_workbook_ignores_invalid_print_titles(tmp_xlsx):
    src = _credential_workbook(tmp_xlsx)
    path = _rewrite(
        src,
        "titles.xlsx",
        {
            "xl/workbook.xml": (
                b"<definedNames />",
                (
                    b'<definedNames><definedName name="_xlnm.Print_Titles" '
                    b'localSheetId="0">#N/A</definedName></definedNames>'
                ),
            ),
        },
    )
    _assert_only_doormat_reads(path)


def test_extract_workbook_ignores_malformed_pivot_cache(tmp_xlsx):
    # A pivot cache with "-" where a number belongs, plus a pivot table using it.
    src = _credential_workbook(tmp_xlsx)
    main = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
    pivot_rel = "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotTable"
    cache_rel = (
        "http://schemas.openxmlformats.org/officeDocument/2006/relationships/pivotCacheDefinition"
    )
    path = _rewrite(
        src,
        "pivot.xlsx",
        {
            "xl/_rels/workbook.xml.rels": (
                b"</Relationships>",
                f'<Relationship Id="rIdCache" Target="pivotCache/pivotCacheDefinition1.xml" '
                f'Type="{cache_rel}"/></Relationships>'.encode(),
            ),
            "xl/workbook.xml": (
                b"</workbook>",
                f'<pivotCaches><pivotCache xmlns:r="{REL_NS}" cacheId="1" r:id="rIdCache"/>'
                "</pivotCaches></workbook>".encode(),
            ),
            "xl/pivotCache/pivotCacheDefinition1.xml": (
                f'<pivotCacheDefinition xmlns="{main}"><cacheSource type="worksheet"/>'
                '<cacheFields count="1"><cacheField name="Amount"><sharedItems>'
                '<n v="-"/></sharedItems></cacheField></cacheFields></pivotCacheDefinition>'.encode()
            ),
            "xl/worksheets/_rels/sheet1.xml.rels": (
                f'<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
                f'<Relationship Id="rId1" Target="../pivotTables/pivotTable1.xml" Type="{pivot_rel}"/>'
                "</Relationships>".encode()
            ),
            "xl/pivotTables/pivotTable1.xml": (
                f'<pivotTableDefinition xmlns="{main}" name="Pivot1" cacheId="1" dataCaption="Values">'
                '<location ref="D1:E2" firstHeaderRow="1" firstDataRow="1" firstDataCol="1"/>'
                "</pivotTableDefinition>".encode()
            ),
        },
    )
    _assert_only_doormat_reads(path)


def test_text_starting_with_equals_is_a_value_not_a_formula(tmp_xlsx):
    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["B1"] = "=k9Xq!fake"
    wb.active["B1"].data_type = "s"
    wb.active["C1"] = "=SUM(1,2)"
    p = tmp_xlsx(wb)
    wb.close()

    cells = {c[1]: c[2:4] for c in extract_xlsx(p)[0]}
    assert cells[2] == ("=k9Xq!fake", "")
    assert cells[3] == ("=SUM(1,2)", "=SUM(1,2)")


def test_extract_array_formula_as_text(tmp_xlsx):
    from openpyxl.worksheet.formula import ArrayFormula

    wb = openpyxl.Workbook()
    wb.active["A1"] = ArrayFormula("A1", "=SUM(B1:B2*C1:C2)")
    p = tmp_xlsx(wb)
    wb.close()

    cells = extract_xlsx(p)[0]
    assert [c[2:4] for c in cells] == [("=SUM(B1:B2*C1:C2)", "=SUM(B1:B2*C1:C2)")]


def test_xml_entity_declarations_are_rejected(tmp_xlsx):
    """A worksheet that declares XML entities must be refused, not expanded."""
    import doormat

    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["B1"] = "&e;"
    clean = tmp_xlsx(wb)
    wb.close()

    evil = clean.with_name("entities.xlsx")
    with zipfile.ZipFile(clean) as src, zipfile.ZipFile(evil, "w") as dst:
        for item in src.infolist():
            data = src.read(item.filename)
            if item.filename == "xl/worksheets/sheet1.xml":
                data = data.replace(
                    b"<worksheet", b'<!DOCTYPE worksheet [<!ENTITY e "x">]><worksheet', 1
                )
                assert b"<!DOCTYPE" in data
            dst.writestr(item, data)

    doc = doormat.load(evil)
    assert [(e.sheet, e.kind) for e in doc.errors] == [("Sheet", "EntitiesForbidden")]
    assert doc.credentials() == []
    assert not doc.complete


def test_xml_entity_declarations_in_workbook_structure_raise(tmp_xlsx):
    import doormat

    wb = openpyxl.Workbook()
    wb.active["A1"] = "x"
    clean = tmp_xlsx(wb)
    wb.close()

    evil = _rewrite(
        clean,
        "entities_wb.xlsx",
        {
            "xl/workbook.xml": (b"<workbook", b'<!DOCTYPE workbook [<!ENTITY e "x">]><workbook'),
        },
    )
    with pytest.raises(ValueError):
        doormat.load(evil)


# ---------- cells holding only a style ----------

_MAIN_NS = b"http://schemas.openxmlformats.org/spreadsheetml/2006/main"


def _with_sheet_data(tmp_xlsx, rows: bytes, prefix: bytes = b"") -> Path:
    """A workbook whose first sheet's XML is replaced by ``rows`` inside sheetData."""
    wb = openpyxl.Workbook()
    wb.active["A1"] = "x"
    clean = tmp_xlsx(wb)
    wb.close()
    p = prefix + b":" if prefix else b""
    xmlns = b"xmlns:" + prefix if prefix else b"xmlns"
    xml = (
        b"<"
        + p
        + b"worksheet "
        + xmlns
        + b'="'
        + _MAIN_NS
        + b'"><'
        + p
        + b"sheetData>"
        + rows
        + b"</"
        + p
        + b"sheetData></"
        + p
        + b"worksheet>"
    )
    return _rewrite(clean, "styled.xlsx", {"xl/worksheets/sheet1.xml": xml})


def _extract_counted(path: Path) -> tuple[list, int]:
    from doormat._limits import CellBudget

    budget = CellBudget(None, path.name)
    sheets = extract_xlsx(path, None, budget)
    return [(c[0], c[1], c[2]) for c in sheets[0]], budget.used


def test_style_only_cells_are_neither_parsed_nor_counted(tmp_xlsx):
    path = _with_sheet_data(
        tmp_xlsx,
        (
            b'<row r="1"><c r="A1" s="0"/><c r="B1" t="inlineStr"><is><t>Password</t></is></c>'
            b'<c r="C1" s="0"/></row><row r="2"><c r="A2" s="0"/>\n<c r="B2" s="0"/></row>'
        ),
    )
    assert _extract_counted(path) == ([(1, 2, "Password")], 1)


def test_cell_without_reference_after_a_removed_cell_keeps_its_column(tmp_xlsx):
    """openpyxl places a cell without ``r`` after the previous cell, so that one stays."""
    path = _with_sheet_data(
        tmp_xlsx,
        (
            b'<row r="1"><c r="A1" s="0"/><c r="B1" s="0"/><c t="inlineStr"><is><t>v</t></is></c></row>'
        ),
    )
    assert _extract_counted(path) == ([(1, 3, "v")], 2)


def test_style_only_cells_with_prefixed_tags_are_removed(tmp_xlsx):
    path = _with_sheet_data(
        tmp_xlsx,
        (
            b'<x:row r="1"><x:c r="A1" s="0"/><x:c r="B1" t="inlineStr"><x:is><x:t>v</x:t></x:is></x:c>'
            b"<x:c r='C1'/></x:row>"
        ),
        prefix=b"x",
    )
    assert _extract_counted(path) == ([(1, 2, "v")], 1)


def test_style_only_cells_do_not_change_extraction(tmp_xlsx):
    """A sheet formatted across a range reads the same cells as one without formatting."""
    from openpyxl.comments import Comment
    from openpyxl.styles import Font

    def build(styled: bool) -> openpyxl.Workbook:
        wb = openpyxl.Workbook()
        ws = wb.active
        if styled:
            for r in range(1, 40):
                for c in range(1, 12):
                    ws.cell(r, c).font = Font(bold=True)
        ws["B2"] = "Password"
        ws["C2"] = "s3cRet!99"
        ws["D5"] = 45000
        ws["E7"] = "=SUM(D1:D9)"
        ws["F9"].comment = Comment("note on an empty cell", "a")
        ws.merge_cells("G3:H4")
        return wb

    styled = _cells(tmp_xlsx, build(True))
    plain = _cells(tmp_xlsx, build(False))
    assert styled == plain
    assert [c[2] for c in styled] == ["Password", "s3cRet!99", "45000", "=SUM(D1:D9)", ""]


def test_cell_limit_ignores_style_only_cells(tmp_xlsx):
    import doormat
    from openpyxl.styles import Font

    wb = openpyxl.Workbook()
    for r in range(1, 101):
        for c in range(1, 21):
            wb.active.cell(r, c).font = Font(bold=True)
    wb.active["A1"] = "Password"
    wb.active["B1"] = "s3cRet!99"
    path = tmp_xlsx(wb)
    wb.close()
    assert [c.coordinate for c in doormat.load(path, max_cells=10).credentials(120)] == ["B1"]
