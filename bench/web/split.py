"""Assign the Fuse subset to test, design and reserve splits by layout family (see PROTOCOL.md).

A file's family is the SHA-256 of each sheet's name followed by the text of
its first non-empty row, lower-cased, whitespace collapsed and digits replaced
by ``#``. A file that cannot be read is its own family. Families are ordered
by their digest and fill test (20% of files), then design (30%), then
reserve; a family holding more than 1% of all files goes to design. Only
digests are stored or printed, never cell text; the manifest is a CSV of
file name, file digest, family digest and split.

Usage:
    python bench/web/split.py [--workers N]
"""

from __future__ import annotations

import argparse
import hashlib
import multiprocessing as mp
import re
import signal
import warnings
from pathlib import Path

from doormat.api import _get_format_registry

WEB_DIR = Path(__file__).resolve().parent
DATA_DIR = WEB_DIR / "data" / "spreadsheets"
MANIFEST = WEB_DIR / "split_manifest.csv"
SHARES = (("test", 0.20), ("design", 0.30))
LARGE_FAMILY = 0.01

warnings.simplefilter("ignore", UserWarning)


class FileTimeout(Exception):
    """Raised by SIGALRM when a single file exceeds the per-file budget."""


def _on_alarm(_signum: int, _frame: object) -> None:
    raise FileTimeout


def _normalise(text: str) -> str:
    return re.sub(r"\d", "#", " ".join(text.lower().split()))


def fingerprint(path_str: str) -> tuple[str, str, str]:
    """Return (file name, file SHA-256, family digest) for one file. Never raises."""
    path = Path(path_str)
    file_digest = hashlib.sha256(path.read_bytes()).hexdigest()
    signal.signal(signal.SIGALRM, _on_alarm)
    signal.alarm(120)
    try:
        _, extractor = _get_format_registry()[path.suffix.lower()]
        parts = []
        for sheet in extractor(path):
            if not sheet:
                continue
            first_row = min(cell[0] for cell in sheet)
            texts = [cell[2] for cell in sorted(sheet, key=lambda c: c[1]) if cell[0] == first_row]
            parts.append(_normalise(sheet[0][5]) + "\x1f" + "\x1f".join(_normalise(t) for t in texts))
        family = hashlib.sha256("\x1e".join(parts).encode()).hexdigest() if parts else f"file:{file_digest}"
    except Exception:  # noqa: BLE001 - an unreadable file is its own family
        family = f"file:{file_digest}"
    finally:
        signal.alarm(0)
    return path.name, file_digest, family


def assign(rows: list[tuple[str, str, str]]) -> dict[str, str]:
    """Map each family to a split, following the protocol's shares and order."""
    sizes: dict[str, int] = {}
    for _, _, family in rows:
        sizes[family] = sizes.get(family, 0) + 1
    total = len(rows)
    splits = {f: "design" for f, n in sizes.items() if n > LARGE_FAMILY * total}
    filled = {"test": 0, "design": sum(sizes[f] for f in splits)}
    for family in sorted(f for f in sizes if f not in splits):
        target = next((name for name, share in SHARES if filled[name] < share * total), "reserve")
        splits[family] = target
        if target != "reserve":
            filled[target] += sizes[family]
    return splits


def main() -> None:
    """Fingerprint every file in parallel and write split_manifest.json."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--workers", type=int, default=4)
    opts = parser.parse_args()
    files = sorted(str(p) for p in DATA_DIR.iterdir() if p.is_file())
    with mp.Pool(opts.workers, maxtasksperchild=200) as pool:
        rows = pool.map(fingerprint, files, chunksize=16)
    splits = assign(rows)
    # Digests are cut to 16 hex digits (64 bits): enough to identify a file or
    # family here, and it keeps the committed manifest small.
    lines = ["file,sha256,family,split"] + [
        f"{name},{digest[:16]},{family.removeprefix('file:')[:16]},{splits[family]}"
        for name, digest, family in sorted(rows)
    ]
    MANIFEST.write_text("\n".join(lines) + "\n")
    counts: dict[str, int] = {}
    for _, _, family in rows:
        counts[splits[family]] = counts.get(splits[family], 0) + 1
    unreadable = sum(family.startswith("file:") for _, _, family in rows)
    print(f"{len(rows)} files, {len(splits)} families ({unreadable} unreadable files), by split: {counts}")


if __name__ == "__main__":
    main()
