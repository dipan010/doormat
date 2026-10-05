"""Build the recall candidate pool without using gridmap's detection pipeline.

Every cell of every corpus file is matched against plain credential keywords and
inline ``key: value`` patterns. Matching cells are written to results/pool.jsonl
with their location. Labelling that pool gives the recall denominator: the
credentials a reviewer could find by keyword search, independent of gridmap's
header sets, scoring and thresholds.

Only gridmap's extractors are shared (to read the files the same way); nothing
from the Rust core is called.

Usage:
    python bench/corpus/keyword_pool.py [--workers N] [--timeout SECONDS]
"""

from __future__ import annotations

import argparse
import hashlib
import json
import multiprocessing as mp
import re
import signal
from pathlib import Path

from gridmap.api import _get_format_registry

from scan import DATA_DIR, RESULTS_DIR, FileTimeout, _on_alarm, split_of

KEYWORD = re.compile(
    r"\b(pass ?words?|passwd|pass ?codes?|pass ?phrases?|pwd|pw|pins?|log ?ins?|"
    r"log ?ons?|user ?names?|user ?ids?|credentials?|secrets?|tokens?|"
    r"api ?keys?|access codes?|security codes?)\b",
    re.IGNORECASE,
)
INLINE = re.compile(
    r"\b(password|passwd|pwd|pw|pin|passcode|login|user ?(?:name|id))\s*[:=]\s*\S+",
    re.IGNORECASE,
)


def pool_file(args: tuple[str, int]) -> dict:
    """Return every keyword-matching cell in one file. Never raises."""
    path_str, timeout = args
    path = Path(path_str)
    raw = path.read_bytes()
    md5 = hashlib.md5(raw).hexdigest()
    row: dict = {"file": path.name, "md5": md5, "split": split_of(md5), "status": "ok", "hits": []}
    registry = _get_format_registry()
    entry = registry.get(path.suffix.lower())
    if entry is None or (entry[0] is not None and not raw.startswith(entry[0])):
        row["status"] = "skipped"
        return row

    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(timeout)
    try:
        for cells in entry[1](path):
            for r, c, value, formula, comment, sheet, _merged in cells:
                for field, text in (("value", value), ("formula", formula), ("comment", comment)):
                    if not text:
                        continue
                    kind = "inline" if INLINE.search(text) else "keyword" if KEYWORD.search(text) else None
                    if kind:
                        row["hits"].append(
                            {"sheet": sheet, "cell": [r, c], "field": field, "kind": kind, "text": text[:300]}
                        )
    except FileTimeout:
        row["status"] = "timeout"
    except Exception as exc:  # noqa: BLE001 - failures are reported, not fatal
        row["status"] = f"error:{type(exc).__name__}"
    finally:
        signal.alarm(0)
    return row


def main() -> None:
    """Build results/pool.jsonl over the whole corpus."""
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--workers", type=int, default=mp.cpu_count())
    parser.add_argument("--timeout", type=int, default=60)
    opts = parser.parse_args()

    files = sorted(p for p in DATA_DIR.rglob("*") if p.is_file())
    RESULTS_DIR.mkdir(exist_ok=True)
    out_path = RESULTS_DIR / "pool.jsonl"
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool, out_path.open("w") as out:
        jobs = ((str(p), opts.timeout) for p in files)
        for row in pool.imap_unordered(pool_file, jobs, chunksize=8):
            out.write(json.dumps(row) + "\n")
    print(f"{len(files)} files -> {out_path}")


if __name__ == "__main__":
    main()
