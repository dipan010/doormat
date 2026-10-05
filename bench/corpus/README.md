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
python bench/corpus/scan.py             # gridmap over every file -> results/scan.jsonl
python bench/corpus/keyword_pool.py     # independent recall pool -> results/pool.jsonl
```

## Split

Each file is assigned by `int(md5[:8], 16) % 5`: bucket 0 is the **held-out
test split** (about 20%), buckets 1 to 4 are **dev**. Heuristic tuning may only
look at dev. Reported numbers come from the test split.

## Method

- **Precision:** every gridmap finding at confidence >= 120 in the test split
  (or a random sample stratified by `reason` prefix if there are too many) is
  labelled with the rubric below.
- **Recall:** `keyword_pool.py` scans every cell of every file for credential
  keywords and inline `key: value` patterns without using gridmap. Credentials
  found by labelling that pool form the recall denominator. This measures
  recall on *keyword-discoverable* credentials only. Credentials with no
  nearby label are invisible to both, so true recall is likely lower.
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

Labels never record plaintext. `labels.jsonl` rows hold `md5`, `sheet`,
`row`, `col`, `value_sha256`, `label` (`tp`/`fp`/`unsure`) and `note`.
