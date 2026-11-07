# 0021: Read `.xlsx` in Rust, with output identical to the openpyxl reader

**Status:** accepted, 2026-11-07.

**Context.** Decision 0014 removed style-only cells before openpyxl parsed a
sheet, but openpyxl still cost about 6 µs per cell with content: a
1.9-million-cell Enron workbook took about 11 s to read, against 0.35 s of
detection (#20). Two project rules stood in the way of a native reader: the
core's four-dependency budget, and file I/O living in Python. A reader is not
needed for safety (decision 0013), so the gain had to come without changing
what doormat sees.

**Options.** Keep openpyxl; write a looser native reader and accept different
output; or port openpyxl's reading behaviour exactly into a separate crate.

**Decision.** A new crate, `doormat-xlsx`, reads the parts that hold cells and
reproduces the openpyxl 3.1 reader's cell tuples exactly. That covers:

- number casting and Python's float `repr`, including its tie-to-even choice
  among shortest digit strings;
- Excel date, time and duration serials, rounded as openpyxl and Python's
  `timedelta` do;
- shared formulas through a port of openpyxl's formula tokenizer and
  translator;
- duplicate coordinates and row/column counters, with decision 0014's removal
  of style-only cells applied while streaming;
- merged ranges, hyperlinks (which write their target into empty cells),
  comments, hidden sheets and sheet-title rules;
- the XML rules of expat with defusedxml: entity declarations refused,
  characters XML 1.0 forbids refused, line endings normalised.

Where openpyxl failed a sheet or file, the same Python exception name is
reported. The exception is parts doormat does not use (column widths, print
settings and the like): those are not parsed, so they no longer fail a sheet.

- **Crate boundary.** `doormat-core` keeps its four dependencies. The new
  crate depends on the core, `quick-xml` and `zip` (decompression only).
- **One call per workbook.** `load()` reads and scans a workbook in one call
  to `process_xlsx`, so file I/O for `.xlsx` moves into Rust. `doormat-py`
  only converts types. A private `_read_xlsx` returns the cell tuples for the
  extraction check and for tools that need cells; `load()` does not call it.
  Other formats are still read in Python.
- **Dependencies.** openpyxl and defusedxml leave doormat's runtime
  dependencies. openpyxl stays a test dependency for building workbooks.

**Dependencies added.** Beyond the core's tree, on the shipped platforms:
`quick-xml`, `zip`, `flate2` with `zlib-rs` (inflate), `crc32fast`,
`indexmap`, `equivalent`, `hashbrown`, `typed-path` and `getrandom` 0.4
everywhere; `nix` and `bitflags` on macOS and the BSDs; `windows-sys` and
`windows-link` on Windows. zlib-rs is licensed Zlib, now on cargo-deny's
permissive list. Every part is read no further than its declared size, so
understated sizes fail closed as in decision 0013.

**Evidence.** The openpyxl reader's output was frozen before the port.
Extraction digests are identical on all 15,871 Enron `.xlsx` files, the 15
fixtures, set A (87) and set A2 (165). Of set B's 1,622 `.xlsx` files, one
differs: openpyxl dropped a sheet over an empty column width, which the Rust
reader reads. 12,154 float, date and duration vectors generated with Python
match. Findings are unchanged: Enron gives the same 322, field for field; sets
A and A2 give identical evaluation output; set B gives the same 74 findings
and file statuses. The 10 slowest Enron workbooks load in 11.0 s instead of
61 s, measured alternately with main in one session. 3,000 truncated and
mutated workbooks caused no panics.

**Consequences.** Reading behaviour is now doormat's own code to maintain;
the vector tests and the frozen digests are the reference for future changes.
`doormat.extract`'s openpyxl helpers (`clean_comment`,
`collect_merge_origins`, `extract_single_sheet`, `extract_workbook`) are
removed. The cell count that `max_cells` checks can be slightly lower than
before, never higher.
