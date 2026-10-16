# 0002: Deduplicate findings per cell, not per value

**Status:** accepted, 2026-10-07

**Context.** Findings were deduplicated by value: the same credential in two
cells, or on two sheets, was reported once and the other locations were
silently dropped.

**Options.** Keep per-value deduplication (shorter output); or deduplicate per
(cell, value), so only two pathways reporting the same value in the same cell
collapse.

**Decision.** Per cell. A responder has to know every place a secret is
stored; a shorter list that hides locations is wrong for that job. It is a
breaking change, accepted under 0.x rules.

**Evidence.** On the Enron corpus it added 4 test and 58 dev findings, all
labelled, and exposed a credential the manual review had missed (a second,
identical Login/Password block in the same sheet).
