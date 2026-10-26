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
| `--allowlist FILE` | none | Ignore findings matching a hand-written allowlist (see below). |
| `--baseline FILE` | none | Report only findings not recorded in a baseline (see below). |
| `--write-baseline FILE` | none | Record the findings in a baseline and exit 0 (2 if a file was not fully read). |

Results are printed in the order the files were found, so `--jobs` never
changes the output.

## Allowlists

An allowlist names findings to ignore for good, such as sample data or a
known false alarm. It is a JSON file you write:

```json
{
  "version": 1,
  "allow": [
    {"path": "docs/samples/*.xlsx", "note": "made-up values for the tutorial"},
    {"path": "ops/handover.xlsx", "sheet": "Legacy", "cell": "C12", "note": "rotated, account removed"}
  ]
}
```

`path` is an `fnmatch` pattern matched against the scanned file's path
relative to the allowlist's own directory, with `/` on every platform; `sheet`
and `cell` narrow it further, and `note` is for people. Anything else in the
file is an error. Matching files are still read, so a file that cannot be read
still gives exit status 2; to skip files entirely, use `--exclude`.

## Baselines

A baseline records the findings that exist today so a CI gate fails only on
new ones. Write it once from a full scan, commit it, then scan against it:

```bash
doormat scan . --write-baseline .doormat-baseline.json
doormat scan . --baseline .doormat-baseline.json
```

Each entry records where a finding is and nothing derived from its value:
the path (relative to the baseline's own directory), sheet, cell, label cell,
label (cut at 40 characters, as in masked output) and detection pathway
(`spatial`, `split`, `table`, `variable`, `inline`, `formula` or `comment`).
A finding matches an entry when all of these match.

```json
{
  "version": 1,
  "keyed": false,
  "findings": [
    {"path": "ops/it.xlsx", "sheet": "Servers", "cell": "B3", "header_cell": "A3", "label": "Password", "detector": "spatial"}
  ]
}
```

- **Paths** are relative to the baseline file, so a scan started from a
  subdirectory, or on the files pre-commit passes, matches the same entries.
  Findings in files outside the baseline's directory are never suppressed or
  recorded.
- **Updating.** `--write-baseline` replaces the entries of each file it scans
  and keeps the rest, so running it on a few files never forgets the others.
  A file that was not fully read keeps its old entries too. Entries follow
  `--min-confidence`: lowering it later surfaces findings below the old
  threshold as new.
- **Stale entries.** When a fully read file no longer has a recorded finding,
  the summary says how many entries were not found; rewrite the baseline to
  drop them.
- **Moved or relabelled.** A finding that moves to another cell, sheet or
  file, or whose label changes, is reported as new.

### Keyed baselines

A location-only baseline cannot tell that a recorded cell now holds a
different secret. Set `DOORMAT_BASELINE_KEY` (at least 16 bytes, for example
a CI secret) when writing, and each entry also stores an HMAC-SHA256 tag of
the value bound to its location:

```bash
DOORMAT_BASELINE_KEY=... doormat scan . --write-baseline .doormat-baseline.json
```

Scanning against a keyed baseline then reports a changed value in a recorded
cell as new. Two cells holding the same password get different tags, so the
file does not show which cells share a value. The key is read from the
environment only. Without it a keyed baseline cannot be used (exit status 2);
a lost or rotated key makes every finding new again; and anyone holding the
key can test guesses against the tags, so treat it like the secrets being
scanned. A plain hash of the value is never stored.

## Output formats

Findings matched by an allowlist or a baseline are left out of every format;
the summary on stderr counts them.

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
- run: doormat scan --format sarif --output doormat.sarif --baseline .doormat-baseline.json .
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
