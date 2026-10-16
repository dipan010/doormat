# 0003: A frozen, hash-assigned test split, and saying what is in sample

**Status:** accepted, 2026-10-06

**Context.** Heuristics tuned and measured on the same files report inflated
accuracy. The Enron corpus is the only large real-world set available.

**Options.** Measure on everything; random split; deterministic split by file
hash.

**Decision.** `int(md5[:8], 16) % 5 == 0` is the test split (about 20%), the
rest is dev. Test was labelled before any change. Every reported number says
whether it is in sample (the change was designed using those files) or out of
sample, and later fixes partly designed from test errors are reported as in
sample.

**Evidence.** v0.1.0 measured 16.5% precision and 31.2% recall on the clean
test split. The split is by file, so near-identical templates appear on both
sides; this is reported as a caveat, and it is why later evaluation sets are
split by layout family instead ([0009](0009-pre-registered-evaluation.md)).
