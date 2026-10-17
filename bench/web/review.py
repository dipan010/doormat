"""Print the set B keyword pool for review, with values masked (see PROTOCOL.md).

For each pooled file, each hit is shown with its label, the cells to its
right and below, and its row. A cell that is itself a pool hit (a label) is
shown as text, with anything after ``:`` or ``=`` and any token containing a
digit masked. Every other cell is shown only as a shape: letters become
``A``/``a``, digits ``9``, other characters are kept, so ``Xk9#p`` reads
``Aa9#a``. Cell values are never printed in full.

Usage:
    python bench/web/review.py --split test > results/review_test.txt
"""

from __future__ import annotations

import argparse
import json
import re
import warnings
from pathlib import Path

from doormat.api import _get_format_registry

WEB_DIR = Path(__file__).resolve().parent
DATA_DIR = WEB_DIR / "data" / "spreadsheets"
RESULTS_DIR = WEB_DIR / "results"
RIGHT, BELOW, MAX_HITS = 3, 6, 12

warnings.simplefilter("ignore", UserWarning)


def shape(text: str, limit: int = 24) -> str:
    """Mask a value to its character classes, e.g. ``Pass12!`` -> ``Aaaa99!``.

    Letters of every script are masked (``A`` upper, ``a`` otherwise), not only
    Latin ones; digits become ``9``.
    """
    masked = "".join(
        "9" if ch.isdigit() else ("A" if ch.isupper() else "a") if ch.isalpha() else ch for ch in text
    )
    return masked if len(masked) <= limit else f"{masked[:limit]}…({len(masked)})"


# Words a label may show as text; any other word in a label cell is masked, so
# prose such as "the password is <word>" cannot show the word.
LABEL_WORDS = frozenset(
    re.findall(
        r"\S+",
        """
    user users username usernames name names account accounts login logins logon log in on email
    e-mail mail server host site url web ip address id ids number no code codes wireless wifi wi-fi
    network admin administrator system database db the for of to your new old default initial
    temporary reset change changed forgot forgotten enter confirm field type description notes note
    comments comment port domain school student students staff teacher teachers pupil parent parents
    my and or is are by password passwords passwd pwd pw pin pins passcode passphrase secret secrets
    credential credentials token tokens api key keys access security otp totp 2fa mfa ssid wpa wpa2
    wep passkey sign signin required please with optional this that if not
    """,
    )
)


def label(text: str) -> str:
    """Show a label's known words; mask other words, digits and anything after ``:``/``=``."""
    head, sep, tail = re.split(r"([:=])", text, maxsplit=1) if re.search(r"[:=]", text) else (text, "", "")
    head = " ".join(t if t.lower().strip("()[].,;?!*#'\"") in LABEL_WORDS else shape(t) for t in head.split())
    return f"{head[:60]}{sep}{' ' + shape(tail.strip()) if sep else ''}"


def show(grid: dict, hit_cells: set, key: tuple[str, int, int]) -> str:
    """A label cell as masked text, any other cell as a shape."""
    value = grid.get(key, "")
    return label(value) if key in hit_cells else shape(value)


def column(n: int) -> str:
    """1-based column number to letters."""
    out = ""
    while n:
        n, rem = divmod(n - 1, 26)
        out = chr(65 + rem) + out
    return out


def main() -> None:
    """Print the masked review sheet for one split's pool."""
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    parser.add_argument("--split", required=True)
    opts = parser.parse_args()
    pool = [json.loads(line) for line in (RESULTS_DIR / f"pool_{opts.split}.jsonl").open()]
    registry = _get_format_registry()
    for row in sorted((r for r in pool if r["hits"]), key=lambda r: (not r["strong"], r["file"])):
        path = DATA_DIR / row["file"]
        grid = {}
        for sheet in registry[path.suffix.lower()][1](path):
            for r, c, value, _f, _comment, name, _m in sheet:
                grid[(name, r, c)] = value
        hit_cells = {(h["sheet"], h["cell"][0], h["cell"][1]) for h in row["hits"] if h["field"] == "value"}

        print(f"\n=== {row['file']} ({'strong' if row['strong'] else 'weak'}, {len(row['hits'])} hits)")
        for h in row["hits"][:MAX_HITS]:
            sheet, (r, c) = h["sheet"], h["cell"]
            if h["field"] != "value":
                print(f"  [{sheet}] {column(c)}{r} {h['field']}: {label(h['text'])}")
                continue
            right = " | ".join(show(grid, hit_cells, (sheet, r, c + i)) for i in range(1, RIGHT + 1))
            below = " | ".join(show(grid, hit_cells, (sheet, r + i, c)) for i in range(1, BELOW + 1))
            row_cells = sorted(k for k in grid if k[0] == sheet and k[1] == r)
            row_text = " | ".join(f"{column(k[2])}:{show(grid, hit_cells, k)}" for k in row_cells[:10])
            print(f"  [{sheet}] {column(c)}{r} {h['kind']}{'*' if h['strong'] else ''}: {label(h['text'])}")
            print(f"      right: {right}\n      below: {below}\n      row:   {row_text}")
        if len(row["hits"]) > MAX_HITS:
            print(f"  ... {len(row['hits']) - MAX_HITS} more hits")


if __name__ == "__main__":
    main()
