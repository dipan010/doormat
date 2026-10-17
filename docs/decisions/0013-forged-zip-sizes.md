# 0013: Forged zip sizes fail closed; no byte-counting reader needed

**Status:** accepted, 2026-10-17. Corrects the evidence of
[0005](0005-zip-bomb-limits.md).

**Context.** Record 0005 says the zip-bomb check "trusts declared sizes; a
forgery-proof limit needs a reader that counts decompressed bytes (#8)". That
made a Rust `.xlsx` reader look like a security fix as well as a speed-up.

**Options.** Count decompressed bytes in our own reader; or rely on how parts
are already read, if that bounds them.

**Decision.** No byte-counting reader. Every reader doormat uses (the cell
reader, openpyxl and odfpy) opens parts through Python's `zipfile`, which
returns at most the size declared in the central directory, the size the
zip-bomb check reads, and raises `BadZipFile` when the content does not match
it. So the limits in 0005 bound what is decompressed even when the sizes are
forged. An understated worksheet is skipped and listed in `GridDoc.errors`; an
understated workbook part makes `load()` raise.

**Evidence.** A part declaring 1,000 bytes and holding 10 MB returned no data
and raised `BadZipFile` (CRC mismatch). Regression tests forge a worksheet's
size and a workbook part's size. #8 is now about speed only.

**Open.** The workbook-level failure raises `zipfile.BadZipFile`, which is
not among the exceptions `load()` documents.
