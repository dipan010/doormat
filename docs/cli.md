# Command line

`doormat scan` scans files and directories from a shell, a CI job or a
pre-commit hook. It is a thin layer over [`doormat.load()`](api.md): each file
is loaded once, in a pool of worker processes, and the findings are printed.
It adds no detection logic, so it finds exactly what the Python API finds.

```bash
pip install doormat
doormat scan shares/finance/ handover.xlsx
```

```text
shares/finance/it.xlsx: Servers!B3: Password = ******** (confidence 210)
shares/finance/it.xlsx: Notes!A1: VPN password = ******** (confidence 250)
doormat: 2 finding(s) in 1 of 14 file(s)
```

`python -m doormat scan ...` works too.

## What is scanned

Each `PATH` is a file or a directory. Directories are walked recursively for
`.xlsx`, `.xlsm`, `.xls`, `.ods`, `.csv` and `.tsv` files (any letter case);
symlinked directories are not followed. `.xls` needs `doormat[xls]` and
`.ods` needs `doormat[ods]`; without them those files are reported as not
read.

`--include GLOB` and `--exclude GLOB` (both repeatable) filter the files. A
pattern is matched with Python's `fnmatch`, case-sensitively, against the path
relative to the directory given, written with `/` on every platform; for a
file named on the command line, against the path as given. `*` also matches
`/`, so `--exclude 'archive/*'` skips everything below `archive`.

## Values stay masked

Findings are secrets, and scan output ends up in terminals, CI logs and
dashboards. By default:

- every value is shown as `********`, eight characters whatever its length;
- a label longer than 40 characters is cut. For a finding inside a cell,
  formula or comment (`user admin, password: ...`) the label is all the text
  before the value, which can hold a user name or another secret;
- the score's `reason` is left out, since it describes the value's characters
  and length;
- error messages are left out, because a parser's message can quote the file
  content it failed on. The error's type is always shown.

`--reveal` shows all four. It never changes SARIF output, which carries no
values in any mode: SARIF files are uploaded to and stored by code-scanning
services.

Labels and sheet names come from the scanned file. In table output, control
characters and Unicode direction overrides are escaped (`\x1b`, `‮`), so
a file cannot drive your terminal or reorder what you read.

## Exit status

| Status | Meaning |
|---|---|
| 0 | Every file was read, and nothing at or above `--min-confidence` was found. |
| 1 | Every file was read, and at least one finding is at or above `--min-confidence`. |
| 2 | A file or a sheet was not fully read, or the command line was invalid. Findings are still reported. |

Status 2 wins over 1: a scan that could not read everything is never reported
as complete. A path that does not exist, a named file of an unsupported type,
and a directory holding no spreadsheets at all each give 2, so a mistyped path
in CI cannot pass. Files skipped by `--include` or `--exclude` are not errors.

## Options

| Option | Default | |
|---|---|---|
| `-f`, `--format` | `table` | `table`, `json` (JSON lines) or `sarif` (SARIF 2.1.0) |
| `-o`, `--output FILE` | stdout | Write the report to a file (UTF-8). The summary and errors still go to stderr. |
| `--min-confidence N` | `120` | Report findings at or above this score (see [how detection works](how-it-works.md)). |
| `--reveal` | off | Show values, full labels, reasons and error messages (never in SARIF). |
| `--include GLOB`, `--exclude GLOB` | none | Filter files; repeatable. |
| `-j`, `--jobs N` | one per CPU | Worker processes. `1` scans in the calling process. |
| `--max-cells N` | `2000000` | Passed to `load()`; a larger file is reported as not read. `0` removes the limit. |
| `--max-uncompressed-bytes N` | 512 MiB | Passed to `load()` for zip-based files. `0` removes the limit. |

Results are printed in the order the files were found, so `--jobs` never
changes the output.

## Output formats

**table** prints one line per finding to stdout, and errors and a summary to
stderr:

```text
path: Sheet!B3 [hidden sheet]: Label = ******** (confidence 160)
```

**json** prints one object per line: a `finding` for each finding and an
`error` for each file or sheet not fully read. Output is ASCII; other
characters are escaped by JSON.

```json
{"type": "finding", "path": "it.xlsx", "sheet": "Servers", "cell": "B3", "row": 3, "col": 2, "header_cell": "A3", "hidden": false, "key": "Password", "confidence": 210.0, "value": "********"}
{"type": "error", "path": "old.xls", "sheet": null, "kind": "CompDocError"}
```

With `--reveal`, findings also carry `reason` and errors carry `message`.
`sheet` is `null` when the whole file could not be read.

**sarif** prints one SARIF 2.1.0 run with a single rule, `doormat/credential`.
Each result names the file and, since spreadsheet cells have no line numbers,
the cell as a logical location (`Servers!B3`) and in its message and
properties. Findings at 200 or above are `error`, the rest `warning`. Each
result has a location-only fingerprint (`doormatLocation/v1`, a SHA-256 of
file, sheet, cell and label cell), so code-scanning services track an alert
across runs without ever seeing the value. Files and sheets not fully read are
listed as tool execution notifications, and the run is marked unsuccessful.

## CI

Fail a job on new secrets in committed spreadsheets and upload the results to
GitHub code scanning:

```yaml
- run: pip install "doormat[all]"
- run: doormat scan --format sarif --output doormat.sarif .
- uses: github/codeql-action/upload-sarif@v4
  if: always()
  with:
    sarif_file: doormat.sarif
```

## pre-commit

Add a local hook to `.pre-commit-config.yaml`; pre-commit installs doormat
from PyPI into the hook's environment and passes the staged spreadsheets:

```yaml
repos:
  - repo: local
    hooks:
      - id: doormat
        name: doormat
        entry: doormat scan
        language: python
        additional_dependencies: ["doormat[all]==0.5.0"]
        files: (?i)\.(xlsx|xlsm|xls|ods|csv|tsv)$
```

doormat does not ship a `.pre-commit-hooks.yaml`: a hook pointing at the
doormat repository would make pre-commit build it from source, which needs a
Rust toolchain on every developer's machine.
