# 0020: Allowlists and baselines keyed by location, with an optional keyed value tag

**Status:** accepted, 2026-10-27.

**Context.** Decision 0019 added `doormat scan`. A CI gate built on it fails
on every existing finding, including known false alarms and secrets already
being rotated, so teams need to record what exists and fail only on what is
new (#10). Common scanners key these records on a hash of the secret. A
baseline file is usually committed, and a password's plain hash in a
repository can be checked against guesses offline. Spreadsheet passwords are
often short and human-chosen, so they would fall to a dictionary.

**Options.** Key entries on a hash of the value; on the location only; on
the location plus the label text and the score's full `reason`; or on the
location plus an HMAC of the value under a key that is kept out of the
repository.

**Decision.** Two JSON files (the standard library reads JSON on Python 3.10;
`tomllib` needs 3.11), both with `"version": 1` and validated strictly:

- **Allowlist** (`--allowlist`), written by hand: `path` (an `fnmatch`
  pattern), optional `sheet` and `cell`, and a free-text `note`.
- **Baseline** (`--baseline`, `--write-baseline`): one entry per finding with
  its path, sheet, cell, label cell, label cut at 40 characters, and
  detection pathway. The pathway is the first token of `reason`, mapped to
  `spatial`, `split`, `table`, `variable`, `inline`, `formula` or `comment`;
  the vocabulary is part of the format, and a test fails if the core's reason
  names change. The full `reason` is not stored, because it records the
  value's length and character classes (`length>=8;upper;digit`), and nor is
  the full label, which for inline findings is all the cell text before the
  value.
- **Paths** in both files are relative to the file's own directory, so a scan
  from a subdirectory, or of the files pre-commit passes, matches the same
  entries. A file outside that directory matches nothing.
- **Partial runs.** `--write-baseline` replaces the entries of the files it
  scans and keeps all others; a file not fully read keeps its old entries as
  well. Stale entries are counted only for files fully read in the run.
- **Keyed mode.** With `DOORMAT_BASELINE_KEY` set (environment only, at least
  16 bytes), each entry also stores HMAC-SHA256 over a version string, the
  path, sheet, cell and value. A changed value in a recorded cell is then
  reported as new. Binding the location means cells sharing one password get
  different tags. This refines the "HMAC(key, value)" first proposed for #10,
  which would show which cells share a value. A keyed baseline without the
  key fails with exit status 2; a lost or rotated key makes every finding new
  again.
- Files are written sorted, ASCII-only (no direction overrides in a diff) and
  replaced atomically. Suppressed findings are left out of all outputs and
  counted in the summary.

**Evidence.** On the Enron corpus, a baseline written at the default
threshold holds 322 entries; rescanning against it reports none and exits 0,
and the same holds for a keyed baseline. On set B's test split, 74 entries
are written, the rescan reports none, and the exit status stays 2 for the 544
files not read. No finding's value appears in any field of either Enron
baseline (one value equals a sheet name elsewhere in the corpus; sheet names
are part of the location), and none of the fixture values appears in plain
or keyed baselines written from the fixtures.

**Consequences.** Moving or relabelling a recorded finding makes it new.
Location-only baselines cannot see a value change in a recorded cell; that is
the trade-off keyed mode exists for. The key must be guarded like the secrets
themselves, since anyone with it can test guesses against the tags.
