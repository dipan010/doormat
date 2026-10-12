# Evaluation

doormat is measured on real spreadsheets, not only on synthetic fixtures. The
full method, labelling rubric and caveats are in
[`bench/corpus/README.md`](https://github.com/dipan010/doormat/blob/main/bench/corpus/README.md);
this page summarises them.

## Corpus

The [Enron spreadsheet corpus](https://figshare.com/articles/dataset/Enron_Spreadsheets_and_Emails/1221767)
(Hermans and Murphy-Hill, ICSE 2015, CC BY 4.0): 15,929 real spreadsheets
attached to Enron emails. It is not redistributed with doormat;
`bench/corpus/fetch_enron.py` downloads and verifies it.

Files are split by hash: about 20% form a **test** split, the rest **dev**. The
test split was labelled before any detection change and is now frozen; later
fixes were partly designed from its errors, so out-of-sample precision is
measured on dev.

## Results (0.2.0)

| Split | Engine | Findings | Precision | Recall |
|---|---|---|---|---|
| test | 0.1.0 | 334 | 16.5% | 31.2% (55 of 176) |
| test | 0.2.0 | 90 | 67.8% (in sample) | 34.7% (61 of 176) |
| dev | 0.2.0 | 243 | **59.3%** | not measured |

- **Recall is measured against keyword-discoverable credentials.** Credentials
  with no label nearby are invisible to both doormat and the reviewer, so true
  recall is likely lower.
- **Results are concentrated.** One directory file holds 31 of the test-split
  true positives, and recurring service-account templates repeat across dev.
  Counting each distinct (key, value) pair once, precision is 72% on test and
  31% on dev.
- **Labels were assigned by one reviewer**, who also designed the fixes. No
  second-annotator agreement has been measured.

## Robustness

0.2.0 reads all 15,929 files. The last 4 failures were malformed parts that
hold no cells (an invalid print-titles name, a pivot cache with `-` where a
number belongs); the `.xlsx` reader now skips those parts. Reading the whole corpus takes about 63 minutes
of CPU time.

## Reproducing

```bash
python bench/corpus/fetch_enron.py
python bench/corpus/scan.py --workers 4
python bench/corpus/evaluate.py bench/corpus/results/scan.jsonl
```

Labels are stored with SHA-256 hashes of values, never the values themselves.
