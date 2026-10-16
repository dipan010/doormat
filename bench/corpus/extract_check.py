"""Check that a change to the .xlsx extractor leaves its output unchanged, and time it.

``run`` extracts every .xlsx/.xlsm file in the corpus and writes one JSONL row
per file: a SHA-256 digest of the extracted cell tuples and sheet errors, the
number of cells kept, the number parsed (what ``load(max_cells=...)`` counts)
and the time taken. Nothing else from the file is stored. ``compare`` reports
every file whose digest or status differs between two runs, and the timings.

Usage:
    python bench/corpus/extract_check.py run --out NAME [--workers N] [--files LIST]
    python bench/corpus/extract_check.py compare OLD NEW
"""

from __future__ import annotations

import argparse
import hashlib
import json
import multiprocessing as mp
import signal
import time
import warnings
from pathlib import Path

from doormat._limits import CellBudget
from doormat.extract import extract_xlsx

CORPUS_DIR = Path(__file__).resolve().parent
DATA_DIR = CORPUS_DIR / "data" / "spreadsheets"
RESULTS_DIR = CORPUS_DIR / "results"

# openpyxl warns on malformed print areas and headers; they do not affect cells
warnings.simplefilter("ignore", UserWarning)


class FileTimeout(Exception):
    """Raised by SIGALRM when a single file exceeds the per-file budget."""


def _on_alarm(_signum: int, _frame: object) -> None:
    raise FileTimeout


def check_file(args: tuple[str, int]) -> dict:
    """Extract one file and digest the result. Never raises; failures become a status."""
    path_str, timeout = args
    path = Path(path_str)
    row: dict = {"file": path.name, "bytes": path.stat().st_size, "status": "ok"}
    errors: list[tuple[str, str]] = []
    budget = CellBudget(None, path.name)
    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(timeout)
    start = time.perf_counter()
    try:
        sheets = extract_xlsx(
            path, lambda sheet, exc: errors.append((sheet, type(exc).__name__)), budget
        )
        digest = hashlib.sha256(repr((sheets, errors)).encode()).hexdigest()
        row.update(digest=digest, cells=sum(len(s) for s in sheets), parsed=budget.used)
    except FileTimeout:
        row["status"] = "timeout"
    except Exception as exc:  # noqa: BLE001 - every failure is a reportable result
        row["status"] = f"error:{type(exc).__name__}"
    finally:
        signal.alarm(0)
        row["elapsed_ms"] = round((time.perf_counter() - start) * 1000, 2)
    return row


def run(opts: argparse.Namespace) -> None:
    """Extract every .xlsx/.xlsm file in parallel and write results/NAME."""
    files = sorted(p for p in DATA_DIR.iterdir() if p.suffix.lower() in (".xlsx", ".xlsm"))
    if opts.files:
        wanted = set(opts.files.read_text().split("\n")) - {""}
        files = [p for p in files if p.name in wanted]
    RESULTS_DIR.mkdir(exist_ok=True)
    out_path = RESULTS_DIR / opts.out
    start = time.perf_counter()
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool, out_path.open("w") as out:
        jobs = ((str(p), opts.timeout) for p in files)
        for done, row in enumerate(pool.imap_unordered(check_file, jobs, chunksize=8), 1):
            out.write(json.dumps(row) + "\n")
            if done % 2000 == 0:
                print(f"{done}/{len(files)} files", flush=True)
    print(f"{len(files)} files in {time.perf_counter() - start:.1f}s -> {out_path}")


def load_rows(name: str) -> dict[str, dict]:
    """Read a run's rows keyed by file name."""
    with (RESULTS_DIR / name).open() as fh:
        return {row["file"]: row for row in map(json.loads, fh)}


def compare(opts: argparse.Namespace) -> None:
    """Report output differences and timings between two runs."""
    old, new = load_rows(opts.old), load_rows(opts.new)
    common = sorted(old.keys() & new.keys())
    differ = [
        f
        for f in common
        if (old[f]["status"], old[f].get("digest")) != (new[f]["status"], new[f].get("digest"))
    ]
    print(f"files: {len(common)} in both, {len(old.keys() ^ new.keys())} in one run only")
    print(f"output differs: {len(differ)}")
    for f in differ:
        print(f"  {f}: {old[f]['status']} -> {new[f]['status']}")
    old_s = sum(old[f]["elapsed_ms"] for f in common) / 1000
    new_s = sum(new[f]["elapsed_ms"] for f in common) / 1000
    print(f"summed time: {old_s:.0f}s -> {new_s:.0f}s ({(new_s - old_s) / old_s:+.0%})")
    limit = 2_000_000
    old_over = sum(old[f].get("parsed", 0) > limit for f in common)
    new_over = sum(new[f].get("parsed", 0) > limit for f in common)
    print(f"files parsing more than {limit:,} cells: {old_over} -> {new_over}")
    print("slowest 10 (old run):     MB  kept cells   parsed old ->         new   seconds")
    for f in sorted(common, key=lambda f: -old[f]["elapsed_ms"])[:10]:
        o, n = old[f], new[f]
        print(
            f"  {o['bytes'] / 1e6:6.1f} {o.get('cells', 0):>11,}"
            f" {o.get('parsed', 0):>11,} -> {n.get('parsed', 0):>11,}"
            f"  {o['elapsed_ms'] / 1000:6.2f} -> {n['elapsed_ms'] / 1000:6.2f}"
        )


def main() -> None:
    """Parse arguments and dispatch to ``run`` or ``compare``."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    sub = parser.add_subparsers(dest="command", required=True)
    run_p = sub.add_parser("run")
    run_p.add_argument("--out", required=True)
    run_p.add_argument("--workers", type=int, default=4)
    run_p.add_argument("--timeout", type=int, default=600)
    run_p.add_argument("--files", type=Path, help="newline-separated file names to check")
    cmp_p = sub.add_parser("compare")
    cmp_p.add_argument("old")
    cmp_p.add_argument("new")
    opts = parser.parse_args()
    if opts.command == "run":
        run(opts)
    else:
        compare(opts)


if __name__ == "__main__":
    main()
