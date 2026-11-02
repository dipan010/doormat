# 0018: The research export reads columns to their end and adds six features

**Status:** accepted, 2026-11-02.

**Context.** ADR 0017 kept the hand-set confidences after the lab's first
test. The lab (`doormat-lab`) has committed a second protocol before any of
its data was read: recall and false positives count distinct secrets per
file, the test slice is new (Fuse volumes 65-90), and features aimed at the
first model's errors may be added, designed on training data only. Two
things in ADR 0016's export stand in the way. It still stops a column after
`TABLE_MAX_ROWS` (1,000) rows, although the pipeline reads a credential table
to its end since 0.4.0, so a pipeline finding could fall outside the
generator's candidates. And its features cannot see the errors ADR 0017
recorded: numbers in repeated captions, string literals in `IF` and
`HYPERLINK` formulas, postal PIN codes inside addresses, and titles beside a
password header.

**Options.** Compute the new features in the lab (they would drift from the
core, the reason for ADR 0016); replace the feature vector; or keep ADR
0016's 36 features in place and append new ones.

**Decision.** In `research.rs` only:

- Columns are read to their end. The column below a label still stops at a
  new label or after more than `GAP_MAX_EMPTY` empty rows. Column statistics
  come from one pass over each column per sheet, split into unbroken runs of
  filled cells and shared by every value in the column, so a long column
  stays linear. `TABLE_MAX_ROWS`, now unused, is removed.
- Six features are appended after `baseline_confidence`, so the first 36 keep
  their positions and the model ADR 0017 evaluated can still be read:

  | Feature | Meaning |
  |---|---|
  | `label_cell_commas` | commas in the label cell's text (an address holding `pin : 999999`) |
  | `value_row_labels` | other password, username or URL labels in the value's row, besides the value and its own label (a title beside `Password`; titles that are not labels, such as `Notes`, are not counted) |
  | `label_text_repeats` | label cells among the sheet's candidates with the same normalized label text (a caption repeated down a price list) |
  | `value_formula_kind` | the value cell's formula: 0 none, 1 `HYPERLINK`, 2 `IF` (not `COUNTIF` or `SUMIF`), 3 any other; read from the whole formula, not the function enclosing the literal |
  | `column_value_share` | share of the value's run of filled cells equal to it (a default password repeated down a table) |
  | `rows_to_column_end` | rows from the value to the last filled row of its column |

None of them encodes value text. They are candidates: the lab keeps a
feature only if it helps on training data, and freezes the list in its
protocol, not in this record. The pipeline, `doormat-py` and the published
wheels are unchanged; the export stays inert in the product. If a model is
ever adopted, that change gets its own record.

**Evidence.** The product path gives the same output as before: against a
build of `main`, every finding is identical on the full Enron corpus (15,929
files, 322 findings), evaluation set B (11,200 files, 74 findings), and the
reports of sets A and A2. Every pipeline finding is a candidate on all four
corpora (also on Fuse volumes 20-34 as a whole, 36,439 files).

The export changes only where a column is longer than 1,000 rows: no
candidate is removed; 6,386 candidates on Enron and 8,769 on the Fuse
volumes are added, all more than 1,000 rows below their label; and the first
36 features differ only in the three column statistics of values in those
columns (43 Enron files and 130 Fuse files change in any way). Sets A and A2
are unchanged.

Lifting the bound exposed a quadratic step that the bound had hidden:
`empty_between` walked every row between a label and a value below it. It
now counts the filled rows from the column index. A synthetic password
column of 5,000, 10,000 and 20,000 rows exports in 18, 36 and 72 ms (the
quadratic version took 1,346 ms at 20,000 rows; the pipeline takes 23 ms).

`cargo bench --bench pipeline`, run old -> new -> old in one session, shows
no change beyond noise (`process_workbook_10x5k` 2.410, 2.395, 2.394 ms):
the pipeline's code is the same.
