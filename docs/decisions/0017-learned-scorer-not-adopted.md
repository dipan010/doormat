# 0017: Keep the hand-set confidences; the learned scorer is not adopted

**Status:** accepted, 2026-10-25.

**Context.** ADR 0016 added a research export so that a separate experiment
(the private `doormat-lab` repository) could test whether a learned model (M)
ranks the exported candidates better than doormat's hand-set confidences (B).
The lab's protocol fixed, before any data was read, a model, a threshold and a
rule: M is eligible to replace B only if, on a fresh slice of the Fuse web
corpus tested once, it has at most 70% of B's false positives, at least B's
recall by credential cell, finds a credential in every layout family where B
does, and B has at least 20 false positives.

**Options.** Adopt M if it met the rule, with its own ADR and the full
no-regression gate; or keep B and leave M in the lab.

**Decision.** Keep B. M did not meet the rule, so nothing from the lab ships.
The research export stays in the core, unchanged, so the experiment can be
reproduced against the same code.

**Evidence.** Fuse volumes 35-64 (74,852 spreadsheets, none in Enron or set
B); the test split is 21,103 files whose layout family appears in neither.
The answer key, built from a keyword pool before any system ran, holds 1,464
credential cells, but only 7 distinct values: two password columns that
repeat one value in every row hold 1,454 of them.

| Test split | B | M |
|---|---|---|
| False positives (cells, blind review) | 134 | 6 |
| Credential cells found | 1,374 / 1,464 | 1,370 / 1,464 |
| Distinct credentials found | 5 / 7 | 5 / 7 |
| Families with a credential found | 6 | 6 |

M fails the recall criterion by 4 cells: three rows of one repeated column
and one of five sheets carrying the same value. It passes the other three.
The run also showed two things about B itself, recorded here and not acted
on: `TABLE_MAX_ROWS` (1,000) stopped the table pathway partway down a
1,088-row password column, missing 88 cells; and 88 of B's 134 false
positives were numbers in repeated `(x = 999)` captions and string literals
in `IF` and `HYPERLINK` formulas, from five files. Full results, including
every false positive and every miss by shape and cause, are in the lab's
`RESULTS.md`.
