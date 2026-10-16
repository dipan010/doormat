# 0005: Zip-bomb limits calibrated on the Enron corpus

**Status:** accepted, 2026-10-12

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
