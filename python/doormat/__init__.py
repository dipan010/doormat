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

from importlib.metadata import version as _dist_version

from doormat._core import version
from doormat.api import GridDoc, Relationship, load

# The installed distribution's version in PEP 440 form (e.g. "0.2.0rc1"), which
# is what pip reports. version() returns the Rust core's Cargo form ("0.2.0-rc.1").
__version__ = _dist_version("doormat")

__all__ = ["GridDoc", "Relationship", "__version__", "load", "version"]
