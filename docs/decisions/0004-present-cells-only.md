# 0004: Read only the cells present in a sheet

**Status:** accepted, 2026-10-12

**Context.** The `.xlsx` extractor walked each sheet's declared range. One
formatted cell at `XFD1048576` declares about 17 billion positions, so a
100 KB file never finished: a denial of service.

**Options.** Cap the declared range; or iterate only the cells present in the
parsed sheet.

**Decision.** Iterate the cells present (`Worksheet._cells`). This relies on an
openpyxl internal, so openpyxl is pinned below 4 and a regression test builds
the attack file.

**Evidence.** Over the 15,929-file Enron corpus, summed read time fell from
8,510 s to 3,798 s (-55%) with identical findings; four workbooks that timed
out now load.
