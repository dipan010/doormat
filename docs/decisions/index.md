# Design decisions

Short records of decisions that shaped doormat, each with the context, the
options considered, the decision and the evidence behind it. A record is not
edited once accepted; a later decision that changes it gets a new record that
says which one it replaces.

| # | Decision | Date |
|---|---|---|
| [0001](0001-remove-arrow2.md) | Store cells in plain `Vec` columns, not Arrow | 2026-10-07 |
| [0002](0002-deduplicate-per-cell.md) | Deduplicate findings per cell, not per value | 2026-10-07 |
| [0003](0003-held-out-test-split.md) | A frozen, hash-assigned test split, and saying what is in sample | 2026-10-06 |
| [0004](0004-present-cells-only.md) | Read only the cells present in a sheet | 2026-10-12 |
| [0005](0005-zip-bomb-limits.md) | Zip-bomb limits calibrated on the Enron corpus | 2026-10-12 |
| [0006](0006-abi3-python-310.md) | One stable-ABI wheel per platform; Python 3.10 floor | 2026-10-11 |
| [0007](0007-cells-only-xlsx-reader.md) | An `.xlsx` reader that parses only parts holding cells | 2026-10-12 |
| [0008](0008-per-sheet-errors.md) | A sheet that fails is reported; the others are scanned | 2026-10-12 |
| [0009](0009-pre-registered-evaluation.md) | Pre-registered evaluation sets split by layout family | 2026-10-13 |
| [0010](0010-cell-limit.md) | A default limit of 2,000,000 cells per file | 2026-10-16 |
| [0011](0011-compound-labels.md) | Compound credential labels by their final part | 2026-10-16 |
| [0012](0012-zensical.md) | Build the documentation with Zensical | 2026-10-16 |
| [0013](0013-forged-zip-sizes.md) | Forged zip sizes fail closed; no byte-counting reader needed | 2026-10-17 |
