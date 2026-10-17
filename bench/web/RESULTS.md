# Results: spreadsheets from the public web (set B)

Protocol: [`PROTOCOL.md`](PROTOCOL.md), committed before any data was
downloaded; amendments 1-4 record every change and why. Answer key committed
(e7c3df9) before either detector run. Both runs on 2026-10-17, each once.

## Data

36,439 spreadsheets from the Fuse corpus (Common Crawl, about 2013-2014): the
first part of its binaries archive, 34,817 `.xls` and 1,622 `.xlsx`. Split by
layout family: test 7,289 files, design 10,932, reserve 18,218. The test split's
keyword pool held one credential, so 3,110 reserve families (3,911 files) were
moved to test under the minimum-yield rule: **11,200 test files, 60
credentials**.

Credentials on the public web are rare and concentrated: 58 of the 60 are one
student account table (a `Password` column beside ID, name and user-name
columns). The other two are an inline password in an instructions sheet
(`... the password is "x"`) and a password on a hidden sheet, two cells right of
`The password is:`.

## Results

At confidence >= 120, with 95% Wilson intervals:

| | 0.3.0 as released | `main` (`.xls` fix, ADR 0015) |
|---|---|---|
| Files read | 6,640 / 11,200 | **10,656 / 11,200** |
| Findings | 74 in 10 files | 74 in 10 files |
| Precision | 58/74 = 78% (68-86%) | same |
| Precision by distinct value | 58/73 | same |
| Recall | 58/60 = 97% (89-99%) | same |
| Table with account column | 58/58 | same |
| Inline | 0/1 | same |
| Label beside value | 0/1 | same |
| Table without an account column (#16) | none in the key | none in the key |
| **Without the largest file** | precision 0/16, recall 0/2 | same |
| Time per file | median 1.6 ms, p99 279 ms, max 12.6 s | median 2.0 ms, p99 241 ms, max 9.6 s |

The 60 credentials sit in 3 files and 3 layout families; the largest file and the largest family are the same table (58 of 60). The two runs found exactly the same things: the 4,016 files that only `main`
can read hold no credential and produced no finding. The headline numbers rest
on one table; outside it, doormat found nothing and raised 16 false alarms in
11,200 files.

## What it shows

- **The open 0.4.0 detection cases did not occur.** No credential table without an
  account column (#16), and no prefixed account label such as `login_username`
  (the Bitwarden TOTP gap) among the pooled cells of the 11,200 test files. This
  sample gives no data to design or check either change.

- **Tables with an account column are found** (58/58), out of sample, in a layout
  nobody tuned for.
- **False alarms on benign files are rare but real**: 16 in 11,200 files (0.14%
  of files), all 16 confirmed as not credentials. 11 are inline matches on words
  that are not credential labels in context:
  - `Pwd` as an abbreviation for powder in a drug list
  - `Pass` in swim-pass prices
  - `PIN` for a machine part
  - `Secret` in a title
  - `Login` as a help-desk topic
  
  The rest are a header row, a menu and a list paired by distance.
- **Prose is missed.** `the password is "x"` has no `:` or `=`; 0.2.0 removed the
  whitespace-separated forms because they were mostly false positives on Enron.
- **A label with a gap is missed.** `The password is:` with its value two cells
  to the right (an empty cell between) was not paired.
- **Robustness.** 0.3.0 could not read 4,560 of the 11,200 files, 4,064 of them
  because xlrd refuses `.xls` files whose workbook stream shares a sector. With
  the fix, 544 still fail: 449 `IndexError` and 29 `struct.error` from xlrd on
  damaged files, 40 `CompDocError`, 17 other xlrd errors (mostly encrypted workbooks), 8 refused because their
  declared workbook stream is larger than the file (truncated downloads), and
  1 formula error.

## Limits

- 60 credentials, 58 in one file: the intervals are wide and the precision figure
  is really "one table found, 16 false alarms".
- One annotator, no agreement measure. The pool review used masked shapes; two
  masking gaps (non-Latin letters, keyword-valued cells) were found and fixed
  during review (amendment 4).
- Recall counts what a keyword review finds; unlabelled secrets are out of scope.
- The reserve is no longer wholly unread: all its pooled files were reviewed to
  apply the minimum-yield rule. Two labelled reserve files beyond the cut-off (a
  second inline password and a school account table of about 166 shared
  passwords) are known.
- Public web files are not internal business files; credentials appear mostly by
  accident.

## Labels away from their value (amendment 6)

Pre-registered after the test run, to see whether the missed case (a password
two cells right of its label) occurs in data not yet used. The design split
(10,932 files) was halved by family and its keyword pool (351 files) reviewed
in full before any change.

| | design-A (5,467 files) | design-B (5,465 files) |
|---|---|---|
| Credentials | 0 | 2 |
| of which gap (value 2+ cells from its label) | **0** | 0 |

**Stop rule met: no rule was designed.** Design-A has no gap credential, so
there is nothing to design on. The two design-B credentials are both written
inline: one `... password: x`, one in prose (`Password is 1`-style).

Across the whole sample, the commonest miss is the prose form: `the password
is "x"` in the test split, again in a reserve file, and `Password is x` in
design-B. It was not the pre-registered question and is a known false-positive
trap (0.2.0 dropped whitespace-separated forms after they misfired on Enron),
so it is recorded here, not acted on.

