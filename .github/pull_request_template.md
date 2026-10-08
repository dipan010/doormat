## What this changes

<!-- What and why, in a few sentences. Link the issue it fixes, if any. -->

## Checklist

- [ ] Tests added or updated, and `cargo test -p doormat-core` and `python -m pytest python/tests` pass
- [ ] `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` pass
- [ ] The precision/recall harness still passes (`python bench/harness/run_harness.py`)
- [ ] Detection logic lives in `doormat-core`; `doormat-py` only converts types
- [ ] `CHANGELOG.md` updated for user-visible changes
- [ ] No real credentials anywhere: fixtures, tests, docs and screenshots use made-up values
