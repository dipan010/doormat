"""Apply set B's minimum-yield rule: move reserve families to test in split order (see PROTOCOL.md).

Families are ordered as ``split.py`` ordered them, by the SHA-256 of the full
family key; a file that was unreadable when the split was drawn keeps its key
``file:<digest>``. Families are moved until the test answer key holds 30
credentials. The moved files are written to ``results/yield_moved.txt``.

Usage:
    python bench/web/yield_rule.py
"""

from __future__ import annotations

import csv
import hashlib
import multiprocessing as mp

from split import DATA_DIR, MANIFEST, WEB_DIR, fingerprint

MINIMUM = 30
TEST_CREDENTIALS = 1
# Credentials labelled in reserve files during the review, by file-name prefix.
RESERVE_CREDENTIALS = {"169e2559": 1, "33033325": 1, "5706a5b0": 58, "0556fa17": 166}


def main() -> None:
    """Order the reserve families, move them until the key holds MINIMUM, report the cut-off."""
    with MANIFEST.open() as fh:
        rows = {r["file"]: r for r in csv.DictReader(fh) if r["split"] == "reserve"}
    with mp.Pool(4) as pool:
        out = pool.map(fingerprint, [str(DATA_DIR / name) for name in rows], chunksize=16)
    families: dict[str, list[str]] = {}
    for name, digest, family in out:
        if rows[name]["family"] == rows[name]["sha256"]:  # unreadable when the split was drawn
            family = f"file:{digest}"
        families.setdefault(family, []).append(name)
    order = sorted(families, key=lambda f: hashlib.sha256(f.encode()).hexdigest())
    total, moved = TEST_CREDENTIALS, []
    for index, family in enumerate(order, 1):
        moved += families[family]
        total += sum(n for prefix, n in RESERVE_CREDENTIALS.items() for f in families[family] if f.startswith(prefix))
        if total >= MINIMUM:
            break
    (WEB_DIR / "results" / "yield_moved.txt").write_text("\n".join(moved) + "\n")
    print(f"{len(order)} reserve families; moved {index} ({len(moved)} files); test key holds {total}")


if __name__ == "__main__":
    main()
