# 0014: Remove cells holding only a style before openpyxl parses a sheet

**Status:** accepted, 2026-10-17. Corrects the evidence of
[0010](0010-cell-limit.md).

**Context.** A sheet formatted across whole rows or columns stores millions
of `<c r="A1" s="2"/>` elements: a style, no value. openpyxl builds an object
for each, and the cell limit counted them. The slowest Enron workbook
(42.7 MB) parsed 14.8 million cells to extract 55,000, in about 51 s. Record
0010 said the largest Enron workbook had about 170,000 cells and that no
corpus file was refused; in fact a default `load()` refused 5 of the 15,871
`.xlsx` files, because the corpus scans called the extractor without the
limit.

**Options.** openpyxl's `read_only` mode (still parses every cell, and drops
comments and merged ranges); our own sheet parser in Python (must reproduce
openpyxl's dates, shared formulas and types); a Rust `.xlsx` reader (new
dependencies, and file I/O moves out of Python); or removing style-only cells
from the XML before openpyxl sees it.

**Decision.** `_EmptyCellFilter` streams each worksheet part through a byte
regex that removes self-closing cells with a reference. A cell is removed
only when the next element is another referenced cell or the end of the row,
because openpyxl places a cell without a reference after the previous one.
Chunks are cut at a `<`, which cannot occur inside a tag. openpyxl still
converts every value, so output is unchanged by construction. The cell limit
now counts cells with content, which is what costs memory.

**Evidence.** All 15,871 Enron `.xlsx` files extract identically before and
after (digest of every cell tuple and sheet error). None is refused by the
default limit (was 5); the largest has 1.92 million cells with content. On
the 10 slowest files, timed alternately in one session: 118.6 s to 44.6 s;
the 42.7 MB workbook 51 s to 11 s; dense sheets with few empty cells within
1%. Handing openpyxl's parser whole 1 MiB chunks instead of the 16 KiB it
asks for made dense sheets 45% slower, so reads return at most the size
requested.
