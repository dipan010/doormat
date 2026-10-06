# gridmap

A spatial document graph engine that detects credentials stored in spreadsheets. It analyzes spatial proximity between cells, content features, and scoring heuristics to infer typed relationships — finding passwords, tokens, and secrets that live next to their labels in grid layouts. Built as a Rust core with Python bindings via PyO3.

**Status: 0.1.0 alpha**

## Install

```bash
pip install gridmap
```

> **Note:** gridmap is not yet published to PyPI. To install from source, see [CONTRIBUTING.md](CONTRIBUTING.md).

## Quickstart

```python
import gridmap

doc = gridmap.load("workbook.xlsx")

print(f"Scanned {doc.sheet_count} sheets, {doc.cell_count} cells")

for cred in doc.credentials(min_confidence=120):
    print(f"  {cred.key} = {cred.value}  (confidence: {cred.confidence:.0f}, reason: {cred.reason})")

# Access all inferred relationships regardless of confidence
all_rels = doc.relationships()
```

## Supported formats

| Format | Extensions | Formulas | Comments | Extra dependency |
|---|---|---|---|---|
| Excel (OOXML) | `.xlsx`, `.xlsm` | yes | yes | none (openpyxl) |
| Excel 97-2003 | `.xls` | no | no | `xlrd>=2.0` (`pip install gridmap[xls]`) |
| OpenDocument | `.ods` | yes | yes | `odfpy>=1.4` (`pip install gridmap[ods]`) |
| Delimited text | `.csv`, `.tsv` | no | no | none (stdlib `csv`) |

The format is chosen by file extension, and binary formats are checked against their magic bytes before parsing. CSV and TSV files are treated as a single sheet named after the file. `pip install gridmap[all]` installs both optional dependencies.

## How it works

gridmap treats a spreadsheet as a 2D spatial document. Each cell has coordinates, a value, and optional metadata (formulas, comments). The engine builds a columnar store of all cells using Arrow-backed arrays, then runs a multi-phase pipeline to find credential-like patterns.

The pipeline starts with feature extraction: normalizing cell text, computing character-class flags (uppercase, lowercase, digits, special characters), calculating Shannon entropy, and matching against a multilingual set of password/username/URL header keywords using Aho-Corasick multi-pattern matching. These features are stored as bitmasks for fast downstream filtering.

Candidate cells — those with header keywords, high entropy, formulas, comments, or inline credential patterns — are selected and classified as headers, section titles, or values. Spatially adjacent candidates are clustered into regions via BFS over a 7x7 neighbor grid. Each password header is then paired with its best-scoring neighbor value cell, considering distance, character complexity, entropy, region membership, and contextual signals like nearby username or URL headers.

The scoring is heuristic, not ML-derived. Confidence scores reflect the sum of spatial and content-based bonuses. A score above 120 typically indicates a real credential; scores above 200 indicate high-confidence inline or formula-embedded detections. Sheet-level parallelism via Rayon makes workbook processing scale linearly with core count.

## Performance

Measured on Apple Silicon (aarch64) using criterion.rs on synthetic fixtures. Real-world performance may vary depending on workbook complexity and credential storage patterns.

| Workbook size | Time (Rust core) |
|---|---|
| 1 sheet, 1K cells | 180 µs |
| 1 sheet, 5K cells | 927 µs |
| 10 sheets, 5K cells each | 3.3 ms |
| 10K cells (from_raw only) | 718 µs |
| 100K cells (from_raw only) | 7.4 ms |

## What it detects

- **Spatial-adjacent credentials** — a password header cell ("Password", "Pwd", "Token", etc.) with the credential value in a neighboring cell (right, below, or nearby within a 3-cell radius)
- **Inline credentials** — key-value pairs embedded in a single cell (e.g., `password: s3cret!`)
- **Split-across-cells credentials** — password values split vertically across 2-5 cells below a header
- **Formula-hidden credentials** — credentials assembled via CONCAT or embedded as string literals in formulas
- **Comment-hidden credentials** — credential values stored in cell comments, either on a password header cell or containing an inline credential pattern
- **Multilingual headers** — password keywords detected in 15+ languages including English, German, Spanish, French, Portuguese, Russian, Japanese, Chinese, Korean, Dutch, Polish, and Swedish

## What it does NOT do

- **Plugin system** — credential detection is the only built-in detection type. A plugin interface will be added when a second detection type is needed.
- **Formulas and comments in `.xls`** — xlrd 2.x does not expose them, so formula-hidden and comment-hidden detection only works for `.xlsx`, `.xlsm` and `.ods`. CSV and TSV have no formulas or comments to inspect.
- **ML-based scoring** — confidence scores are heuristic (distance + content bonuses), not derived from a trained model. They work well on structured spreadsheets but may produce false positives on unusual layouts.
- **JS/Java/Swift bindings** — Python is the only language binding. Others are planned but not yet built.
- **CLI tool** — gridmap is API-only. There is no command-line interface.

## Real-world accuracy

The 100% precision and recall below are on 14 synthetic fixtures. On real spreadsheets (the public Enron corpus, 15,929 files, see [`bench/corpus/`](bench/corpus/README.md)):

| Engine | Precision | Recall |
|---|---|---|
| v0.1.0 | 16.5% | 32.5% |
| current `main` | 55.8% (dev split, out of sample) | 32.0% (test split) |

Credentials written next to their label (`Password:` | `value`) or inline (`Password: value`) are found reliably. Password *tables*, where a `Password` column holds one credential per row, are not detected yet and account for most missed credentials. Numbers are concentrated in a few files and recurring templates; the corpus README has the breakdown.

## Project status and roadmap

**v0.1.0** (current) — core engine complete with all detection pathways, 121 Rust unit tests, 54 Python integration tests, 100% precision and 100% recall on 14 synthetic fixtures. Three optimization rounds delivered -24% end-to-end improvement on workbook processing.

**Next:**
- Password-table detection (one credential per row under a `Password` column)
- CI/CD pipeline (GitHub Actions, wheel building, PyPI publishing)
- JS bindings via napi-rs
- Plugin system for additional detection types

**Deferred:**
- Explicit SIMD intrinsics
- Polars-style columnar architecture
- GPU support
- numba/cython acceleration

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, testing, and PR guidelines.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or <http://opensource.org/licenses/MIT>)

at your option.
