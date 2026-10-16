# 0001: Store cells in plain `Vec` columns, not Arrow

**Status:** accepted, 2026-10-07

**Context.** `CellStore` kept immutable columns in `arrow2` buffers. `arrow2` is
unmaintained (RUSTSEC-2025-0038) and has no fixed release, so `cargo audit`
could not pass while it was a dependency.

**Options.** Migrate to the maintained `arrow-rs`; or replace Arrow with plain
`Vec` columns, since no Arrow interoperability was ever exposed.

**Decision.** Plain `Vec` columns. The core keeps four dependencies.

**Evidence.** Criterion benchmarks in one session: `from_raw` 24% faster,
`process_sheet` 12%, `process_workbook` 6%, because cell strings are moved
instead of copied into Arrow buffers. No public API changed.
