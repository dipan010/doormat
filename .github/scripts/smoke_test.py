"""Smoke-test an installed doormat: find a planted credential at the right cell.

Used by the release workflow on every platform, both on freshly built wheels
and after publishing. The workbook is written with the standard library, since
doormat installs no other packages. Prints the version and platform; exits
non-zero on failure.
"""

import pathlib
import platform
import sys
import tempfile
import zipfile

import doormat

MAIN = "http://schemas.openxmlformats.org/spreadsheetml/2006/main"
REL = "http://schemas.openxmlformats.org/officeDocument/2006/relationships"
PARTS = {
    "[Content_Types].xml": (
        '<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">'
        '<Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>'
        '<Default Extension="xml" ContentType="application/xml"/>'
        '<Override PartName="/xl/workbook.xml" '
        'ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>'
        "</Types>"
    ),
    "xl/workbook.xml": (
        f'<workbook xmlns="{MAIN}" xmlns:r="{REL}"><sheets>'
        '<sheet name="Sheet" sheetId="1" r:id="rId1"/></sheets></workbook>'
    ),
    "xl/_rels/workbook.xml.rels": (
        '<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">'
        f'<Relationship Id="rId1" Type="{REL}/worksheet" Target="worksheets/sheet1.xml"/>'
        "</Relationships>"
    ),
    "xl/worksheets/sheet1.xml": (
        f'<worksheet xmlns="{MAIN}"><sheetData><row r="1">'
        '<c r="A1" t="inlineStr"><is><t>Password</t></is></c>'
        '<c r="B1" t="inlineStr"><is><t>s3cret!!</t></is></c>'
        "</row></sheetData></worksheet>"
    ),
}

path = pathlib.Path(tempfile.mkdtemp()) / "smoke.xlsx"
with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
    for name, xml in PARTS.items():
        zf.writestr(name, xml)

found = doormat.load(path).credentials(min_confidence=120)
assert [(c.sheet, c.coordinate, c.value) for c in found] == [("Sheet", "B1", "s3cret!!")], found
print(
    f"doormat {doormat.__version__} OK on {platform.system()} {platform.machine()}, Python {sys.version.split()[0]}"
)
