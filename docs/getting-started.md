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

## Handle findings as secrets

A finding's `value` is a live credential. `repr()` of a finding never includes
it, so logging findings is safe; printing `.value` is not.

```python
>>> doc.credentials(120)[0]
Relationship(key='Password', sheet='Servers', cell='B3', confidence=210)
```

## Untrusted files

`load()` refuses XML entity attacks and zip bombs with `ValueError`. Zip-based
files declaring more than 512 MiB are refused by default:

```python
doormat.load("huge.xlsx", max_uncompressed_bytes=2 * 1024**3)  # allow 2 GiB
doormat.load("huge.xlsx", max_uncompressed_bytes=None)         # no size limit
```

See the [threat model](threat-model.md) for what is and is not covered.
