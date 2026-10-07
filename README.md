<h1 align="center">doormat</h1>

<p align="center">
  <em>Find the passwords people leave next to the label that says "Password".</em>
</p>

<p align="center">
  <a href="https://github.com/dipan010/doormat/actions/workflows/ci.yml"><img alt="CI" src="https://github.com/dipan010/doormat/actions/workflows/ci.yml/badge.svg"></a>
  <a href="https://github.com/dipan010/doormat/actions/workflows/security.yml"><img alt="Security" src="https://github.com/dipan010/doormat/actions/workflows/security.yml/badge.svg"></a>
  <a href="LICENSE-MIT"><img alt="License: MIT OR Apache-2.0" src="https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg"></a>
  <img alt="Python 3.9+" src="https://img.shields.io/badge/python-3.9%20%7C%203.10%20%7C%203.11%20%7C%203.12%20%7C%203.13-blue.svg">
  <img alt="Rust core" src="https://img.shields.io/badge/core-Rust-orange.svg">
  <img alt="Status: alpha" src="https://img.shields.io/badge/status-alpha-yellow.svg">
</p>

---

People hide the spare key under the doormat. They hide passwords the same way: in the spreadsheet cell right next to `Password:`, in a cell that reads `pwd = hunter2`, or inside a database connection string. **doormat** reads `.xlsx`, `.xls`, `.ods`, `.csv` and `.tsv` files, models each sheet as a 2D grid, and pairs credential labels with the values around them.

- **Spatial, not just regex.** Scores every candidate value by its distance and direction from a credential label, its character mix and entropy, and nearby username or URL labels.
- **Finds what grep misses.** Credentials split across cells, built with `CONCAT` in formulas, or tucked into cell comments.
- **Fast.** Rust core with sheet-level parallelism: under 1 ms for a 5,000-cell sheet. Python bindings via PyO3, one FFI call per workbook.
- **Measured on real data.** Precision and recall are reported on the public Enron spreadsheet corpus, not just synthetic fixtures. See [Accuracy](#accuracy).
- **Offline and dependency-light.** One runtime dependency (`openpyxl`). No network calls, no model downloads.

## Installation

```bash
pip install doormat
```

Optional readers for legacy formats:

```bash
pip install "doormat[xls]"   # Excel 97-2003 via xlrd
pip install "doormat[ods]"   # OpenDocument via odfpy
pip install "doormat[all]"   # both
```

> [!NOTE]
> doormat is not on PyPI yet; the first release is in preparation. Until then, install from source as described in [CONTRIBUTING.md](CONTRIBUTING.md). Building requires a Rust toolchain and [maturin](https://www.maturin.rs).

## Quickstart

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

Findings come back in reading order: sheet by sheet, then row and column. A finding's `repr` never includes the secret, so logging one is safe:

```python
>>> doc.credentials(120)[0]
Relationship(key='Password', sheet='Servers', cell='B3', confidence=210)
```

## API

| Name | Description |
|---|---|
| `doormat.load(path)` | Read a spreadsheet (`str` or `pathlib.Path`) and run detection. Returns a `GridDoc`. Raises `FileNotFoundError`, or `ValueError` for an unsupported extension or a file whose signature does not match it. |
| `GridDoc.credentials(min_confidence=0.0)` | Relationships at or above a confidence score. `120` is a sensible default; scores above `200` are high-confidence inline or formula detections. |
| `GridDoc.relationships()` | Every inferred key/value relationship, regardless of score. |
| `GridDoc.sheet_count`, `GridDoc.cell_count` | Size of the scanned workbook. |
| `Relationship` | Frozen dataclass: `key`, `value`, `confidence`, `reason` (a `;`-separated breakdown of the score), and where it was found: `sheet`, `row`, `col` (1-based), `header_row`, `header_col` and `hidden` (the sheet is hidden). |
| `Relationship.coordinate`, `.header_coordinate` | Spreadsheet references of the value and label cells, e.g. `"B3"` and `"A3"`. |

## What it detects

| Pattern | Example |
|---|---|
| Value beside a label | `Password:` in A1, the value in B1 or A2 (within a 3-cell radius) |
| Inline key/value | `Password: s3cret!`, `PWD=s3cret!`, `Password - s3cret!` |
| Split across cells | A value spread over 2 to 5 cells under a header |
| Built in a formula | `=CONCAT("password", "S3cure#1")`, or a `password: ...` string literal in a formula |
| Hidden in a comment | A comment on a password header, or a comment containing `password: ...` |
| Multilingual labels | Password keywords in 15+ languages, including German, Spanish, French, Portuguese, Russian, Japanese, Chinese and Korean |

## Supported formats

| Format | Extensions | Formulas | Comments | Extra |
|---|---|---|---|---|
| Excel (OOXML) | `.xlsx`, `.xlsm` | yes | yes | none |
| Excel 97-2003 | `.xls` | no | no | `doormat[xls]` |
| OpenDocument | `.ods` | yes | yes | `doormat[ods]` |
| Delimited text | `.csv`, `.tsv` | n/a | n/a | none |

The reader is chosen by file extension, and binary formats are checked against their magic bytes before parsing. CSV and TSV files are treated as one sheet named after the file.

## Accuracy

On 14 synthetic fixtures, one per detection pathway, doormat scores 100% precision and recall. Real spreadsheets are harder. On the public [Enron spreadsheet corpus](bench/corpus/README.md) (15,929 files):

| Version | Precision | Recall |
|---|---|---|
| 0.1.0 | 16.5% | 31.2% |
| current `main` | 59.8% | 34.7% |

Precision for `main` is measured on files that played no part in tuning; recall is measured on files that did, so treat it as optimistic. Inline credentials are found with few false positives. Values beside a label are usually found, but a large share of those findings are still wrong pairings (usually a nearby label when the password cell is empty). The method, labelling rubric and caveats are in [`bench/corpus/README.md`](bench/corpus/README.md).

## Performance

Rust core only, Apple Silicon, criterion.rs on synthetic workbooks:

| Workbook | Time |
|---|---|
| 1 sheet, 1,000 cells | 128 µs |
| 1 sheet, 5,000 cells | 668 µs |
| 10 sheets, 5,000 cells each | 2.5 ms |

On very large real workbooks, reading the file with openpyxl dominates: 27.5 s to read a 170,000-cell workbook against 0.1 s of detection.

## How it works

1. **Extract.** Python reads every cell, formula and comment in a single pass and sends the workbook to Rust in one call.
2. **Featurize.** Each cell is normalized once and tagged with bitmask flags (character classes, header keywords via Aho-Corasick) and Shannon entropy.
3. **Select candidates.** Cells with header keywords, high entropy, formulas, comments or inline patterns are kept and classified as headers or values.
4. **Group.** Adjacent candidates are clustered into regions with a breadth-first search over a 7x7 neighbourhood.
5. **Pair and score.** Each credential label is paired with its best-scoring neighbour using distance, character mix, entropy, region membership and nearby username or URL labels. Labels, prose and formulas are penalized.

Sheets are processed in parallel with Rayon. Scoring is heuristic; there is no trained model.

## Limitations

- **Password tables are not detected.** A `Password` column with one credential per row is the most common pattern doormat misses today.
- **Legacy `.xls` files** expose no formulas or comments through xlrd, so those pathways only apply to `.xlsx`, `.xlsm` and `.ods`.
- **Python only.** Bindings for other languages are planned, as is a command-line tool.

## Roadmap

- Password-table detection
- CI, prebuilt wheels for Linux, macOS and Windows, and the first PyPI release

Progress is tracked in [STATUS.md](STATUS.md) and changes in [CHANGELOG.md](CHANGELOG.md).

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for building from source, running the tests and the benchmark harness, and adding a detection pathway.

## Acknowledgements

Real-world evaluation uses the Enron spreadsheet corpus by Felienne Hermans and Emerson Murphy-Hill ("Enron's Spreadsheets and Related Emails: A Dataset and Analysis", ICSE 2015), licensed CC BY 4.0. The corpus is not redistributed in this repository.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option.

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in doormat by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
