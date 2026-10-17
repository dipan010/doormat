"""Halve set B's design split by layout family into design-A and design-B (amendment 6).

Families are ordered as ``split.py`` ordered them, by the SHA-256 of the full
family key; a file that was unreadable when the split was drawn keeps its key
``file:<digest>``. Families holding more than 1% of all files go to design-A;
the rest fill design-A in order until it holds half the design files, and the
remainder are design-B. Writes ``design_halves.csv`` (file, half).

Usage:
    python bench/web/halves.py
"""

from __future__ import annotations

import csv
import hashlib
import multiprocessing as mp

from split import DATA_DIR, LARGE_FAMILY, MANIFEST, WEB_DIR, fingerprint


def main() -> None:
    """Assign every design file to design-A or design-B and write the CSV."""
    with MANIFEST.open() as fh:
        manifest = list(csv.DictReader(fh))
    rows = {r["file"]: r for r in manifest if r["split"] == "design"}
    with mp.Pool(4) as pool:
        out = pool.map(fingerprint, [str(DATA_DIR / name) for name in rows], chunksize=16)
    families: dict[str, list[str]] = {}
    for name, digest, family in out:
        if rows[name]["family"] == rows[name]["sha256"]:  # unreadable when the split was drawn
            family = f"file:{digest}"
        families.setdefault(family, []).append(name)
    large = {f for f, files in families.items() if len(files) > LARGE_FAMILY * len(manifest)}
    half = {f: "A" for f in large}
    size_a = sum(len(families[f]) for f in large)
    for family in sorted((f for f in families if f not in large), key=lambda f: hashlib.sha256(f.encode()).hexdigest()):
        half[family] = "A" if size_a < len(rows) / 2 else "B"
        size_a += len(families[family]) if half[family] == "A" else 0
    lines = ["file,half"] + sorted(f"{name},{half[fam]}" for fam, files in families.items() for name in files)
    (WEB_DIR / "design_halves.csv").write_text("\n".join(lines) + "\n")
    count_a = sum(line.endswith(",A") for line in lines)
    print(f"{len(families)} families, {len(large)} large; design-A {count_a} files, design-B {len(rows) - count_a}")


if __name__ == "__main__":
    main()
