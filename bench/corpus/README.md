# Real-world corpus: Enron spreadsheets

Phase 2 validates gridmap beyond the 14 synthetic fixtures using the Enron
spreadsheet corpus: 15,000+ spreadsheets extracted from the Enron email archive.

> Hermans, F. and Murphy-Hill, E. "Enron's Spreadsheets and Related Emails:
> A Dataset and Analysis." ICSE 2015. Data: figshare article 1221767
> (`spreadsheets.7z`), licensed CC BY 4.0.

The spreadsheets are third-party data and are never committed. `data/` and
`results/` are gitignored. Only scripts, the labelling rubric and labels
(keyed by file hash and cell, with values stored as SHA-256) are committed.

## Reproduce

```bash
python bench/corpus/fetch_enron.py      # download, verify sha256, extract to data/
python bench/corpus/scan.py --workers 4                # gridmap over every file -> results/scan.jsonl
python bench/corpus/keyword_pool.py --split test       # recall pool -> results/pool_test.jsonl
python bench/corpus/evaluate.py results/scan.jsonl     # precision and recall on the test split
```

`scan.py` also takes `--split`, `--files` and `--out` to rescan a subset.
Use 4 workers or fewer on a laptop; a full scan with every core runs hot.

## Split

Each file is assigned by `int(md5[:8], 16) % 5`: bucket 0 is the **held-out
test split** (about 20%), buckets 1 to 4 are **dev**. Heuristic tuning may only
look at dev. The test split was labelled before any code change and is now
frozen: later fixes were partly designed from its errors, so post-fix test
numbers are in-sample and out-of-sample precision is measured on dev.

## Method

- **Precision:** every gridmap finding at confidence >= 120 in the test split
  (or a random sample stratified by `reason` prefix if there are too many) is
  labelled with the rubric below.
- **Recall:** `keyword_pool.py` scans every cell of every file for credential
  keywords and inline `key: value` patterns without using gridmap. Credentials
  found by labelling that pool form the recall denominator. This measures
  recall on *keyword-discoverable* credentials only. Credentials with no
  nearby label are invisible to both, so true recall is likely lower.
  In practice the denominator was built from the password-family hits only
  (`password`, `pwd`, `pw`, `pin`, `passcode` and inline patterns); cells
  matching only `user id`, `user name` or `login` were not reviewed. Where a
  `Password` header topped a table, every cell in that column was counted,
  not just the pool hit.
- **Robustness:** error rate, timeout rate and throughput over the whole corpus.

The archive holds 15,929 files: 15,871 `.xlsx` (converted by the dataset
authors) and 58 `.xls`. Formulas and comments are therefore visible for
almost the whole corpus; only the 58 `.xls` files go through xlrd, which
exposes neither.

## Labelling rubric

A finding is a **true positive** when the value cell (or inline value) holds a
usable secret and the key identifies it as one:

- password, passphrase, PIN, passcode, access/security code
- API key, token, shared secret, private key material
- a username/login is a TP only when the finding pairs it as the credential
  value and the sheet also holds the matching secret. A username alone is FP.

It is a **false positive** when the value is any of:

- a mask or placeholder: `****`, `xxxx`, `TBD`, `N/A`, `none`, `?`, empty-ish
- a reference rather than a secret: "same as above", "see John", "ask IT",
  "on file", a URL, a file path, an email address
- policy or instructions: "must be 8 characters", "change every 90 days"
- another header, a label, a date, a currency amount or an ordinary number
  paired with a non-credential key (e.g. "Pass-through cost")
- a secret paired with the wrong key (value is real but the key cell is not
  its label)

**Unsure** is allowed and is reported separately, never folded into TP or FP.

All labels were assigned by a single reviewer, who also designed the fixes.
There has been no second-annotator agreement check.

Labels never record plaintext. `labels_test.jsonl` and `labels_dev.jsonl`
rows hold `md5`, `sheet`, `header`, `cell`, `value_sha256`, `label`
(`tp`/`fp`) and `note`. `creds_test.json` lists the credential cells (no
values) that form the recall denominator.

## Results

Scan of all 15,929 files (2026-10-06): 99.5% processed, 15.5 minutes on an
Apple Silicon laptop. Failures: 64 openpyxl `KeyError`s on broken external
links, 5 timeouts, 4 other parse errors. On the largest workbooks, openpyxl
extraction takes over 99% of the time (27.5 s vs 0.1 s in the Rust core for
170k cells).

| Split | Engine | Findings | Precision | Recall | Notes |
|---|---|---|---|---|---|
| test | v0.1.0 | 334 | **16.5%** (55) | **32.5%** (55/169) | Clean: labelled before any code change |
| test | after fixes | 83 | 65.1% (54) | 32.0% (54/169) | In-sample: fixes were designed from these errors |
| dev | after fixes | 181 | **55.8%** (101) | not measured | Out of sample for the split-password and scoring fixes; the inline digit-token rule was chosen on dev (2 of the 181 findings) |

Read these with the concentration in mind. 31 of the 54 post-fix test true
positives come from one directory file. 100 of the 169 test credentials sit
in three files (two are copies of the same workbook). Collapsing duplicate
(key, value) pairs gives 72% precision on test and 31% on dev, because the
same service-account templates recur across many dev files. The split is by
file hash, so near-identical templates appear on both sides.

Recall by credential kind (test, after fixes): adjacent to a header 19/20,
inline 35/49, password-table column 0/100. The fixes dropped 14 inline
credentials on purpose: phone dial-in PINs (`(800) ... PIN nnnnnn`) and
`password for X = y` notes no longer match, because the looser patterns
were mostly false positives on dev.

Remaining false positives on dev are pairings with formulas, labels such as
`Included Deals`, database names next to an empty password cell, and a
help-desk log whose category column is literally "Password". The largest
recall gap is password tables (a `Password` column with one credential per
row), which no detector handles yet.
