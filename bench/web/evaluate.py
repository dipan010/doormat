"""Run doormat once on set B's test files and score it against the answer key (see PROTOCOL.md).

The test files are the test split plus the reserve families moved under the
minimum-yield rule. Each file goes through ``doormat.load()`` with its default
limits, as a user would call it. A finding at confidence >= 120 is a true
positive when its cell and the SHA-256 of its value match the key. Findings
are stored with value digests and masked shapes only.

Usage:
    python bench/web/evaluate.py --out NAME [--workers N]
    python bench/web/evaluate.py --report NAME
"""

from __future__ import annotations

import argparse
import collections
import csv
import hashlib
import json
import math
import multiprocessing as mp
import signal
import time
import warnings
from pathlib import Path

import doormat
from review import shape

WEB_DIR = Path(__file__).resolve().parent
DATA_DIR = WEB_DIR / "data" / "spreadsheets"
RESULTS_DIR = WEB_DIR / "results"
MIN_CONFIDENCE = 120

warnings.simplefilter("ignore", UserWarning)


class FileTimeout(Exception):
    """Raised by SIGALRM when a single file exceeds the per-file budget."""


def _on_alarm(_signum: int, _frame: object) -> None:
    raise FileTimeout


def test_files() -> list[str]:
    """The test split plus the files moved from the reserve."""
    with (WEB_DIR / "split_manifest.csv").open() as fh:
        files = [r["file"] for r in csv.DictReader(fh) if r["split"] == "test"]
    return files + (WEB_DIR / "yield_moved.txt").read_text().split()


def run_file(name: str) -> dict:
    """Load one file; never raises."""
    row: dict = {"file": name, "status": "ok", "findings": [], "errors": 0}
    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(300)
    start = time.perf_counter()
    try:
        doc = doormat.load(DATA_DIR / name)
        row["errors"] = len(doc.errors)
        row["findings"] = [
            {
                "sheet": r.sheet,
                "cell": [r.row, r.col],
                "header": [r.header_row, r.header_col],
                "reason": r.reason,
                "confidence": r.confidence,
                "key_shape": shape(r.key, 40),
                "value_shape": shape(r.value, 40),
                "value_sha256": hashlib.sha256(r.value.encode()).hexdigest(),
            }
            for r in doc.credentials(MIN_CONFIDENCE)
        ]
    except FileTimeout:
        row["status"] = "timeout"
    except Exception as exc:  # noqa: BLE001 - every failure is a reportable result
        row["status"] = f"error:{type(exc).__name__}"
    finally:
        signal.alarm(0)
        row["elapsed_ms"] = round((time.perf_counter() - start) * 1000, 2)
    return row


def wilson(k: int, n: int) -> str:
    """95% Wilson interval for k successes out of n, as a percentage range."""
    if n == 0:
        return "n/a"
    z, p = 1.96, k / n
    centre = (p + z * z / (2 * n)) / (1 + z * z / n)
    half = z * math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / (1 + z * z / n)
    return f"{100 * max(0.0, centre - half):.1f}-{100 * min(1.0, centre + half):.1f}%"


def report(name: str) -> None:
    """Score a run against the answer key and print the metrics the protocol asks for."""
    rows = [json.loads(line) for line in (RESULTS_DIR / name).open()]
    key = json.loads((WEB_DIR / "answer_key_test.json").read_text())["credentials"]
    truth = {(k["file"], k["sheet"].removesuffix("[HIDDEN]"), *k["cell"]): k for k in key}
    found = set()
    tp, fp = [], []
    for row in rows:
        for f in row["findings"]:
            k = truth.get((row["file"], f["sheet"], *f["cell"]))
            if k and k["value_sha256"] == f["value_sha256"]:
                tp.append((row["file"], f))
                found.add((row["file"], f["sheet"], *f["cell"]))
            else:
                fp.append((row["file"], f, bool(k)))
    findings = len(tp) + len(fp)
    statuses = collections.Counter(r["status"] for r in rows)
    times = sorted(r["elapsed_ms"] for r in rows)
    print(f"files {len(rows)}: {dict(statuses)}; files with sheet errors: {sum(r['errors'] > 0 for r in rows)}")
    print(f"time per file: median {times[len(times) // 2]:.0f} ms, p99 {times[int(len(times) * 0.99)]:.0f} ms")
    print(f"findings >= {MIN_CONFIDENCE}: {findings} in {sum(bool(r['findings']) for r in rows)} files")
    print(f"precision: {len(tp)}/{findings} ({wilson(len(tp), findings)})")
    print(f"recall: {len(found)}/{len(key)} ({wilson(len(found), len(key))})")
    for kind in sorted({k["kind"] for k in key}):
        ks = [t for t, k in truth.items() if k["kind"] == kind]
        print(f"  {kind}: {sum(t in found for t in ks)}/{len(ks)}")
    by_file = collections.Counter(t[0] for t in truth)
    big, n_big = by_file.most_common(1)[0]
    rest = [t for t in truth if t[0] != big]
    print(f"largest file holds {n_big}/{len(key)}; without it recall {sum(t in found for t in rest)}/{len(rest)}")
    print("false positives (masked):")
    for file, f, same_cell in fp:
        print(f"  {file[:8]} [{f['sheet'][:18]}] r{f['cell'][0]}c{f['cell'][1]} {f['reason']} {f['confidence']}"
              f" key={f['key_shape']} value={f['value_shape']}{' (key cell, other value)' if same_cell else ''}")


def main() -> None:
    """Run the test files once, or report on a finished run."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--out")
    parser.add_argument("--report")
    parser.add_argument("--workers", type=int, default=4)
    opts = parser.parse_args()
    if opts.report:
        report(opts.report)
        return
    RESULTS_DIR.mkdir(exist_ok=True)
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool, (RESULTS_DIR / opts.out).open("w") as out:
        for row in pool.imap_unordered(run_file, test_files(), chunksize=8):
            out.write(json.dumps(row) + "\n")
    print(f"doormat {doormat.__version__} ({doormat.__file__}) -> {RESULTS_DIR / opts.out}")


if __name__ == "__main__":
    main()
