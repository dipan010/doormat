"""doormat --- spatial document graph engine for credential detection.

Detects credentials (passwords, tokens, secrets) stored in xlsx
spreadsheet files by analyzing spatial proximity, content features,
and scoring heuristics. Powered by a Rust core via PyO3.

Quick start::

    import doormat

    doc = doormat.load("workbook.xlsx")
    for cred in doc.credentials(min_confidence=120):
        print(f"{cred.key} = {cred.value}")
"""

from doormat._core import version
from doormat.api import GridDoc, Relationship, load

__all__ = ["GridDoc", "Relationship", "load", "version"]
