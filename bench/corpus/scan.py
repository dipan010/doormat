"""Run gridmap over every spreadsheet in the corpus and record one JSONL row per file.

Each sheet is sent through ``gridmap._core.process_sheet`` separately so that
findings can be mapped back to (sheet, row, col). ``gridmap.load()`` returns
only internal per-sheet cell indices, which are ambiguous across sheets.

Usage:
    python bench/corpus/scan.py [--workers N] [--timeout SECONDS] [--limit N]
                                [--split test|dev] [--out NAME] [--files LIST]
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

from gridmap import _core
from gridmap.api import _get_format_registry

CORPUS_DIR = Path(__file__).resolve().parent
DATA_DIR = CORPUS_DIR / "data" / "spreadsheets"
RESULTS_DIR = CORPUS_DIR / "results"

# openpyxl warns on malformed print areas and headers; they do not affect cells
warnings.simplefilter("ignore", UserWarning)


class FileTimeout(Exception):
    """Raised by SIGALRM when a single file exceeds the per-file budget."""


def _on_alarm(_signum: int, _frame: object) -> None:
    raise FileTimeout


def split_of(md5: str) -> str:
    """Return the deterministic split for a file hash: ``test`` or ``dev``."""
    return "test" if int(md5[:8], 16) % 5 == 0 else "dev"


def _coords_by_id(cells: list[tuple]) -> list[tuple[int, int]]:
    """Mirror CellStore::from_raw id assignment: first occurrence of (row, col)."""
    seen: set[tuple[int, int]] = set()
    coords: list[tuple[int, int]] = []
    for cell in cells:
        rc = (cell[0], cell[1])
        if rc not in seen:
            seen.add(rc)
            coords.append(rc)
    return coords


def scan_file(args: tuple[str, int, str]) -> dict:
    """Extract and score one file. Never raises; failures become a status."""
    path_str, timeout, only_split = args
    path = Path(path_str)
    raw = path.read_bytes()
    md5 = hashlib.md5(raw).hexdigest()
    row: dict = {
        "file": path.name,
        "md5": md5,
        "split": split_of(md5),
        "bytes": len(raw),
        "status": "ok",
        "sheets": 0,
        "cells": 0,
        "elapsed_ms": 0.0,
        "findings": [],
    }
    if only_split and row["split"] != only_split:
        row["status"] = "other_split"
        return row
    registry = _get_format_registry()
    ext = path.suffix.lower()
    if ext not in registry:
        row["status"] = "unsupported_ext"
        return row
    magic, extractor = registry[ext]
    if magic is not None and not raw.startswith(magic):
        row["status"] = "bad_magic"
        return row

    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(timeout)
    start = time.perf_counter()
    try:
        sheets = extractor(path)
        row["sheets"] = len(sheets)
        row["cells"] = sum(len(s) for s in sheets)
        for cells in sheets:
            if not cells:
                continue
            coords = _coords_by_id(cells)
            sheet_name = cells[0][5]
            for rel in _core.process_sheet(cells):
                hr, hc = coords[rel["header_cell_id"]]
                vr, vc = coords[rel["value_cell_id"]]
                row["findings"].append(
                    {
                        "sheet": sheet_name,
                        "header": [hr, hc],
                        "cell": [vr, vc],
                        "key": rel["key"],
                        "value": rel["value"],
                        "confidence": rel["confidence"],
                        "reason": rel["reason"],
                    }
                )
    except FileTimeout:
        row["status"] = "timeout"
    except Exception as exc:  # noqa: BLE001 - every failure is a reportable result
        row["status"] = f"error:{type(exc).__name__}"
        row["error"] = str(exc)[:200]
    finally:
        signal.alarm(0)
        row["elapsed_ms"] = round((time.perf_counter() - start) * 1000, 2)
    return row


def main() -> None:
    """Scan the corpus in parallel and write results/scan.jsonl."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workers", type=int, default=mp.cpu_count())
    parser.add_argument("--timeout", type=int, default=60)
    parser.add_argument("--limit", type=int, default=0)
    parser.add_argument("--split", choices=("test", "dev"), default="")
    parser.add_argument("--out", default="scan.jsonl")
    parser.add_argument("--files", type=Path, help="newline-separated file names to scan")
    opts = parser.parse_args()

    files = sorted(p for p in DATA_DIR.rglob("*") if p.is_file())
    if opts.files:
        wanted = set(opts.files.read_text().split("\n")) - {""}
        files = [p for p in files if p.name in wanted]
    if opts.limit:
        files = files[: opts.limit]
    RESULTS_DIR.mkdir(exist_ok=True)
    out_path = RESULTS_DIR / opts.out

    start = time.perf_counter()
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool, out_path.open("w") as out:
        jobs = ((str(p), opts.timeout, opts.split) for p in files)
        for done, row in enumerate(pool.imap_unordered(scan_file, jobs, chunksize=8), 1):
            if row["status"] != "other_split":
                out.write(json.dumps(row) + "\n")
            if done % 1000 == 0:
                print(f"{done}/{len(files)} files", flush=True)
    print(f"{len(files)} files in {time.perf_counter() - start:.1f}s -> {out_path}")


if __name__ == "__main__":
    main()
