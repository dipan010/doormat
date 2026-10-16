# 0008: A sheet that fails is reported; the others are scanned

**Status:** accepted, 2026-10-12 (decided by the maintainer in #15)

**Context.** If any sheet could not be parsed, `load()` raised and returned no
findings for the readable sheets. For a scanner, a secret on a readable sheet
must still be reported.

**Options.** All-or-nothing; skip silently; or report the failure and continue.

**Decision.** Each extractor reads sheet by sheet. A sheet that fails is listed
in `GridDoc.errors` as a `SheetError(sheet, kind, message)` and the others are
scanned; `GridDoc.complete` is false. Problems with the file as a whole
(missing, wrong format, zip bomb, cell limit, unreadable workbook structure)
still raise. `repr(SheetError)` omits the message, which can quote file
content.

**Evidence.** Extraction is unchanged on 805 sampled Enron `.xlsx` files and
all 58 `.xls` files. A later randomised test of the `.ods` reader found that a
covered cell (part of a merged range) made the whole sheet fail; with this
design that sheet had been reported as an error rather than silently lost.
