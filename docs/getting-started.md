# Getting started

## Install

```bash
pip install doormat
```

Optional readers for legacy formats:

```bash
pip install "doormat[xls]"   # Excel 97-2003 via xlrd
pip install "doormat[ods]"   # OpenDocument via odfpy
pip install "doormat[all]"   # both
```

Prebuilt wheels cover CPython 3.10 and newer on Linux (x86_64, aarch64, glibc
and musl), macOS (Intel, Apple Silicon) and Windows (x64, arm64). Elsewhere pip
builds from source, which needs a [Rust toolchain](https://rustup.rs).

## Scan a file

```python
import doormat

doc = doormat.load("it_handover.xlsx")
print(f"Scanned {doc.sheet_count} sheets, {doc.cell_count} cells")

for cred in doc.credentials(min_confidence=120):
    print(f"{cred.sheet}!{cred.coordinate}  {cred.key!r} -> {cred.value!r}  score={cred.confidence:.0f}")
```

```text
Scanned 2 sheets, 7 cells
Servers!B3  'Password' -> 's3cRet!99'  score=210
Notes!A1  'VPN password' -> 'hunter2!'  score=250
```

- `credentials(min_confidence=120)` is a sensible default; scores above 200 are
  inline, formula or comment detections. `relationships()` returns everything.
- Every finding knows where it is: `sheet`, `row`, `col`, `coordinate` (e.g.
  `"B3"`), the label's `header_coordinate`, and `hidden` for hidden sheets.
- `reason` explains the score, for example `distance=100;length>=8;upper;lower`.

## From the command line

```bash
doormat scan it_handover.xlsx shares/
```

prints each finding with its value masked and exits 1 if anything was found.
See [Command line](cli.md) for JSON and SARIF output, CI and pre-commit.

## Handle findings as secrets

A finding's `value` is a live credential. `repr()` of a finding never includes
it, so logging findings is safe; printing `.value` is not.

```python
>>> doc.credentials(120)[0]
Relationship(key='Password', sheet='Servers', cell='B3', confidence=210)
```

## Damaged sheets

A sheet that cannot be parsed does not stop the scan. It is listed in
`doc.errors` and the other sheets are still scanned. When a scan must be
exhaustive, for example a CI gate, check `doc.complete`:

```python
doc = doormat.load("export.xlsx")
for err in doc.errors:
    print(f"could not read sheet {err.sheet!r}: {err.kind}")
if not doc.complete:
    raise SystemExit(2)
```

`err.message` holds the parser's message, which can quote file content, so
`repr()` of a `SheetError` leaves it out. Problems with the file as a whole
(missing, wrong format, zip bomb, unreadable workbook structure) still raise.

## Untrusted files

`load()` refuses zip bombs with `ValueError`, and refuses XML entity attacks:
a worksheet that declares entities is skipped and listed in `doc.errors`.
Zip-based files declaring more than 512 MiB, and files with more than
2,000,000 cells, are refused by default. Cells are counted while sheets are
parsed, so an oversized file fails early; reading costs roughly 0.6 KB of
memory and 25 µs per cell, so the default bounds a file at about 1 GiB and
under a minute:

```python
doormat.load("huge.xlsx", max_uncompressed_bytes=2 * 1024**3)  # allow 2 GiB
doormat.load("huge.xlsx", max_cells=10_000_000)                # allow 10 million cells
doormat.load("huge.xlsx", max_uncompressed_bytes=None, max_cells=None)  # no limits
```

See the [threat model](threat-model.md) for what is and is not covered.
