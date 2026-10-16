# 0007: An `.xlsx` reader that parses only parts holding cells

**Status:** accepted, 2026-10-12

**Context.** openpyxl's loader parses every workbook part. A malformed part
that holds no cells (an invalid print-titles name `#N/A`, a pivot cache with
`-` where a number belongs) made the whole file unreadable: the last 4 Enron
failures.

**Options.** Catch the errors and give up on those files; patch openpyxl; or
read only what extraction needs.

**Decision.** `_CellReader` reads shared strings, the sheet list, styles and
worksheets with their comments, one sheet at a time. It skips defined names,
print settings, document properties, the theme, pivot tables and caches,
charts, images and external-link caches. It uses openpyxl internals, so
openpyxl is pinned below 4 and regression tests build each malformed part.

**Evidence.** All 15,929 Enron files load. Extraction is identical to
openpyxl's loader on 805 sampled files, and 58% faster on 300 files timed back
to back. The equivalence check also found that array formulas had been
extracted as object descriptions, which was fixed.
