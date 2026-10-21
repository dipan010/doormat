# 0016: A research export of candidates and features in the core

**Status:** accepted, 2026-10-21.

**Context.** A separate experiment (the private `doormat-lab` repository,
with its protocol committed before any data) asks whether a learned model
can rank credential candidates better than doormat's hand-set confidences.
The model needs every candidate the pathways could propose, not only those
that pass their thresholds, and a feature vector for each. If the features
were computed outside the core, they would drift from the code that would
use a shipped model. The project's rule is one FFI call per workbook into
`doormat._core`.

**Options.** Recompute the features in Python in the lab (they drift); add a
second entry point to `doormat-py`, optionally behind a cargo feature; or
expose the export as a public Rust function in `doormat-core` and bind it
from a separate crate in the lab.

**Decision.** `research.rs` in `doormat-core` provides `export_sheet` and
`export_workbook`. They return `Candidate`s, each with label and value
cells, the proposing `Pathway` and a feature vector named by
`FEATURE_NAMES`. They never return value text. The generator lists:

- every pathway's candidates before its threshold or accept rule;
- prose forms (`the password is x`);
- inline matches on everyday keywords (`pass`, `pin`, `login`, `key`);
- values 1 to `GAP_MAX_EMPTY` empty cells right of or below any cell
  holding a credential word.

Every `process_sheet` finding is a candidate, with its confidence as a
feature. `doormat-py` and the published wheels are unchanged. The lab
builds its own PyO3 crate that depends on `doormat-core` by path, and
reaches it once per workbook. The pipeline's detectors are shared with
`pub(crate)` and not changed. The feature list is frozen in the lab's
protocol, not in this record. If a model is ever adopted, that change gets
its own record.

**Evidence.** The product path gives the same output as before. A full
Enron rescan matches the scan of 741e5d8 on all 15,929 files and 322
findings, sets A and A2 report identically, and the 11,200 set B test files
give the same 74 findings (58/74, 58/60). Benchmarks, run old → new → old
in one session, show no difference: `process_sheet` on 1k cells 138.5 /
136.3 / 136.4 µs, on 5k 707.9 / 707.8 / 709.6 µs, and a 10 × 5k workbook
2.63 / 2.63 / 2.68 ms.

On training data only (counts, no cell text), every doormat finding is
among the candidates:

| Input | Doormat findings | Candidates | Per file |
|---|---|---|---|
| 15 fixtures | 19 | 59 | — |
| Sets A and A2 (294 files) | 2,939 | 14,440 | — |
| 500 Enron files | 13 | 360 | at most 109 |
| 500 Fuse files | — | 35 | — |

That keeps the volume small enough to label.
