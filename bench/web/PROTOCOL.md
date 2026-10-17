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
masked shape and length of a value (amendment 5); values never appear in chat, commits, tests, docs or
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

1. **2026-10-17, after the download, before any file was read.** Counts only.
   Volumes 20-34 decoded to 36,441 complete tar members: 34,817 `.xls`, 1,622
   `.xlsx` and 2 that are not spreadsheets (excluded); none is a copy of an
   Enron file. Members are named by UUID without an extension, so the format
   is taken from the content (OLE2 is `.xls`; a zip is `.xlsx`, `.xlsm` or
   `.ods` by its parts). Extracted size 3.2 GB. The block offset and LZMA2
   settings were read from the archive index once and are constants in
   `fetch_fuse.py`, so the script does not fetch volume 140; a dropped
   transfer is resumed.
2. **2026-10-17, before any labelling or detector run.** `split.py` sorted
   families by their key instead of by the SHA-256 of the key, as this
   protocol says. Unreadable files, whose key is `file:<digest>`, all sorted
   last and went to reserve, so the test split had none of them. The order is
   now by SHA-256 and the splits were redrawn: test 7,289 files (2,918 of them
   unreadable), design 10,932 (2,492), reserve 18,218 (7,441). A keyword pool
   had been built on the first test split; only its counts were seen, and it
   was deleted.
3. **2026-10-17, before any detector run on test.** 12,851 of the 36,439
   files yield no cells. On the **design** split, 2,433 of its 2,492 such
   files fail in xlrd with `CompDocError` (a strict check of the OLE2
   container), and 2,209 of those open with xlrd's
   `ignore_workbook_corruption=True` and hold cells. The `.xls` extractor will
   be changed on that evidence, with its own tests and the usual gates. The
   test split is then run **twice**, each run once: with 0.3.0 as released
   (the measurement this protocol pre-registered) and with the extractor
   change, both reported. The answer key covers every test file the keyword
   pool can read, which is decided by the more permissive extractor, so both
   runs are scored against the same key; files 0.3.0 cannot read count as
   missed.
4. **2026-10-17, before any detector run on test.** The test answer key held
   1 credential (one inline password in an instructions sheet) in 176 pooled
   files, so the minimum-yield rule applied. Finding the cut-off needs the
   labels of reserve files in split order, so **every pooled reserve file was
   reviewed** (420 files), then families were moved in the order `split.py`
   uses (`yield_rule.py`). Test reached 30 after 3,110 of 13,623 reserve
   families (3,911 files) and now holds **60 credentials**: the inline password,
   a password beside its label on a hidden sheet, and a 58-row student account
   table. Labelled reserve files beyond the cut-off (a second inline password
   and a school account table of about 166 shared low-entropy passwords) stay
   in reserve; the reserve is no longer wholly unread. Review used masked
   shapes; two gaps were found and fixed: letters outside the Latin alphabet
   were not masked (only form text in Thai was shown), and a value that is
   itself a keyword (`secret`) was shown because pool-hit cells are printed as
   labels.
5. **2026-10-17, after both test runs, before any further work.** The answer
   key stored the SHA-256 of each credential, as the Enron labels do. Unlike
   Enron's, these passwords may still work, and many are weak (47 are six
   capital letters): an unsalted hash beside a file name and cell lets anyone
   test guesses offline. The key now holds each cell's location, the masked
   shape of its value (`review.shape`) and its length; scoring matches on cell
   and shape. Both runs re-scored identically (58/74, 58/60). Earlier commits
   still contain the hashes; history was not rewritten, because the corpus
   itself is public.
6. **2026-10-17, before any design-split file is pooled, labelled or scanned.
   A second question: labels whose value is not in the next cell.** The test
   run missed a password two cells right of `The password is:`, with an empty
   cell between. That case was seen only in the spent test split, so it may
   drive a change only if it also occurs in data not yet used.
   - **Data.** The design split (10,932 files), halved by family: families in
     `split.py` order fill **design-A** until it holds half the files, the rest
     are **design-B**. Families over 1% of all files go to design-A.
     `halves.py` writes the assignment, committed before pooling.
   - **Answer key.** Both halves are pooled with `pool.py` and every pooled
     file is reviewed, as for test, before any change. Each credential's kind
     is recorded, and a **gap** credential is one whose value is two or more
     cells from its label along a row or column, with only empty cells between.
     Keys store locations and masked shapes only.
   - **Stop rule.** If design-A holds fewer than 5 gap credentials, or they
     come from fewer than 2 families, no rule is designed and the counts are
     reported.
   - **Design** on design-A only. Gates: no labelled Enron true positive lost
     and every new Enron finding labelled (masked); sets A and A2 design and
     holdout no worse; set B test re-run and reported as not independent; the
     labelled reserve files beyond the cut-off (detector never run on them)
     checked for new false positives.
   - **Check.** Freeze; run design-B once with the frozen detector and with
     `main` before the change. Report gap recall, precision and every false
     positive, whatever they show.

