# Contributing to gridmap

## Building from source

### Prerequisites

- Rust (stable, edition 2021)
- Python >= 3.9
- [maturin](https://www.maturin.rs/) >= 1.0

### Setup

```bash
git clone https://github.com/dipan010/gridmap.git
cd gridmap

# Install Python build and test dependencies
pip install maturin openpyxl pytest

# Build and install the extension in development mode
maturin develop --release
```

## Running tests

### Rust unit tests

```bash
cargo test --workspace
```

Runs 121 unit tests across all Rust modules. Each module tests its own functions using synthetic `CellStore` instances.

### Python tests

```bash
# Requires maturin develop --release first
python -m pytest python/tests/ -v
```

Runs 54 tests including extraction tests, integration tests against all 14 fixture workbooks, and a smoke test for the version function.

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

Runs all 14 fixture workbooks through `gridmap.load()` and compares detected credentials against `bench/fixtures/ground_truth.json`. Reports per-fixture and aggregate precision/recall. Target: P >= 0.9, R >= 0.85.

## Adding a new detection pathway

1. Identify which pipeline phase the detection belongs in. Each Rust module in `crates/gridmap-core/src/` owns one domain:
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
5. No `TODO` comments in committed code. Track deferred work in issues or `STATUS.md`.
6. Rust: `cargo clippy --workspace -- -D warnings` and `cargo fmt` must pass.
7. Python: type hints on all public function signatures, frozen dataclasses for return types.
8. Benchmark regression check: run `cargo bench --bench pipeline` and verify no phase regresses more than 5% vs the current baseline.

## Code style

See [CLAUDE.md](CLAUDE.md) for the full set of project-specific coding standards, architecture rules, module ownership, and dependency constraints.

## License

By contributing, you agree that your contributions will be dual-licensed under MIT and Apache-2.0.
