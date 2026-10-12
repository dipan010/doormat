"""Smoke-test an installed doormat: find a planted credential at the right cell.

Used by the release workflow on every platform, both on freshly built wheels
and after publishing. Prints the version and platform; exits non-zero on failure.
"""

import pathlib
import platform
import sys
import tempfile

import openpyxl

import doormat

wb = openpyxl.Workbook()
wb.active["A1"] = "Password"
wb.active["B1"] = "s3cret!!"
path = pathlib.Path(tempfile.mkdtemp()) / "smoke.xlsx"
wb.save(path)

found = doormat.load(path).credentials(min_confidence=120)
assert [(c.sheet, c.coordinate, c.value) for c in found] == [("Sheet", "B1", "s3cret!!")], found
print(f"doormat {doormat.__version__} OK on {platform.system()} {platform.machine()}, Python {sys.version.split()[0]}")
