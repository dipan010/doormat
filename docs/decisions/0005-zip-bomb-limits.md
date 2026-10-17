# 0005: Zip-bomb limits calibrated on the Enron corpus

**Status:** accepted, 2026-10-12; amended 2026-10-17

**Context.** `.xlsx`, `.xlsm` and `.ods` are zip archives. A small file can
declare parts that decompress to gigabytes.

**Options.** No check; a fixed size limit; a size limit plus a compression
ratio limit, set from real files.

**Decision.** Before parsing, refuse more than 10,000 parts, more than 512 MiB
declared in total (configurable), or a part of at least 10 MiB compressed
beyond 500:1.

**Evidence.** Among the corpus's 15,871 `.xlsx` files the largest declares
353 MB, the highest ratio is 157:1 and the most parts is 370; none is refused.
The check reads only the zip directory, so it trusts declared sizes; a
forgery-proof limit needs a reader that counts decompressed bytes (#8).

**Amendment 1 (2026-10-17).** The last sentence was wrong. Every reader
doormat uses (openpyxl, odfpy and the cell reader) opens parts through
Python's `zipfile`, which returns at most the size declared in the central
directory and raises `BadZipFile` when the content does not match it. So the
limits above bound what is decompressed even when the sizes are forged: an
understated worksheet is skipped and listed in `GridDoc.errors`, and an
understated workbook part makes `load()` raise. Regression tests forge both.
A byte-counting reader is not needed for this; #8 is about speed only.
