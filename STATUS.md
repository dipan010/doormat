# gridmap — Status

Last reviewed: 2026-10-07

| Built | Documented | Hosted | Posted |
|---|---|---|---|
| ✅ v0.1.0 alpha: core engine, all detection pathways | ✅ README, CHANGELOG, CONTRIBUTING, RELEASE_CHECKLIST match the code (Phase 1 done 2026-10-06) | ❌ Not on PyPI; public repo `dipan010/gridmap` only | ❌ |

## Where it stands

- Rust core + PyO3 bindings + Python package. 121 Rust unit tests, 54 Python integration tests, 100% P/R on 14 synthetic fixtures. Three optimisation rounds (−24% end to end).
- Latest code work (2026-07-02) added CSV, TSV, ODS and XLS extraction. README and CHANGELOG `[Unreleased]` now document it.
- Git remote is named `main` (not `origin`); branch is pushed and in sync.
- The spec, implementation reference, prompt plan and state files were removed on 2026-06-29 (`26074aa`). `CLAUDE.md` now points at STATUS, README, CHANGELOG and CONTRIBUTING instead. `CLAUDE.md` and `RELEASE_CHECKLIST.md` are gitignored, so those edits are local only.
- No CI (`.github/` absent). RELEASE_CHECKLIST unticked.

## Blockers


- **The PyPI name `gridmap` is taken** (v0.15.0, a DRMAA grid-engine mapper). `pip install gridmap` in the README installs someone else's package. A new distribution name is needed before any release. The import name can stay `gridmap` if desired, but a distinct one avoids confusion.
- Overlaps with `path-finder/credential-pipeline` (same problem: credentials in spreadsheets, rule-based). Decide whether they stay separate or one feeds the other.

## Phases to completion

### Phase 1 — Repair the docs ✅ (2026-10-06)
- [x] Update README "What it does NOT do" and feature list for CSV/TSV/ODS/XLS support.
- [x] Fix `CLAUDE.md`'s "Project files you must read": point it at README, CHANGELOG and this file, or restore the spec and state files from history (`git show <sha>^:gridmap_state.md`).
- [x] Add a CHANGELOG `[Unreleased]` entry for the multi-format work.
- **Exit:** a new session following CLAUDE.md reads files that exist and describe the code.

### Phase 2 — Real-world validation
- [x] Build a small anonymised or realistic corpus beyond the 14 synthetic fixtures (the README's own "Next" item).
  - Enron corpus (15,929 files) via `bench/corpus/`, CC BY 4.0. Method, rubric and results in `bench/corpus/README.md`.
- [x] Report P/R on it honestly; 100% on synthetic data is not a claim about real workbooks.
  - v0.1.0, test split (clean): P 16.5%, R 32.5% (169 credentials in 29 files).
  - After three fixes (inline regex, split-password guards, label/whitespace penalties): dev P 55.8% (out of sample), test P 65.1% / R 32.0% (in sample). Test split is now frozen.
  - Numbers are concentrated: one directory file and a few recurring service-account templates dominate. Distinct (key, value) precision: test 72%, dev 31%.
- [ ] Decide the relationship with path-finder's credential-pipeline.
- **Exit:** P/R numbers on non-synthetic data in README. (Met 2026-10-07; credential-pipeline decision still open.)

Follow-ups found in Phase 2 (not blocking):
- Password tables: 100 of 169 test credentials sit in `Password` columns with one credential per row; no detector handles them.
- Remaining dev FPs: formula cells, labels like `Included Deals`, database names beside an empty password cell, help-desk logs with a "Password" category column.
- `Relationship` exposes only internal per-sheet cell ids; add sheet, row and col so callers can locate a finding.
- 64 Enron files fail in openpyxl on broken external-link parts; try `load_workbook(..., keep_links=False)`.
- openpyxl extraction is >99% of wall time on large workbooks (27.5 s vs 0.1 s in the core for 170k cells).
- `FORMULA_KEYWORD_REGEX` has the same missing word boundary as the old inline regex (no corpus hits, but `spin`/`monkey` would match).

### Phase 3 — Release engineering
- [ ] Choose and reserve a free PyPI name (check `pypi.org/pypi/<name>/json` returns 404); update `pyproject.toml`, README install line, and the version const in `crates/gridmap-core/src/lib.rs`.
- [ ] Add GitHub Actions: `cargo test`, `cargo clippy -D warnings`, `cargo fmt --check`, `pytest`, harness. (CLAUDE.md defers CI to after v0.1.0; building wheels for five targets makes it worth doing now. Update CLAUDE.md's "What NOT to build" if so.)
- [ ] maturin wheel matrix (linux x86_64/aarch64, macOS x86_64/arm64, windows x86_64) with PyPI trusted publishing on tag.
- **Exit:** CI green on every push; a tag produces wheels.

### Phase 4 — Publish
- [ ] Work through `RELEASE_CHECKLIST.md`; publish to TestPyPI, then PyPI.
- [ ] `pip install <name>==0.1.0` from a clean venv; GitHub release with the CHANGELOG body.
- [ ] Write-up / LinkedIn post: spatial credential inference, the benchmark numbers, the Rust core / thin wrapper design.
- **Exit:** installable from PyPI, release page live, post linked here.

## Definition of done
Engine complete (done) · docs match code · validated beyond synthetic fixtures · on PyPI under a free name · post published.
