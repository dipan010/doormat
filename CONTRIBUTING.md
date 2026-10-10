# Contributing to doormat

Everyone taking part in this project is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md).

## Building from source

### Prerequisites

- Rust (stable, edition 2021)
- Python >= 3.9
- [maturin](https://www.maturin.rs/) >= 1.0

### Setup

```bash
git clone https://github.com/dipan010/doormat.git
cd doormat

# Install Python build and test dependencies
pip install maturin openpyxl pytest

# Build and install the extension in development mode
maturin develop --release
```

`maturin develop` writes the compiled extension into `python/doormat/`. If an older build with a different file name is left there (for example `_core.cpython-312-darwin.so` next to `_core.abi3.so`), Python loads the version-specific one. Delete stale `python/doormat/_core.*` files when switching Python versions or build settings; `python -c "import doormat._core as c; print(c.__file__)"` shows which one is in use.

## Running tests

### Rust unit tests

```bash
cargo test -p doormat-core
```

Each Rust module tests its own functions using synthetic `CellStore` instances. `doormat-py` is a Python extension module with no Rust tests of its own; it is exercised by the Python suite.

### Python tests

```bash
# Requires maturin develop --release first
python bench/fixtures/generate_fixtures.py   # the fixture .xlsx files are not committed
python -m pytest python/tests/ -v
```

Covers the public API, every extractor, and integration tests against all 14 fixture workbooks.

### Continuous integration

Every push to `main` and every pull request runs [`.github/workflows/ci.yml`](.github/workflows/ci.yml):

- `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings` and the core tests on Linux
- a source build with all extras, the Python tests and the precision/recall harness on Linux, macOS and Windows with Python 3.9 and 3.13

[`.github/workflows/security.yml`](.github/workflows/security.yml) runs on every push, every pull request and weekly:

- `cargo deny check` (RustSec advisories, licences, banned crates and sources; configured in [`deny.toml`](deny.toml))
- `pip-audit` on the runtime and optional dependencies declared in `pyproject.toml`
- `bandit` and `semgrep` (security-audit, python, rust and secrets rule packs)

Dependabot opens weekly update pull requests for Rust, Python and GitHub Actions dependencies.

A pull request should pass all of these before review.

## Reporting issues

Use the issue forms: bug report, detection issue (missed credential or false alarm) or feature request. Report security vulnerabilities privately as described in [SECURITY.md](SECURITY.md). Never attach real credentials anywhere; describe the layout with made-up values.

## Releasing

[`.github/workflows/release.yml`](.github/workflows/release.yml) builds one abi3 wheel per platform (Linux x86_64/aarch64, macOS arm64/x86_64, Windows x64) and a source distribution, smoke-tests every wheel the runner can execute, and checks that all files carry the same version as the tag.

1. Set the version in `crates/doormat-core/Cargo.toml` and `crates/doormat-py/Cargo.toml` (a release candidate uses Cargo's form, e.g. `0.2.0-rc.1`, which becomes `0.2.0rc1` on PyPI).
2. Move the CHANGELOG entries under that version with today's date.
3. Push a tag: `git tag v0.2.0rc1 && git push main v0.2.0rc1`. Tags containing `rc` publish to TestPyPI; any other `v*` tag publishes to PyPI.

Publishing uses [PyPI trusted publishing](https://docs.pypi.org/trusted-publishers/): no API tokens are stored in the repository. It needs a one-time setup on [PyPI](https://pypi.org/manage/account/publishing/) and [TestPyPI](https://test.pypi.org/manage/account/publishing/): add a pending publisher for project `doormat`, owner `dipan010`, repository `doormat`, workflow `release.yml`, and environment `pypi` (TestPyPI: `testpypi`).

Running the workflow manually (Actions > Release > Run workflow) builds and checks everything without publishing. To run the dependency checks locally: `cargo install cargo-deny && cargo deny check`.

## Running benchmarks

### Phase-level benchmarks

```bash
# Run all pipeline benchmarks
cargo bench --bench pipeline

# Run per-phase benchmarks
cargo bench --bench phases

# Run sub-phase benchmarks (from_raw and detect_regions internals)
cargo bench --bench subphases
```

### Comparing against baselines

```bash
# Save current results as a named baseline
cargo bench --bench pipeline -- --save-baseline my-baseline

# Compare a future run against a saved baseline
cargo bench --bench pipeline -- --baseline my-baseline
```

Results are stored in `target/criterion/` and include HTML reports.

## Running the precision/recall harness

```bash
python bench/harness/run_harness.py
```

Runs all 14 fixture workbooks through `doormat.load()` and compares detected credentials against `bench/fixtures/ground_truth.json`. Reports per-fixture and aggregate precision/recall. Target: P >= 0.9, R >= 0.85.

## Adding a new detection pathway

1. Identify which pipeline phase the detection belongs in. Each Rust module in `crates/doormat-core/src/` owns one domain:
   - Inline patterns: `detection.rs` (`detect_inline_credentials`)
   - Formula analysis: `detection.rs` (`analyze_formulas`)
   - Comment analysis: `detection.rs` (`analyze_comments`)
   - Spatial inference: `inference.rs` (`infer_relationships`)
   - New feature flags: `features.rs` (`precompute_features`)
   - New candidate criteria: `candidates.rs` (`reduce_candidate_space`)

2. Add the detection logic in the appropriate module.

3. Add a test fixture in `bench/fixtures/generate_fixtures.py` and regenerate:
   ```bash
   python bench/fixtures/generate_fixtures.py
   ```

4. Update `bench/fixtures/ground_truth.json` with expected findings.

5. Run the full verification:
   ```bash
   cargo test --workspace
   cargo clippy --workspace -- -D warnings
   maturin develop --release
   python -m pytest python/tests/ -v
   python bench/harness/run_harness.py
   ```

## PR guidelines

1. One logical change per PR.
2. All existing tests must pass (`cargo test --workspace` + `pytest`).
3. New public functions require tests and doc comments (`///` in Rust, docstrings in Python).
4. Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `test:`, `refactor:`, `docs:`, `chore:`.
5. No `TODO` comments in committed code. Track deferred work in GitHub issues.
6. Rust: `cargo clippy --workspace -- -D warnings` and `cargo fmt` must pass.
7. Python: type hints on all public function signatures, frozen dataclasses for return types.
8. Benchmark regression check: run `cargo bench --bench pipeline` and verify no phase regresses more than 5% vs the current baseline.

## Code style

See [CLAUDE.md](CLAUDE.md) for the full set of project-specific coding standards, architecture rules, module ownership, and dependency constraints.

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0.
