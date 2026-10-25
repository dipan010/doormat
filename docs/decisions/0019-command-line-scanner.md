# 0019: A command-line scanner that never prints a value by default

**Status:** accepted, 2026-10-25.

**Context.** doormat was a Python library only. File-share sweeps, CI gates,
pre-commit hooks and incident response all need a command that walks
directories, prints machine-readable results and returns an exit status
(#10). Its output lands in terminals, CI logs and code-scanning dashboards,
which are far more widely readable than the spreadsheets being scanned, so a
scanner that prints what it finds would copy every secret into them.

**Options.** A separate scanner application on top of the library; a command
with its own logic (filtering, deduplication, extra rules); or a thin command
in the Python package over `doormat.load()`. For output: values shown, partly
masked (first and last character), or fully masked.

**Decision.** `doormat scan` is a thin `argparse` layer in the Python package:
no detection logic and no new runtime dependency. Each file is loaded with
`load()` (one FFI call per workbook) in a process pool; results keep the
input order, so the number of workers never changes the output.

- **Masking.** Values are replaced with a fixed-width mask, so the length is
  not shown either. Labels over 40 characters are cut, because for inline,
  formula and comment findings the label is the whole text before the value.
  The score's `reason` (it describes the value's characters and length) and
  error messages (parsers can quote file content) are omitted. `--reveal`
  shows all four.
- **SARIF never carries a value**, with or without `--reveal`, and its
  fingerprint is a hash of the location only (file, sheet, cell, label cell).
- **Exit status.** 0 clean, 1 findings at or above `--min-confidence`
  (default 120), 2 any file or sheet not fully read, with the findings still
  reported. A missing path, a named file of an unsupported type and a
  directory holding no spreadsheets give 2, so a mistyped path cannot pass a
  CI gate.
- **Per-file errors.** The exceptions a bad file is known to raise from
  `load()` (the per-sheet set, `OSError`, `ImportError`, `RuntimeError` from a
  core panic, `struct.error`, and xlrd's `XLRDError`, `CompDocError` and
  `FormulaError`) are reported for that
  file and the scan goes on. Any other exception is a bug and stops the scan.
- **Untrusted text.** In table output, control characters and Unicode
  direction overrides from sheet names and labels are escaped.
- **pre-commit** is documented as a `repo: local` hook that installs doormat
  from PyPI. A `.pre-commit-hooks.yaml` in this repository would make
  pre-commit build doormat from source, which needs Rust.

Allowlists and baselines follow separately, keyed by location only, never by
a hash of the value.

**Evidence.** On the 15,929-file Enron corpus the command's findings match a
direct scan of the same commit cell for cell, and no exception escaped the
per-file boundary. On evaluation set B's 11,200 test files it reports the
same 74 findings as the evaluation run and marks the same 544 files as not
read, with status 2; two exception types first seen there (`struct.error` and
xlrd's `FormulaError`) were added to the boundary. `--jobs 1` and `--jobs N`
output are byte-identical on set B. No
fixture value appears in masked table, JSON or SARIF output, nor in SARIF
with `--reveal`. The SARIF output validates against the OASIS 2.1.0 schema.

**Consequences.** CLAUDE.md's "no CLI" rule is replaced by these constraints.
Masking costs some convenience: triage needs `--reveal` on a trusted machine.
A label under 40 characters can still hold a user name; the documentation
says so.
