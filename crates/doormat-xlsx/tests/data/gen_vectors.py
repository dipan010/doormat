"""Regenerate the value vectors the Rust tests compare against (needs openpyxl 3.1).

Each line is tab-separated: kind, input, expected output (or "!" where
openpyxl falls back to #VALUE!). Run from the repository root:

    python crates/doormat-xlsx/tests/data/gen_vectors.py
"""

from __future__ import annotations

import math
import random
import struct
from pathlib import Path

from openpyxl.utils.datetime import MAC_EPOCH, from_excel

OUT = Path(__file__).with_name("value_vectors.tsv")


def floats(rng: random.Random) -> list[float]:
    """Random bit patterns, subnormals, decimal boundaries and spreadsheet-like numbers."""
    values = [struct.unpack("<d", rng.getrandbits(64).to_bytes(8, "little"))[0] for _ in range(2000)]
    values += [5e-324 * rng.randint(1, 1 << 52) for _ in range(200)]
    for e in (-6, -5, -4, -3, 14, 15, 16, 17):
        values += [10.0**e, 10.0**e * (1 - 1e-16), 10.0**e * (1 + 1e-15), 9.999999999999999 * 10.0**e]
    values += [round(rng.uniform(-1e6, 1e6), rng.randint(0, 10)) for _ in range(1500)]
    values += [rng.randint(-(10**9), 10**9) / 100 for _ in range(500)]
    return [v for v in values if math.isfinite(v)]


def serials(rng: random.Random) -> list[float]:
    """Date serials across the useful range, plus edge cases."""
    values = [rng.uniform(-700000, 3000000) for _ in range(1500)]
    values += [rng.randint(0, 60000) + rng.randint(0, 86399999) / 86400000 for _ in range(1500)]
    values += [rng.random() for _ in range(300)]
    values += [0.0, 0.5, 1.0, 59.0, 59.5, 60.0, 60.5, 61.0, 2958465.99999999, 2958466.0, -693593.0, -693594.0]
    return values


def show(fn, value):
    try:
        return str(fn(value))
    except (OverflowError, ValueError):
        return "!"


def main() -> None:
    rng = random.Random(20261102)
    lines = [f"repr\t{v.hex()}\t{v!r}" for v in floats(rng)]
    for v in serials(rng):
        lines.append(f"date\t{v.hex()}\t{show(from_excel, v)}")
        lines.append(f"date1904\t{v.hex()}\t{show(lambda x: from_excel(x, MAC_EPOCH), v)}")
    for v in [rng.uniform(-1000, 1000) for _ in range(1000)] + [rng.random() for _ in range(300)]:
        lines.append(f"delta\t{v.hex()}\t{show(lambda x: from_excel(x, timedelta=True), v)}")
    OUT.write_text("\n".join(lines) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
