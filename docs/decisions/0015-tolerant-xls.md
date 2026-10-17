# 0015: Read `.xls` files whose workbook stream shares a sector

**Status:** accepted, 2026-10-17.

**Context.** xlrd refuses an OLE2 file in which a sector of the workbook
stream is also claimed by another stream (`CompDocError: Workbook corruption:
seen[3] == 4`). In a 36,439-file sample of the Fuse web corpus, 12,851 files
gave no cells; on its design split, 2,433 of 2,492 such files failed this way.
The same check is what stops a file from reusing sectors in a cycle to
assemble a workbook stream far larger than itself.

**Options.** Keep refusing them; pass xlrd's `ignore_workbook_corruption=True`;
or pass it and bound the stream size ourselves.

**Decision.** Open every `.xls` with `ignore_workbook_corruption=True`, after
refusing (`ValueError`) a file whose OLE2 directory declares a workbook stream
larger than the file, which no real stream can be. xlrd's log goes to a
buffer instead of stdout, where it printed warnings and sector dumps into the
caller's output.

**Evidence.** On the design split, 2,209 previously unreadable files now
yield cells, no file that read before is lost, and the 8,183 that read before
extract identically. The size check refused 9 files, all of which xlrd
already rejected because their stream ran past the end of the file
(truncated downloads). The 58 Enron `.xls` files extract identically.
Regression tests build a shared-sector file, an oversized declared stream and
a file that made xlrd print.
