"""Print aggregate statistics for results/scan.jsonl and results/pool.jsonl.

Output contains counts and timings only, never cell values, so it is safe to
paste into reports.

Usage:
    python bench/corpus/summarize.py
"""

from __future__ import annotations

import json
import statistics
from collections import Counter

from scan import RESULTS_DIR


def _load(name: str) -> list[dict]:
    path = RESULTS_DIR / name
    if not path.exists():
        return []
    with path.open() as f:
        return [json.loads(line) for line in f]


def _reason_kind(reason: str) -> str:
    """Collapse a reason string to its detection pathway."""
    for kind in ("inline_same_cell", "formula", "comment", "split_password"):
        if kind in reason:
            return kind
    return "spatial" if reason.startswith("distance") else "other"


def main() -> None:
    """Print robustness, throughput, finding and pool aggregates."""
    scan = _load("scan.jsonl")
    print(f"files scanned: {len(scan)}")
    for status, n in Counter(r["status"] for r in scan).most_common():
        print(f"  {status:<28} {n:>6}  ({n / len(scan):.1%})")

    ok = [r for r in scan if r["status"] == "ok"]
    times = sorted(r["elapsed_ms"] for r in ok)
    cells = sum(r["cells"] for r in ok)
    print(f"cells: {cells:,}  median ms/file: {statistics.median(times):.0f}  "
          f"p95: {times[int(len(times) * 0.95)]:.0f}  max: {times[-1]:.0f}")

    findings = [(r, f) for r in ok for f in r["findings"]]
    flagged = sum(1 for r in ok if r["findings"])
    print(f"findings: {len(findings)} in {flagged} files "
          f"({flagged / len(ok) * 1000:.1f} flagged per 1k files)")
    for split in ("dev", "test"):
        n = sum(1 for r, _ in findings if r["split"] == split)
        print(f"  {split}: {n}")
    for kind, n in Counter(_reason_kind(f["reason"]) for _, f in findings).most_common():
        print(f"  {kind:<20} {n}")
    bands = Counter(min(int(f["confidence"]) // 50 * 50, 300) for _, f in findings)
    print("  confidence bands: " + ", ".join(f"{b}+: {bands[b]}" for b in sorted(bands)))
    keys = Counter(f["key"].strip().lower()[:30] for _, f in findings)
    print("  top keys: " + ", ".join(f"{k!r}: {n}" for k, n in keys.most_common(12)))

    pool = _load("pool.jsonl")
    if pool:
        hits = [h for r in pool for h in r["hits"]]
        print(f"pool: {len(hits)} keyword cells in {sum(1 for r in pool if r['hits'])} files")
        for kind, n in Counter((h["kind"], h["field"]) for h in hits).most_common():
            print(f"  {kind}: {n}")


if __name__ == "__main__":
    main()
