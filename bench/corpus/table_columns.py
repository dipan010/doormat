"""Collect the columns headed by a password-family keyword-pool hit, for review.

Builds the credential-table answer key independently of doormat's table rule:
every password-family hit in ``results/pool_dev.jsonl`` with at least two
non-empty cells below it (one empty row allowed between them) is written to
``results/table_cols_<half>.jsonl`` with its header row and the cells below.
Each column is then reviewed by hand (credential column or not, then cell
exceptions) and the result recorded in ``creds_dev_tables.json``.

The dev split is halved by file hash: buckets 1-2 are ``design``, 3-4 are
``holdout``.

Usage:
    python bench/corpus/keyword_pool.py --split dev
    python bench/corpus/table_columns.py design|holdout
"""

from __future__ import annotations

import argparse
import json
import multiprocessing as mp
import re

from doormat.api import _get_format_registry

from scan import DATA_DIR, RESULTS_DIR

PASSWORD_FAMILY = re.compile(r"\b(pass ?words?|passwd|pass ?codes?|pass ?phrases?|pwd|pw|pins?)\b", re.I)
HALVES = {"design": (1, 2), "holdout": (3, 4)}
MAX_ROWS = 400
MAX_GAP = 1


def columns(row: dict) -> list[dict]:
    """Return the candidate columns under one file's password-family pool hits."""
    hits = [h for h in row["hits"] if h.get("field", "value") == "value" and PASSWORD_FAMILY.search(h["text"])]
    if not hits:
        return []
    path = DATA_DIR / row["file"]
    sheets = _get_format_registry()[path.suffix.lower()][1](path, lambda name, exc: None)
    grids = {cells[0][5]: {(r, c): (v, f) for r, c, v, f, _, _, _ in cells} for cells in sheets if cells}
    out = []
    for hit in hits:
        grid = grids.get(hit["sheet"])
        if grid is None:
            continue
        r, c = hit["cell"]
        below, gap = [], 0
        for rr in range(r + 1, r + MAX_ROWS + 1):
            if grid.get((rr, c), ("", ""))[0]:
                below.append([rr, grid[(rr, c)][0], bool(grid[(rr, c)][1])])
                gap = 0
            else:
                gap += 1
                if gap > MAX_GAP:
                    break
        if len(below) < 2:
            continue
        header_row = [grid[(r, cc)][0] for cc in range(max(1, c - 8), c + 9) if cc != c and (r, cc) in grid]
        out.append({"file": row["file"], "md5": row["md5"], "sheet": hit["sheet"], "header": [r, c],
                    "header_text": hit["text"], "header_row": header_row[:10], "below": below})
    return out


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("half", choices=sorted(HALVES))
    parser.add_argument("--workers", type=int, default=4)
    opts = parser.parse_args()
    rows = [json.loads(line) for line in open(RESULTS_DIR / "pool_dev.jsonl")]
    rows = [r for r in rows if r.get("hits") and int(r["md5"][:8], 16) % 5 in HALVES[opts.half]]
    out_path = RESULTS_DIR / f"table_cols_{opts.half}.jsonl"
    count = 0
    with mp.Pool(opts.workers) as pool, out_path.open("w") as out:
        for found in pool.imap_unordered(columns, rows, chunksize=4):
            for column in found:
                out.write(json.dumps(column, ensure_ascii=False) + "\n")
                count += 1
    print(f"{opts.half}: {len(rows)} files with hits -> {count} columns in {out_path}")


if __name__ == "__main__":
    main()
