"""Score a scan of the test split against the committed labels and credential list.

Precision uses ``labels_test.jsonl``: each finding is joined on
(md5, sheet, value cell, value SHA-256). Findings with no label are counted
and listed so they can be labelled before the numbers are reported.

Recall uses ``creds_test.json``: the credential cells found by reviewing the
keyword pool (see README). A credential counts as found when a finding's value
cell is that cell. ``split_password`` findings never count, because their value
is a concatenation rather than the credential itself.

Output contains counts only, never cell values.

Usage:
    python bench/corpus/evaluate.py results/scan.jsonl [--min-confidence 120]
"""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from pathlib import Path

CORPUS_DIR = Path(__file__).resolve().parent
LABELS_PATH = CORPUS_DIR / "labels_test.jsonl"
CREDS_PATH = CORPUS_DIR / "creds_test.json"


def _pathway(reason: str) -> str:
    """Collapse a reason string to its detection pathway."""
    for kind in ("inline_same_cell", "formula", "comment", "split_password"):
        if kind in reason:
            return kind
    return "spatial"


def _key(md5: str, sheet: str, cell: list[int], value_sha256: str) -> tuple:
    return (md5, sheet, cell[0], cell[1], value_sha256)


def main() -> None:
    """Print precision by pathway and recall by credential kind."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("scan", type=Path)
    parser.add_argument("--min-confidence", type=float, default=120.0)
    opts = parser.parse_args()

    with LABELS_PATH.open() as f:
        labels = {
            _key(r["md5"], r["sheet"], r["cell"], r["value_sha256"]): r["label"]
            for r in map(json.loads, f)
        }
    creds = {(c["md5"], c["sheet"], c["cell"][0], c["cell"][1]): c["kind"] for c in json.loads(CREDS_PATH.read_text())}

    with opts.scan.open() as f:
        rows = [r for r in map(json.loads, f) if r["split"] == "test"]
    findings = [
        (r["md5"], f)
        for r in rows
        for f in r["findings"]
        if f["confidence"] >= opts.min_confidence
    ]

    by_path: dict[str, Counter] = {}
    unlabelled = []
    found: set[tuple] = set()
    for md5, f in findings:
        sha = hashlib.sha256(f["value"].encode()).hexdigest()
        label = labels.get(_key(md5, f["sheet"], f["cell"], sha))
        path = _pathway(f["reason"])
        by_path.setdefault(path, Counter())[label or "unlabelled"] += 1
        if label is None:
            unlabelled.append((md5[:8], f["sheet"], f["cell"], path))
        loc = (md5, f["sheet"], f["cell"][0], f["cell"][1])
        if loc in creds and path != "split_password":
            found.add(loc)

    total = Counter()
    for path, counts in sorted(by_path.items()):
        total.update(counts)
        judged = counts["tp"] + counts["fp"]
        p = counts["tp"] / judged if judged else 0.0
        print(f"  {path:<18} tp {counts['tp']:>4}  fp {counts['fp']:>4}  unlabelled {counts['unlabelled']:>3}  P={p:.2f}")
    judged = total["tp"] + total["fp"]
    print(f"findings: {len(findings)}  precision: {total['tp'] / judged if judged else 0:.3f} "
          f"({total['tp']}/{judged}, {total['unlabelled']} unlabelled)")

    kinds = Counter(creds.values())
    hit = Counter(creds[loc] for loc in found)
    print(f"recall: {len(found) / len(creds):.3f} ({len(found)}/{len(creds)} credentials)")
    for kind in sorted(kinds):
        print(f"  {kind:<10} {hit[kind]:>3}/{kinds[kind]}")
    for item in unlabelled:
        print("UNLABELLED", *item)


if __name__ == "__main__":
    main()
