# Evaluation protocol: spreadsheets from the public web (set B)

Written and committed **before** any of this data is downloaded, read or
scanned. Later changes are listed under "Amendments" with a reason.

## Question

Every Enron number for 0.3.0 is at least partly in sample (the rules were
designed from its errors), and evaluation sets A and A2 are synthetic. **How
accurate is doormat on real spreadsheets it has never been tuned on?**

This set measures; it does not tune. The detector is frozen at the 0.3.0
rules (`main`, whose detection has not changed since 0.3.0) and run once on
the test split after its answer key is complete. Improvements (#16, tables
without an account column; prefixed account labels such as `login_username`)
may later be designed on the design split, under an amendment, and checked
on data that has not been read.

## Data

The **Fuse** corpus (Barik et al., MSR 2015, https://zenodo.org/record/581678,
CC BY 4.0): 249,376 unique spreadsheets found by Common Crawl on the public
web. It shares no files with any data used to design doormat.

Fuse is one 9.4 GB zip holding a 7z archive split into 140 volumes. The
spreadsheets are a single `fuse-cc-binaries.tar.gz` in the archive's second
compressed block, which starts in volume 20. To keep the download near 1 GB,
only **volumes 20 to 34** are fetched, plus **volume 140** for the archive
index (the block's exact offset and coder settings), about 1.07 GB in all.
LZMA2, gzip and tar are decoded from the start of that block with the
standard library, and every **complete** tar member is kept (the last,
cut-off member is discarded). The block must use LZMA2 alone; any other
coder or filter stops the fetch. The subset is the first part of the tar in its
own order, which this protocol does not choose.

- `fetch_fuse.py` streams each volume with an HTTP range request, checks it
  against the CRC-32 and size in the zip directory, extracts the members and **deletes the
  volumes** once extraction is done. Spreadsheets go to `bench/web/data/`
  (gitignored).
- Excluded and counted: files byte-identical (MD5) to any Enron corpus file;
  formats doormat does not read. Files doormat fails to read stay in and are
  reported as errors.
- `.xls` files are read without formulas or comments; that limit is reported,
  not fixed here.
- The data is deleted when the work on this set ends; the script re-fetches
  it.

**Sensitive content.** These are public documents, but some were posted by
mistake and a credential in one may still work. Labels store only the
SHA-256 of a value; values never appear in chat, commits, tests, docs or
issues (masked shapes only, e.g. `Aa9a****`); no credential is ever tried;
`data/` and `results/` stay gitignored.

## Families and splits

Files built from one template must not be on both sides of a split (the
Enron split by file hash put near-identical templates on both). The source
URLs are in another 1.3 GB block and are not fetched, so a **family** is a
layout fingerprint: the SHA-256 of each sheet's name followed by the text of
its first non-empty row, lower-cased, whitespace collapsed and digits
replaced by `#`. A file that cannot be read is its own family. `split.py`
prints and stores only digests, never cell text.

Families are ordered by the SHA-256 of their fingerprint and assigned in
that order until each split reaches its share of files:

| Split | Share of files | Use |
|---|---|---|
| test | 20% | Answer key built first; the detector is run on it **once** |
| design | 30% | Not pooled, labelled or scanned now; for designing later improvements |
| reserve | 50% | Not pooled, labelled or scanned now; unseen data for checking them, and the source of extra test families (below) |

A family holding more than 1% of all files goes to design, so that no single
template dominates the test split. The split manifest (fingerprint digest,
split, file digests) is committed before any file is labelled or scanned.

## Answer key (test split)

Built without doormat's detector, from a **keyword pool** like Enron's
(`bench/corpus/keyword_pool.py`), widened in two ways fixed here:

- Identifier labels are split on `_`, `.`, `-` and lowerCamel boundaries
  before matching, so `login_password` and `wifiPassword` are pooled.
- Added keywords: `otp`, `totp`, `2fa`, `mfa`, `ssid`, `wpa`, `wep`,
  `passkey`, `key`.

A reviewer sees each pooled cell's label, its header row, neighbouring labels
and masked value shapes, and records:

- **credential**: a usable password, PIN, passphrase, key or token, with its
  kind: adjacent to a label, inline (`key: value`), table with an account
  column, or **table without an account column**;
- **not a credential**: help-desk categories, empty templates, placeholders,
  hashes, words such as `Passport`;
- **unsure**: reported separately, excluded from both counts.

Every test file with a **password-family** hit (`password`, `passwd`, `pwd`,
`pin`, `passcode`, `passphrase`, `secret`, `credential`, `otp`, `totp`,
including inside identifiers) is reviewed in full. Files whose hits are all
other keywords are reviewed in full if there are at most 400; otherwise a
seeded random sample of 400 of them is, and recall is weighted accordingly.

**Minimum yield.** Credentials on the public web are rare. If the test answer
key holds fewer than 30 credentials, whole families are moved from the
reserve to test, in fingerprint order, pooled and labelled the same way, until
it holds 30, all before the detector is run. If the reserve runs out first,
results are reported as counts, without percentages.

**Single annotator.** One reviewer labels everything; there is no second
annotator and no agreement measure, and the results say so. Items marked
unsure are listed with their shapes so a reader can judge them.

## Metrics

At confidence >= 120, each with a 95% Wilson interval:

- **precision** of all test findings, by cell and by distinct value (all
  findings reviewed; a seeded sample of 500 if there are more);
- **recall** against the answer key, overall and by kind, with tables without
  an account column on their own;
- the share of true positives from the largest family and from the largest
  single file, and both metrics without each;
- robustness: files read, files that failed and why, time per file;
- every false positive and every missed credential, described by shape and
  cause (no values).

## Procedure

1. Commit this protocol.
2. Commit `fetch_fuse.py`; download volumes 20-34 and 140, extract, delete
   the volumes. Record the file counts as an amendment (a count, not a result).
3. Commit `split.py` and the split manifest.
4. Build the test keyword pool and review it. Commit the answer key (cell locations and value digests only).
5. Run the frozen detector on test once. Review its findings for precision.
6. Report everything in `RESULTS.md`, including failures, and update the
   README's evaluation numbers.

**Rules.** Nothing seen in the test split may be added to doormat. Design and
reserve files are read only by `split.py` for their fingerprint; they are not
pooled, labelled or run through the detector, except reserve families moved
to test under the minimum-yield rule.

## Known limits

- Web spreadsheets are not internal business files: people rarely post
  credentials on purpose, so credentials will be sparse and skewed toward
  accidental publication (school account lists, Wi-Fi details, staff sheets).
- Common Crawl's coverage of the web around 2013-2014; formats skew to `.xls`.
- The subset is the first part of the tar, not a random sample of Fuse.
- Families are layouts, not websites: two templates from one site can land
  in different splits.
- One annotator, with no agreement check.
- Recall is measured against what a keyword review finds; credentials with no
  label near them are outside doormat's scope and outside the key.

## Amendments

None yet.
