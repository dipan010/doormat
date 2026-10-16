"""Score doormat on evaluation set A2 (see PROTOCOL.md).

A finding at confidence >= 120 is a true positive when its sheet, cell and
value match a credential in the file's ``.truth.json``; anything else is a
false positive, reported by the label it was paired with so that the
hard-negative variables (``*_PASSWORD_FILE``, ``*_RANDOM_ROOT_PASSWORD``, ...)
are visible.

Usage:
    python bench/configs/evaluate.py design holdout
    python bench/configs/evaluate.py test        # once, with a frozen detector
"""

from __future__ import annotations

import argparse
import json
from collections import Counter, defaultdict
from pathlib import Path

import doormat

DATA_DIR = Path(__file__).resolve().parent / "data"
MIN_CONFIDENCE = 120.0


def score_file(path: Path) -> dict:
    """Compare one file's findings with its truth."""
    truth = json.loads(path.with_name(path.name + ".truth.json").read_text())
    creds = {(c["sheet"], c["row"], c["col"]): c["value"] for c in truth["credentials"]}
    found: set = set()
    false_positives: list[str] = []
    for f in doormat.load(path).credentials(MIN_CONFIDENCE):
        key = (f.sheet, f.row, f.col)
        if creds.get(key) == f.value:
            found.add(key)
        else:
            false_positives.append(f.key.strip())
    return {"truth": truth, "creds": creds, "found": found, "fp": false_positives}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("splits", nargs="+", choices=["design", "holdout", "test"])
    opts = parser.parse_args()
    for split in opts.splits:
        files = sorted(p for p in (DATA_DIR / split).rglob("*") if p.suffix in (".csv", ".xlsx"))
        rows = [score_file(p) for p in files]
        tp = sum(len(r["found"]) for r in rows)
        fp = sum(len(r["fp"]) for r in rows)
        total = sum(len(r["creds"]) for r in rows)
        values = [(k in r["found"], v) for r in rows for k, v in r["creds"].items()]
        print(f"== {split}: {len(files)} files")
        print(f"  recall {tp}/{total} cells, {len({v for h, v in values if h})}/{len({v for _, v in values})} distinct values")
        print(f"  precision {tp}/{tp + fp}" + (f" = {tp / (tp + fp):.1%}" if tp + fp else ""))
        for group in ("family", "layout"):
            agg: dict[str, list[int]] = defaultdict(lambda: [0, 0, 0])
            for r in rows:
                a = agg[r["truth"][group]]
                a[0] += len(r["found"])
                a[1] += len(r["creds"])
                a[2] += len(r["fp"])
            print(f"  by {group}:")
            for name, (f_, t_, p_) in sorted(agg.items()):
                print(f"    {name:15s} found {f_:4d}/{t_:<4d} false positives {p_}")
        labels = Counter(label for r in rows for label in r["fp"])
        if labels:
            print("  false positives by label:")
            for label, n in labels.most_common(15):
                print(f"    {n:4d}  {label[:60]}")


if __name__ == "__main__":
    main()
