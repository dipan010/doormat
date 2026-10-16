# 0010: A default limit of 2,000,000 cells per file

**Status:** accepted, 2026-10-16

**Context.** The zip-bomb check bounds declared bytes, but a file with
millions of real cells can still exhaust memory or run for minutes (#12).

**Options.** No limit (document a process timeout); a limit applied after
extraction; or a limit counted while sheets are parsed.

**Decision.** `load(..., max_cells=2_000_000)`, counted row by row while each
sheet is parsed, so an oversized file fails before it is in memory. Exceeding
it raises `ValueError` for the whole file. `None` removes the limit.

**Evidence.** Extraction costs about 574 bytes and 25 µs per cell (measured on
a 500,000-cell workbook), so the default bounds a file at roughly 1.1 GiB and
under a minute. The largest Enron workbook has about 170,000 cells; no corpus
file is refused.
