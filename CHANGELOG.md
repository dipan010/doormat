# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.2.0] - 2026-10-12

First release under the name `doormat` (previously `gridmap`, never published).

### Added

- CSV and TSV extraction via the stdlib `csv` module (single sheet, named after the file)
- `.xls` extraction via optional `xlrd>=2.0` (`doormat[xls]`); formulas and comments are not available
- `.ods` extraction via optional `odfpy>=1.4` (`doormat[ods]`), including formulas and annotations
- `.xlsm` accepted and routed through the xlsx extractor
- `doormat[all]` extra installing both optional dependencies
- Magic-byte validation for `.xlsx`, `.xlsm`, `.xls` and `.ods` before parsing

### Changed

- **Breaking:** Python 3.10 or newer is required (3.9 reached end of life in October 2025). Tested on 3.10 and 3.14
- Renamed the project from `gridmap` to `doormat`, because the PyPI name `gridmap` belongs to another project. The Python import is now `doormat`, the crates are `doormat-core` and `doormat-py`, and the extension module is `doormat._core`. The public API (`load()`, `GridDoc`, `Relationship`) is unchanged
- `python/gridmap/extract.py` split into the `gridmap.extract` package (now `doormat.extract`) with one module per format; `extract_workbook` kept as an alias for `extract_xlsx`
- `doormat.load()` dispatches on file extension and raises `ValueError` for unsupported extensions or mismatched file signatures
- A `.csv` or `.tsv` file that is not valid UTF-8, or has a field over the `csv` module's size limit, is reported in `GridDoc.errors` instead of raising `ValueError`
- Inline detection no longer matches whitespace-separated forms such as `(800) 555-0100 PIN 1234` or `password for Sheet = x`. On the Enron corpus these patterns were mostly false positives (offshore block names, pipeline interconnect IDs, prose)

### Added

- A sheet that cannot be parsed no longer fails the whole file. It is listed in `GridDoc.errors` as a `SheetError(sheet, kind, message)` and the other sheets are still scanned; `GridDoc.complete` is `True` only when every sheet was read. `repr(SheetError)` omits the message, which can quote file content. Problems with the file as a whole (missing, wrong format, zip bomb, unreadable workbook structure) still raise
- Every finding now says where it is: `Relationship.sheet`, `row`, `col` (1-based), `header_row`, `header_col`, `hidden`, and the A1-style `coordinate` and `header_coordinate` properties

### Changed

- **Breaking:** `Relationship.header_cell_id` and `value_cell_id` (internal, per-sheet indices) are removed in favour of the location fields above
- **Breaking:** deduplication is per cell instead of per value. The same credential in two cells, or on two sheets, is now two findings; previously all but one were silently dropped
- Findings are returned in a stable reading order (sheet, then row and column) instead of an arbitrary order
- `repr(Relationship)` no longer includes the credential value, so findings can be logged safely
- `doormat._core.process_sheet` is removed; `process_workbook` is the single FFI entry point

### Packaging

- Eight prebuilt wheels instead of five: Linux x86_64 and aarch64 for both glibc (manylinux) and musl (Alpine), macOS arm64 and x86_64, Windows x64 and arm64. Every wheel is built on a runner of its own architecture and installed and tested there; `abi3audit` verifies each one uses only the stable ABI
- Documentation site at https://dipan010.github.io/doormat/: API reference, how detection works, threat model and evaluation
- Ships type information (`py.typed`, plus a stub for the compiled extension); `mypy --strict` passes on the package and checks user code against it
- Wheels use the CPython stable ABI (`abi3`): one wheel per platform works on CPython 3.10 and every newer version. Other platforms build from the source distribution
- The source distribution and wheels now include both licence files

### Security

- Release and CI workflows pin every GitHub Action to a commit SHA, do not persist checkout credentials, scope elevated permissions to the jobs that need them, and are audited by zizmor on every push
- A sheet whose declared range is far larger than its content (one formatted cell at `XFD1048576` claims about 17 billion positions) made the `.xlsx` extractor run indefinitely. It now visits only the cells present in the file. Similarly, an `.ods` cell repeated a billion times was expanded cell by cell; repeated empty cells are now skipped and repeated values are capped at 16,384 columns. Both reproduced with files under 100 KB
- `.xlsx`, `.xlsm` and `.ods` files are checked for zip bombs before parsing: more than 10,000 archive parts, more than 512 MiB of declared content (configurable with `load(..., max_uncompressed_bytes=...)`), or any part of 10 MiB or more compressed beyond 500:1 is refused with `ValueError`. Limits were calibrated on 15,871 real spreadsheets (largest 353 MB, highest ratio 157:1), none of which are refused. The check reads only the archive directory, so it trusts declared sizes
- XML containing entity declarations is refused instead of being parsed: in a worksheet, that sheet is skipped and reported in `GridDoc.errors`; in the workbook structure, `load()` raises `ValueError`. `defusedxml` is now a dependency; openpyxl uses it automatically, which blocks entity-expansion attacks ("billion laughs") and external entities in `.xlsx` files
- Upgraded `pyo3` 0.22 to 0.29, resolving RUSTSEC-2025-0020 (buffer overflow risk in `PyString::from_object`) and RUSTSEC-2026-0177 (missing `Sync` bound on closures)
- Updated `crossbeam-epoch` to 0.9.21, resolving RUSTSEC-2026-0204
- Removed the unmaintained `arrow2` crate (RUSTSEC-2025-0038, no fixed release). `CellStore` now uses plain `Vec` columns; the affected API was never called. `cargo audit` and `cargo deny` report no advisories

### Performance

- Reading `.xlsx` files only visits cells present in the file: total time over the 15,929-file Enron corpus fell from 8,510 s to 3,798 s (-55%), with identical findings
- The `.xlsx` reader skips parts that hold no cells (pivot tables and caches, charts, images, defined names, document properties, the theme). On 300 Enron files timed back to back it takes 58% less time than openpyxl's own loader, with identical output; summed per-file time over the full corpus is now about 27 minutes
- Compared with 0.1.0 in the same benchmark session: `process_sheet` about 17% faster (1k and 5k cells) and `process_workbook` about 8% faster (10 sheets of 5k cells), while also resolving each finding's location
- Dropping `arrow2` moves cell strings instead of copying them into Arrow buffers: `from_raw` is 24% faster, `process_sheet` 12% and `process_workbook` 6% on the criterion benchmarks

### Fixed

- `.xls` files reported whole-number cells as floats, so a numeric password `12345678` was returned as `12345678.0`
- In `.ods` files, rows after a non-empty row repeated more than 1,000 times were reported with the wrong row number
- Four Enron workbooks that timed out now load
- `.xlsx` workbooks with a malformed part that holds no cells were refused entirely: an invalid print-titles name (`#N/A`) or a pivot cache with `-` where a number belongs. The extractor now reads only shared strings, the sheet list, styles and worksheets, and skips defined names, print settings, document properties, the theme and pivot caches. Every file in the Enron corpus now loads
- Array formulas in `.xlsx` files were extracted as a Python object description (`<openpyxl...ArrayFormula object at 0x...>`) instead of the formula text, so credentials in them were missed. Data-table cells, which have no formula text, are now treated as empty
- Inline detection ran on formula cells, whose value is the formula text, producing a garbled duplicate of the formula finding
- Inline and formula keyword patterns missed keywords after `_` or `.` (`DB_PASSWORD=x`, `app.pwd: x`, `"API_KEY"`) because `\b` treats `_` as a word character; the formula keyword pattern also matched inside words (`spin`, `monkey`)
- Workbooks with broken external-link parts raised `KeyError` in openpyxl; external links are now skipped (`keep_links=False`). This recovered 64 of 73 failing files in the Enron corpus
- A formula cell next to a password label was paired as its value (`Password:` → `=SUM(...)`). Formula cells are no longer spatial value candidates; literal strings in formulas are still detected
- `load()` raised `AttributeError` on any workbook containing a chartsheet
- Inline credential pattern matched keywords inside words (`spin`, `compass`, `by-pass`) and treated plain whitespace as a key/value separator. Keywords must now be whole words, and the separator must be `:`, `=` or a spaced dash; password-family keywords also accept a single digit-bearing token after whitespace
- Split-password detection concatenated form labels (`Post ID:`, `Database:`) and table columns of separate passwords. Fragments must now stand alone in their column and not end in `:`
- Spatial pairing preferred a nearby label or sentence over the real value next to the header. Candidates ending in `:` get a -80 penalty and candidates containing whitespace get -40

### Validation

- Real-world evaluation on the Enron spreadsheet corpus (15,929 files) under `bench/corpus/`. v0.1.0 measured 16.5% precision and 31.2% recall on the held-out test split; the current code measures 59.3% precision on the dev split (out of sample), and 67.8% precision / 34.7% recall on test (in sample). See `bench/corpus/README.md`

## [0.1.0] - 2026-06-29

### Added

- Spatial credential inference engine over xlsx workbooks
- Detection pathways: inline same-cell, spatial-adjacent, split-across-cells, formula-hidden, comment-hidden
- Multilingual password header detection (15+ languages) via Aho-Corasick multi-pattern matching
- Hybrid Arrow/Vec columnar storage backend (`CellStore` with arrow2 immutable columns and Vec workspace)
- Sheet-level parallelism via Rayon (`process_workbook`)
- Shannon entropy computation, skipped by a bitmask pre-filter for most cells
- BFS region detection with a `VecDeque` queue
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

- Comment merge on duplicate coordinates: merges into the existing cell instead of creating duplicates
- Workbook opened once with `data_only=False`: avoids reopening for formula extraction
- Spatial index contains all cells, not just candidates
- Regex patterns compiled once via `LazyLock`

### Performance

- Fused `normalize_and_flags()` with ASCII fast path: `precompute_features` -32%
- Buffer swap pattern for zero-allocation normalization across cells
- `MutableUtf8Array` for incremental Arrow buffer construction in `from_raw`: -3%
- End-to-end pipeline improvement: `process_sheet` (5k cells) 1,102 µs -> 927 µs (-16%)
- End-to-end workbook improvement: `process_workbook` (10x5k) 4,402 µs -> 3,330 µs (-24%)

### Internal

- 121 Rust unit tests across 10 modules
- 54 Python tests (unit + integration)
- 100% precision, 100% recall on 14 synthetic fixtures
- Sub-phase profiling infrastructure (`benches/subphases.rs`)
- Deterministic benchmark fixture generator with xorshift32 PRNG
