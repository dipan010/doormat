"""Score doormat on evaluation set A (see PROTOCOL.md).

A finding at confidence >= 120 is a true positive when its sheet, cell and
value match a credential in the file's ``.truth.json``; anything else is a
false positive. Recall is reported for passwords (primary metric) and TOTP
secrets (secondary), by cell and by distinct value.

Usage:
    python bench/exports/evaluate.py design holdout
    python bench/exports/evaluate.py test        # once, with a frozen detector
"""

from __future__ import annotations

import argparse
import json
from collections import defaultdict
from pathlib import Path
from urllib.parse import parse_qs, urlparse

import doormat

DATA_DIR = Path(__file__).resolve().parent / "data"
MIN_CONFIDENCE = 120.0


def _otpauth_secret(value: str) -> str | None:
    """The ``secret`` parameter of an ``otpauth://`` URI (protocol amendment 1)."""
    if not value.startswith("otpauth://"):
        return None
    query = parse_qs(urlparse(value).query)
    return query.get("secret", [None])[0]


def score_file(path: Path) -> dict:
    """Compare one file's findings with its truth."""
    truth = json.loads(path.with_name(path.name + ".truth.json").read_text())
    creds = {(c["sheet"], c["row"], c["col"]): c for c in truth["credentials"]}
    doc = doormat.load(path)
    found, fp = set(), 0
    for f in doc.credentials(MIN_CONFIDENCE):
        key = (f.sheet, f.row, f.col)
        if key in creds and f.value in (creds[key]["value"], _otpauth_secret(creds[key]["value"])):
            found.add(key)
        else:
            fp += 1
    return {"truth": truth, "creds": creds, "found": found, "fp": fp, "errors": len(doc.errors)}


def report(rows: list[dict], group: str) -> None:
    """Print recall and false positives grouped by a truth field."""
    agg: dict[str, list[int]] = defaultdict(lambda: [0, 0, 0, 0, 0])  # pw found, pw total, totp found, totp total, fp
    for r in rows:
        a = agg[r["truth"][group]]
        for key, c in r["creds"].items():
            i = 0 if c["kind"] == "password" else 2
            a[i] += key in r["found"]
            a[i + 1] += 1
        a[4] += r["fp"]
    print(f"\n  by {group}:")
    for name, (pf, pt, tf, tt, fp) in sorted(agg.items()):
        print(f"    {name:18s} passwords {pf:4d}/{pt:<4d} totp {tf:3d}/{tt:<3d} false positives {fp}")


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("splits", nargs="+", choices=["design", "holdout", "test"])
    opts = parser.parse_args()
    for split in opts.splits:
        files = sorted(p for p in (DATA_DIR / split).rglob("*") if p.suffix in (".csv", ".xlsx"))
        rows = [score_file(p) for p in files]
        tp = sum(len(r["found"]) for r in rows)
        fp = sum(r["fp"] for r in rows)
        pw = [(k in r["found"], c["value"]) for r in rows for k, c in r["creds"].items() if c["kind"] == "password"]
        distinct = {v for _, v in pw}
        distinct_found = {v for hit, v in pw if hit}
        neg = [r for r in rows if r["truth"]["family"] == "negative"]
        print(f"== {split}: {len(files)} files, {sum(r['errors'] for r in rows)} sheet errors")
        print(f"  password recall {sum(h for h, _ in pw)}/{len(pw)} cells, {len(distinct_found)}/{len(distinct)} distinct values")
        print(f"  precision {tp}/{tp + fp}" + (f" = {tp / (tp + fp):.1%}" if tp + fp else ""))
        print(f"  negative controls: {sum(r['fp'] for r in neg)} false positives in {len(neg)} files")
        for group in ("grade", "family", "variant"):
            report(rows, group)


if __name__ == "__main__":
    main()
