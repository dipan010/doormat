# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- CSV and TSV extraction via the stdlib `csv` module (single sheet, named after the file)
- `.xls` extraction via optional `xlrd>=2.0` (`doormat[xls]`); formulas and comments are not available
- `.ods` extraction via optional `odfpy>=1.4` (`doormat[ods]`), including formulas and annotations
- `.xlsm` accepted and routed through the xlsx extractor
- `doormat[all]` extra installing both optional dependencies
- Magic-byte validation for `.xlsx`, `.xlsm`, `.xls` and `.ods` before parsing

### Changed

- Renamed the project from `gridmap` to `doormat`, because the PyPI name `gridmap` belongs to another project. The Python import is now `doormat`, the crates are `doormat-core` and `doormat-py`, and the extension module is `doormat._core`. The public API (`load()`, `GridDoc`, `Relationship`) is unchanged
- `python/gridmap/extract.py` split into the `gridmap.extract` package (now `doormat.extract`) with one module per format; `extract_workbook` kept as an alias for `extract_xlsx`
- `doormat.load()` dispatches on file extension and raises `ValueError` for unsupported extensions or mismatched file signatures
- Inline detection no longer matches whitespace-separated forms such as `(800) 555-0100 PIN 1234` or `password for Sheet = x`. On the Enron corpus these patterns were mostly false positives (offshore block names, pipeline interconnect IDs, prose)

### Fixed

- `load()` raised `AttributeError` on any workbook containing a chartsheet
- Inline credential pattern matched keywords inside words (`spin`, `compass`, `by-pass`) and treated plain whitespace as a key/value separator. Keywords must now be whole words, and the separator must be `:`, `=` or a spaced dash; password-family keywords also accept a single digit-bearing token after whitespace
- Split-password detection concatenated form labels (`Post ID:`, `Database:`) and table columns of separate passwords. Fragments must now stand alone in their column and not end in `:`
- Spatial pairing preferred a nearby label or sentence over the real value next to the header. Candidates ending in `:` get a -80 penalty and candidates containing whitespace get -40

### Validation

- Real-world evaluation on the Enron spreadsheet corpus (15,929 files) under `bench/corpus/`. v0.1.0 measured 16.5% precision and 32.5% recall on the held-out test split; after the fixes above, precision is 55.8% on the dev split (out of sample) and 65.1% on test (in sample). See `bench/corpus/README.md`

## [0.1.0] - 2026-06-29

### Added

- Spatial credential inference engine over xlsx workbooks
- Detection pathways: inline same-cell, spatial-adjacent, split-across-cells, formula-hidden, comment-hidden
- Multilingual password header detection (15+ languages) via Aho-Corasick multi-pattern matching
- Hybrid Arrow/Vec columnar storage backend (`CellStore` with arrow2 immutable columns and Vec workspace)
- Sheet-level parallelism via Rayon (`process_workbook`)
- Shannon entropy computation with bitmask pre-filter (GAP 3)
- BFS region detection with VecDeque (FIX 1 / WIN 1)
- Spatial distance table with pre-computed 7x7 score grid
- Scoring engine with distance, character-class, entropy, region, and context bonuses
- Split-password detection (2-5 cells below header)
- Python API: `gridmap.load()`, `GridDoc`, `Relationship`
- Extraction layer via openpyxl supporting formulas, comments, merged cells, hidden sheets
- PyO3 bindings (`gridmap._core`) with panic-safe FFI boundary
- Criterion benchmark suite with phase-level and sub-phase-level baselines
- 14 synthetic test fixture workbooks covering all detection pathways
- Precision/recall benchmark harness (`bench/harness/run_harness.py`)

### Changed

- Comment merge on duplicate coordinates (FIX 3): merges into existing cell instead of creating duplicates
- Workbook opened once with `data_only=False` (FIX 4): avoids reopening for formula extraction
- Spatial index contains all cells, not just candidates (FIX 2)
- Regex patterns compiled once via `LazyLock` (WIN 3)

### Performance

- Fused `normalize_and_flags()` with ASCII fast path: `precompute_features` -32% (Prompt 18)
- Buffer swap pattern for zero-allocation normalization across cells (Prompt 18)
- `MutableUtf8Array` for incremental Arrow buffer construction in `from_raw`: -3% (Prompt 19)
- End-to-end pipeline improvement: `process_sheet` (5k cells) 1,102 µs -> 927 µs (-16%)
- End-to-end workbook improvement: `process_workbook` (10x5k) 4,402 µs -> 3,330 µs (-24%)

### Internal

- 121 Rust unit tests across 10 modules
- 54 Python tests (unit + integration)
- 100% precision, 100% recall on 14 synthetic fixtures
- Sub-phase profiling infrastructure (`benches/subphases.rs`)
- Deterministic benchmark fixture generator with xorshift32 PRNG
