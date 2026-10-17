"""Build set B's keyword pool for the answer key, without doormat's detector (see PROTOCOL.md).

Every cell of every file in the chosen split is matched against credential
keywords and inline ``key: value`` patterns, after identifier labels are split
on ``_``, ``.``, ``-`` and lowerCamel boundaries. A file is **strong** when a
hit is password-family (password, passwd, pwd, pin, passcode, passphrase,
secret, credential, otp, totp); strong files are reviewed in full.

Only doormat's extractors are shared, to read files the same way. Hits are
written to ``results/pool_<split>.jsonl`` (gitignored: cell text can hold a
credential).

Usage:
    python bench/web/pool.py --split test [--workers N]
"""

from __future__ import annotations

import argparse
import csv
import json
import multiprocessing as mp
import re
import signal
import warnings
from pathlib import Path

from doormat.api import _get_format_registry

WEB_DIR = Path(__file__).resolve().parent
DATA_DIR = WEB_DIR / "data" / "spreadsheets"
RESULTS_DIR = WEB_DIR / "results"
MANIFEST = WEB_DIR / "split_manifest.csv"

KEYWORD = re.compile(
    r"\b(pass ?words?|passwd|pass ?codes?|pass ?phrases?|pwd|pw|pins?|log ?ins?|"
    r"log ?ons?|user ?names?|user ?ids?|credentials?|secrets?|tokens?|"
    r"api ?keys?|access codes?|security codes?|"
    r"otp|totp|2fa|mfa|ssid|wpa2?|wep|pass ?keys?|keys?)\b",
    re.IGNORECASE,
)
INLINE = re.compile(
    r"\b(password|passwd|pwd|pw|pin|passcode|login|user ?(?:name|id)|secret|otp|totp|wpa2?|wep|key)"
    r"\s*[:=]\s*\S+",
    re.IGNORECASE,
)
STRONG = re.compile(
    r"\b(pass ?words?|passwd|pwd|pins?|pass ?codes?|pass ?phrases?|secrets?|credentials?|otp|totp)\b",
    re.IGNORECASE,
)

warnings.simplefilter("ignore", UserWarning)


class FileTimeout(Exception):
    """Raised by SIGALRM when a single file exceeds the per-file budget."""


def _on_alarm(_signum: int, _frame: object) -> None:
    raise FileTimeout


def split_identifiers(text: str) -> str:
    """Separate identifier parts so ``login_password`` and ``wifiPassword`` match as words."""
    return re.sub(r"(?<=[a-z])(?=[A-Z])", " ", re.sub(r"[_.\-]", " ", text))


def pool_file(path_str: str) -> dict:
    """Return every keyword-matching cell in one file. Never raises."""
    path = Path(path_str)
    row: dict = {"file": path.name, "status": "ok", "strong": False, "hits": []}
    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(120)
    try:
        _, extractor = _get_format_registry()[path.suffix.lower()]
        for cells in extractor(path):
            for r, c, value, formula, comment, sheet, _merged in cells:
                for field, text in (("value", value), ("formula", formula), ("comment", comment)):
                    if not text:
                        continue
                    words = split_identifiers(text)
                    kind = "inline" if INLINE.search(words) else "keyword" if KEYWORD.search(words) else None
                    if kind:
                        strong = bool(STRONG.search(words))
                        row["strong"] = row["strong"] or strong
                        row["hits"].append(
                            {"sheet": sheet, "cell": [r, c], "field": field, "kind": kind,
                             "strong": strong, "text": text[:300]}
                        )
    except FileTimeout:
        row["status"] = "timeout"
    except Exception as exc:  # noqa: BLE001 - failures are reported, not fatal
        row["status"] = f"error:{type(exc).__name__}"
    finally:
        signal.alarm(0)
    return row


def main() -> None:
    """Pool every file in one split and write results/pool_<split>.jsonl."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--split", required=True, choices=("test", "design", "reserve"))
    parser.add_argument("--workers", type=int, default=4)
    opts = parser.parse_args()
    with MANIFEST.open() as fh:
        files = [str(DATA_DIR / row["file"]) for row in csv.DictReader(fh) if row["split"] == opts.split]
    RESULTS_DIR.mkdir(exist_ok=True)
    out_path = RESULTS_DIR / f"pool_{opts.split}.jsonl"
    rows = []
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool, out_path.open("w") as out:
        for row in pool.imap_unordered(pool_file, files, chunksize=8):
            out.write(json.dumps(row) + "\n")
            rows.append(row)
    with_hits = [r for r in rows if r["hits"]]
    print(
        f"{len(rows)} files: {len(with_hits)} with hits, {sum(r['strong'] for r in rows)} strong, "
        f"{sum(len(r['hits']) for r in rows)} hits, {sum(r['status'] != 'ok' for r in rows)} unreadable -> {out_path}"
    )


if __name__ == "__main__":
    main()
