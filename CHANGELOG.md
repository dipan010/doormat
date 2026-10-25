# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-10-25

Reads more real files: about a third more `.xls` files from the public web, large `.xlsx` workbooks up to 5 times faster with none wrongly refused, and credential columns past 1,000 rows. Findings on the Enron corpus and on evaluation set B are the same as 0.3.0's.

### Added

- `doormat-core` gains a research export (`research::export_sheet`, `export_workbook`) for an experiment with a learned scorer. It lists every candidate the detection pathways could propose, plus prose, everyday-keyword and label-gap forms, each with 36 numeric features and no value text. The Python package, its one call per workbook and every finding are unchanged (design decision 0016)

### Fixed

- A credential table column, or a column of configuration-variable values, longer than 1,000 rows was cut off silently at row 1,001: a 1,088-row password column in a public-web spreadsheet lost its last 88 rows. Both columns are now read to their end (the walk already stops at a new label or after more than one empty row, so its length is bounded by the column itself). The research export keeps its documented 1,000-row bound. No Enron, set A, A2 or set B file has such a column: all four report identically, and the pipeline benchmarks are unchanged
- A default `load()` refused some real `.xlsx` workbooks as too large: the `max_cells` limit counted cells that hold only a style, and a sheet formatted across whole rows stores millions of them. 5 of the 15,871 Enron `.xlsx` files were refused, one with 55,000 cells of content and 14.8 million parsed. Cells holding only a style are now removed before parsing and not counted; none of the 15,871 is refused (#8)
- `.xls` files whose workbook stream shares a sector with another stream were refused (`CompDocError: Workbook corruption`). In a sample of spreadsheets from the public web (Fuse corpus) that was about a third of all `.xls` files; on the sample's design split, 2,209 of them now yield cells, and no file that read before changes. A workbook stream declared larger than the file is refused with `ValueError`, since the relaxed check no longer stops a file reusing its sectors to build one. `doormat[xls]` now needs `xlrd>=2.0.1`
- Reading some `.xls` files printed xlrd's warnings and sector dumps (`WARNING *** file size ... not 512 + multiple of sector size`) to stdout; they are now discarded

### Performance

- `.xlsx` sheets with many formatted empty cells read up to 5 times faster: cells holding only a style are removed from the sheet XML before openpyxl parses it. On the 10 slowest Enron workbooks, timed alternately in one session, 118.6 s fell to 44.6 s (the slowest from 51 s to 11 s); sheets without such cells are within 1%. Extraction output is identical on all 15,871 Enron `.xlsx` files. The filter runs on untrusted bytes before any XML parser, so every span of its pattern is bounded (the longest real cell tag is 30 bytes) and its cost stays linear on hostile input (#8)

### Validation

- Evaluation set B (`bench/web/`): 36,439 spreadsheets from the public web (Fuse corpus), split by layout family, protocol committed before download and answer key before the run. On 11,200 test files with 60 credentials, 0.3.0 and this version both find 58 (all in one account table) with 58 of 74 findings correct; outside that table they miss a password written in prose and one two cells from its label, and raise 16 false alarms. This version reads 10,656 of the test files, against 6,640. See `bench/web/RESULTS.md`
- The README's accuracy table now gives test and dev figures in separate columns; it had mixed dev precision with test recall

### Documentation

- Design decision 0017: the learned scorer tested against the research export is not adopted, and doormat keeps its hand-set confidences
- Design decision 0015 (reading `.xls` files with shared sectors, and the stream-size bound that replaces xlrd's check)
- Design decisions 0013 (forged zip sizes fail closed: parts are read through `zipfile`, which stops at the declared size, so the zip-bomb limits hold without a byte-counting reader) and 0014 (removing style-only cells), each correcting an earlier record's evidence
- The threat model and README no longer say forged zip sizes get past the zip-bomb check

## [0.3.0] - 2026-10-17

Measured on the Enron corpus, precision rose from 59.3% to 93.5% on dev and from 67.8% to 90.0% on test, and test recall from 34.7% to 61.4%; see `bench/corpus/README.md` and `bench/exports/RESULTS.md` for which parts are independent evidence.

### Added

- Credential tables: a `Password` column with one account per row is reported row by row (`reason` `table_column`, confidence 190). The column qualifies only when a username or ID label shares its header row and at least 80% of the cells below are single tokens, so help-desk logs, form templates and notes columns do not. On the Enron corpus it finds 44 of 46 table credentials on dev and 47 of 100 on test, with no false positives; see `bench/corpus/README.md` for which parts of that are independent
- Credential tables follow their account column: the username or ID label may sit up to 10 columns from the password header, and empty passwords no longer end the table. Password-manager and browser exports with the password column further from the username (Dashlane) are now read row by row
- `load(..., max_cells=2_000_000)`: a file is refused with `ValueError` once more cells than this have been read. Cells are counted while each sheet is parsed, so an oversized workbook fails early instead of filling memory; `None` removes the limit (#12)
- Compound credential labels: an identifier-style label such as `MYSQL_ROOT_PASSWORD`, `login_password`, `mail__options__auth__pass` or `ME_CONFIG_MONGODB_ADMINPASSWORD` is a credential label when its final part is a credential word; `POSTGRES_PASSWORD_FILE` and `..._PASSWORD_HASH` are not. In a list of configuration variables each label takes the value across from it (`reason` `variable_value`), `..._KEY`/`..._SALT` labels count when the value is key material, and `NAME=value` lines are read as inline credentials. Switches (`yes`, `true`, `0`) and signed or decimal amounts are never reported as credentials (#19)
- TOTP secrets: `totp` and `otpsecret` are secret labels, so a TOTP column in an export is reported like a password column

### Fixed

- `.xlsx` text beginning with `=` was treated as a formula, so a stored password such as `=k9X...` was never reported; only real formula cells are formulas now
- An empty form field (`Password:` with nothing to its right) was paired with the label below it (`Date`, `Region`), the largest false-positive family on the Enron corpus (#7)
- A `Password` category value repeated down a column (a help-desk ticket log) was paired with the cell beside it; such a label may still head a credential table
- Inline values inside a URL query (`otpauth://...?secret=X&issuer=Y`) ran on past `&`
- `.ods` sheets containing a merged range were skipped and listed in `GridDoc.errors`: LibreOffice writes covered cells for every merged range, and reading their span failed. Found by a randomised before/after test of the refactored reader
- Table cells holding a placeholder (`N/A`, `TBD`, `none`, `?`, `xxxx`, `****`) are no longer reported
- `docs/how-it-works.md` pointed to the wrong steps for the inline pattern and formula handling, and ended with a stray `EOF` line

### Performance

- Recognising compound labels costs about 3-4% in the Rust core on the criterion benchmarks (`process_sheet` 1k cells 137 µs, 5k cells 718 µs; 10 sheets of 5k cells 2.7 ms), measured in one session against the previous commit. The credential-word scan runs once per cell, in the same pass as label detection

### Documentation

- The documentation site is built with Zensical, the successor to Material for MkDocs (whose security fixes end in November 2026), keeping the same configuration and look (#14)
- Design decision records under `docs/decisions/` (ten so far, from removing `arrow2` to the cell limit), each with its options and evidence (#13)

### Validation

- Credential-table answer key for dev (`bench/corpus/creds_dev_tables.json`), built from the keyword pool independently of the detector; `evaluate.py` reports `table_column` findings as their own pathway
- New fixture `15_credential_table.xlsx`
- Evaluation set A2 (`bench/configs/`): configuration variables from the Docker Official Images documentation, 11 image families split into design, holdout and test before any result. Test split, run once: recall 12/81 -> 81/81, precision 98.8%. Set A's test split, re-run (not independent): Bitwarden and Chromium passwords 140 -> 592 of 605. See `bench/configs/RESULTS.md`
- Evaluation set A (`bench/exports/`): spreadsheets in 8 documented password-manager and browser export layouts, generated from a protocol committed before any data or result. Test split, run once: 100% precision, 23% password recall (Chromium 140/140; Bitwarden 0/465, whose `login_password` header is not recognised). Enron: test precision 80.6% -> 90.0% and dev 66.0% -> 93.5% with recall unchanged. See `bench/exports/RESULTS.md`

## [0.2.0] - 2026-10-13

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
