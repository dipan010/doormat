# doormat: Status

Last reviewed: 2026-10-07

| Built | Documented | Hosted | Posted |
|---|---|---|---|
| ✅ Core engine, all detection pathways; validated on the Enron corpus | ✅ README, CHANGELOG, CONTRIBUTING match the code | ❌ Not on PyPI; public repo `dipan010/doormat` only | ❌ |

## Where it stands

- Rust core + PyO3 bindings + Python package, renamed from `gridmap` to `doormat` on 2026-10-07. 136 Rust tests, 81 Python tests, 100% P/R on 14 synthetic fixtures.
- Real-world accuracy (Enron corpus): precision 59.8% (dev, out of sample), recall 34.7% (test). See `bench/corpus/README.md`.
- Security: 0 RustSec advisories, 0 pip-audit findings, bandit and semgrep clean (2026-10-07). `doormat-core` has 4 dependencies (`arrow2` removed).
- Git remote is named `main` (not `origin`) and points at `https://github.com/dipan010/doormat.git`. GitHub About section and topics are set.
- `CLAUDE.md` and `RELEASE_CHECKLIST.md` are gitignored, so edits to them are local only.
- No CI yet (`.github/` absent).

## Blockers


- None. (The PyPI name `gridmap` belonged to another project; the project was renamed to `doormat` on 2026-10-07.)

## Phases to completion

### Phase 1: Repair the docs ✅ (2026-10-06)
- [x] Update README "What it does NOT do" and feature list for CSV/TSV/ODS/XLS support.
- [x] Fix `CLAUDE.md`'s "Project files you must read": point it at README, CHANGELOG and this file, or restore the spec and state files from history (`git show <sha>^:doormat_state.md`).
- [x] Add a CHANGELOG `[Unreleased]` entry for the multi-format work.
- **Exit:** a new session following CLAUDE.md reads files that exist and describe the code.

### Phase 2: Real-world validation ✅ (2026-10-07)
- [x] Build a small anonymised or realistic corpus beyond the 14 synthetic fixtures (the README's own "Next" item).
  - Enron corpus (15,929 files) via `bench/corpus/`, CC BY 4.0. Method, rubric and results in `bench/corpus/README.md`.
- [x] Report P/R on it honestly; 100% on synthetic data is not a claim about real workbooks.
  - v0.1.0, test split (clean): P 16.5%, R 32.5% (169 credentials in 29 files).
  - After three fixes (inline regex, split-password guards, label/whitespace penalties): dev P 55.8% (out of sample), test P 65.1% / R 32.0% (in sample). Test split is now frozen.
  - Numbers are concentrated: one directory file and a few recurring service-account templates dominate. Distinct (key, value) precision: test 72%, dev 31%.
- [x] Decide the relationship with path-finder's credential-pipeline.
  - 2026-10-07: **shelved until doormat is published.** Compared on the same Enron test split (169 credentials): credential-pipeline's accepted findings reached 9.5% recall (16/169, 0/100 table credentials) against doormat's 32.0%, and 18.3% even counting its 1,467 flagged-for-review claims; median 150 ms/file vs 28 ms. Its masking/export/quarantine layer is the reusable part if it is revived as an app on top of doormat.
- **Exit:** P/R numbers on non-synthetic data in README. (Met 2026-10-07.)

Follow-ups found in Phase 2 (not blocking):
- Password tables: 100 of 169 test credentials sit in `Password` columns with one credential per row; no detector handles them.
- Remaining dev FPs: formula cells, labels like `Included Deals`, database names beside an empty password cell, help-desk logs with a "Password" category column.
- `Relationship` exposes only internal per-sheet cell ids; add sheet, row and col so callers can locate a finding.
- openpyxl extraction is >99% of wall time on large workbooks (27.5 s vs 0.1 s in the core for 170k cells).

### Phase 3: Release engineering
Done:
- [x] Choose a free PyPI name: **`doormat`** (2026-10-07). Package, import, crates, docs and GitHub repo renamed. The name is only reserved on first upload.
- [x] Security and quality scan (2026-10-07), Snyk/Sonar equivalent: pip-audit, cargo-audit, cargo-deny, bandit, semgrep, ruff, radon, clippy pedantic, and a secret scan of all git history. Fixed 4 RustSec advisories (pyo3 0.22 -> 0.29, crossbeam-epoch, arrow2 removed) and every bandit/semgrep finding.

Next actions, in order:
1. [x] **Cell locations on findings** (2026-10-07). `Relationship` gains `sheet`, `row`, `col`, `header_row`, `header_col`, `hidden`, `coordinate`; internal cell ids removed; dedup is per cell, not per value; stable reading order; `repr` hides the secret; `_core.process_sheet` removed. Enron: test P 63.2% / R 32.4% (denominator corrected to 170), dev P 56.5%.
2. [x] **Small correctness fixes** (2026-10-07). `keep_links=False` (failing Enron files 73 -> 5); keyword boundary that accepts `_` and `.` prefixes in both regexes (also fixed a `DB_PASSWORD=x` regression from the earlier `\b` change); formula cells excluded from spatial pairing (dev: -19 FP, +12 FP, 0 TP lost). The newly readable files added 6 test credentials (denominator 176). Dev P 59.8%, test P 67.8% / R 34.7%.
   - New follow-up: when a form label like `Password:` has an empty value cell, it pairs with the next label below (`Date`, `Price`, `Region`); 19 dev and test FPs share this shape.
3. [x] **Version 0.2.0** (2026-10-07). Crates bumped (pyproject reads the version from Cargo), `doormat.__version__` added, CHANGELOG section is `[0.2.0] - Unreleased`; replace "Unreleased" with the date when the `v0.2.0` tag is pushed.
4. [ ] **CI workflow (`.github/workflows/ci.yml`)** on every push and PR: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `cargo test`, then `maturin develop`, `pytest` and the fixture harness on Linux, macOS and Windows for Python 3.9 and 3.13. Update the "No CI/CD pipelines" line in CLAUDE.md.
5. [ ] **Security workflow (`security.yml`)** on push and weekly: cargo-audit, cargo-deny (commit `deny.toml`), pip-audit, bandit, semgrep. Add Dependabot for cargo, pip and GitHub Actions.
6. [ ] **Release workflow (`release.yml`)** on `v*` tags: maturin-action wheels for linux x86_64/aarch64 (manylinux), macOS x86_64/arm64 and windows x86_64, plus an sdist; smoke-install each wheel; publish with PyPI trusted publishing (no API tokens in the repo).
7. [ ] **Repo hygiene.** `SECURITY.md` with a private vulnerability-reporting route (expected of a security tool); issue and PR templates.
8. [ ] Optional: refactor `extract_ods` (cyclomatic complexity 36).

Needs the user:
- [ ] Turn on branch protection for `main` requiring the CI checks (repo Settings > Branches).
- [ ] Optional SonarCloud and Snyk: connect the repo on their sites and add `SONAR_TOKEN` / `SNYK_TOKEN` as GitHub secrets.

- **Exit:** CI green on all three OSes on every push; a `v*` tag produces wheels and an sdist.

### Phase 4: Publish
Next actions, in order:
1. [ ] **Accounts (user).** PyPI and TestPyPI accounts with 2FA; register `dipan010/doormat` + `release.yml` as a pending trusted publisher on both.
2. [ ] **Pre-flight.** Work through `RELEASE_CHECKLIST.md` (update it for doormat first: names, version location, trusted publishing).
3. [ ] **Release candidate.** Tag `v0.2.0rc1` and publish to TestPyPI; install it in clean venvs on Linux, macOS and Windows and run the README quickstart. This is also where PyPI confirms the name is accepted.
4. [ ] **Release (user go-ahead required).** Tag `v0.2.0` and publish to PyPI; verify `pip install doormat` from a clean venv.
5. [ ] **Afterwards.** GitHub release with the CHANGELOG body; set the repo website to the PyPI page; add PyPI version and CI badges to the README.
6. [ ] **Write-up / LinkedIn post.** The doormat idea, the Enron evaluation with honest numbers, the Rust core / thin wrapper design, and the arrow2 removal.

- **Exit:** installable from PyPI, release page live, post linked here.

### Backlog (after the first release)
- Password-table detection (100 of 169 test-split credentials are in `Password` columns).
- Remaining dev false positives: formula and label pairings, empty password cells, help-desk logs.
- Extraction speed: openpyxl is >99% of wall time on large workbooks.
- Revisit credential-pipeline as a scanner app on top of doormat (masking, export, quarantine).

## Definition of done
Engine complete (done) · docs match code · validated beyond synthetic fixtures · on PyPI under a free name · post published.
