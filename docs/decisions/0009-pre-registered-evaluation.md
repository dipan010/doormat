# 0009: Pre-registered evaluation sets split by layout family

**Status:** accepted, 2026-10-13

**Context.** By the credential-table work (#6) the Enron dev split had been
used for design and its test split run once; any further tuning on Enron would
make its numbers less trustworthy. The held-out dev half had also shown how
easily a rule overfits: the first table rule found 27/27 tables on the design
half and 0/19 on the held-out half.

**Options.** Keep tuning on Enron with caveats; label a second real corpus; or
build a synthetic set from layouts documented by third parties.

**Decision.** Evaluation set A (`bench/exports/`): spreadsheets in the export
layouts of eight password managers and browsers, with headers taken from
vendor documentation or exporter source code and graded by source. The
protocol, splits and metrics were committed before any data or result; later
changes are numbered amendments with reasons. Splits are by layout family, so
no layout appears on both sides. Labels of families in the holdout and test
splits are not added to the detector, even when known from documentation.

**Evidence.** The test split, run once, showed 100% precision and 23% password
recall: Chromium exports found in full, Bitwarden's `login_password` not
recognised (#19). Building the set also exposed an extractor bug (text
starting with `=` treated as a formula).
