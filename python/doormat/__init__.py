"""doormat: find passwords and secrets hidden in spreadsheets.

Reads .xlsx, .xlsm, .xls, .ods, .csv and .tsv files, models each sheet as a
2D grid, and pairs credential labels with the values around them. Detection
runs in a Rust core via PyO3.

Quick start::

    import doormat

    doc = doormat.load("workbook.xlsx")
    for cred in doc.credentials(min_confidence=120):
        print(f"{cred.sheet}!{cred.coordinate}: {cred.key} = {cred.value}")
"""

from doormat._core import version
from doormat.api import GridDoc, Relationship, load

__version__ = version()

__all__ = ["GridDoc", "Relationship", "__version__", "load", "version"]
