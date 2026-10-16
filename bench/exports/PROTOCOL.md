# Evaluation protocol: credential exports (set A)

Written and committed **before** any generator code, data or detection result
exists, so the commit history shows the method was fixed first. Changes after
that commit are listed under "Amendments" with a reason; nothing above that
section is edited silently.

## Question

How well does doormat find credentials in spreadsheets laid out like the
exports of password managers and browsers, a layout a DLP sweep or an incident
responder is likely to meet? These layouts are documented by their vendors, so
they do not come from doormat's own rules, unlike the Enron answer keys.

## Layout families

Each family's header row is copied from the source listed. Sources are graded:
**primary** (the vendor's documentation or the exporter's source code) or
**secondary** (third-party documentation only). Results are reported per grade.

| Family | Header (in order) | Grade | Source (checked 2026-10-13) |
|---|---|---|---|
| bitwarden | personal: `folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp`; organisation: `collections,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp` | primary | bitwarden.com/help/condition-bitwarden-import/ |
| keepassxc | `Group,Title,Username,Password,URL,Notes,TOTP,Icon,Last Modified,Created` | primary | keepassxc `src/format/CsvExporter.cpp` |
| firefox | `url,username,password,httpRealm,formActionOrigin,guid,timeCreated,timeLastUsed,timePasswordChanged` | primary | Firefox `toolkit/components/passwordmgr/LoginExport.sys.mjs` |
| chromium (Chrome, Google Password Manager, Edge) | `name,url,username,password,note` | primary | Chromium `components/password_manager/core/browser/export/password_csv_writer.cc` |
| lastpass | `url,username,password,totp,extra,name,grouping,fav` | secondary | LastPass knowledge-base article 56, as summarised by search; page body not retrievable |
| protonpass | `name,url,email,username,password,note,totp,vault` | secondary | third-party summaries of proton.me/support/pass-export |
| apple-passwords | `Title,URL,Username,Password,Notes,OTPAuth` | secondary | Apple community discussions; Apple's guide gives no column names |
| dashlane | `username,username2,username3,title,password,note,url,category,otpSecret` | secondary | third-party guides; support.dashlane.com refuses automated access |

1Password is excluded: its documentation gives the column order but not the
names.

## Splits

Splits are by **family**, never by file, so no layout is seen on both sides.
Families are sorted by the SHA-256 of their name; the first 4 are **design**,
the next 2 **holdout**, the last 2 **test**:

- design: keepassxc, firefox, protonpass, dashlane
- holdout: apple-passwords, lastpass
- test: bitwarden, chromium

## Variants

Every family is generated in each variant below. A variant may be combined
with others only as listed.

| ID | Variant |
|---|---|
| V1 | Canonical: the documented header in row 1, one account per row, as `.csv` |
| V2 | V1 saved as `.xlsx` |
| V3 | V2 with the header re-cased: Title Case, and UPPER CASE (two files) |
| V4 | V2 with a title row (`<family> export <date>`) in A1, the header in row 3, and two extra leading columns (`#`, `Owner`) |
| V5 | V2 with an empty row after every 5th account |
| V6 | V2 with about 20% of password cells left empty |
| V7 | V2 with about 10% of password cells holding a placeholder (`****`, `N/A`, `TBD`, `see vault`) |
| V8 | `.xlsx` with one sheet per folder/vault (2-4 sheets), one of them hidden |

Each file holds 5-40 accounts (seeded). Values are synthetic, generated with a
fixed seed, and never derived from any real credential:

- passwords from four generators in equal shares: random 8-20 characters of
  mixed classes; 3-4 dictionary words joined by `-`; a word, a year and a
  symbol (`Harbor2023!`); a 4-8 digit PIN
- usernames: email addresses and handles; URLs: `https://` plus a synthetic
  domain; notes: short prose without secrets; TOTP: 32-character base32 (Apple:
  an `otpauth://` URI); other columns: plausible filler (dates, GUIDs, flags)

## Negative controls

Files with no credentials but with credential-like labels, one set per split
(the same generator, different seeds). The families come from false-positive
patterns seen in the Enron dev split, so they test known traps, not unknown
ones:

- help-desk log with a `Category` column whose values include `Password`
- login-form template: `User ID:`, `Password:` labels with empty values and
  further labels below them
- trading report with `...-PW` book codes; parts list with `PIN` part numbers
- password policy sheet: prose rows under a `Password` heading

## Ground truth

Known by construction and written by the generator next to each file:

- **credential:** every non-empty, non-placeholder password cell; every TOTP
  cell, reported as a separate secondary metric
- **not a credential:** usernames, emails, URLs, notes, placeholders, filler,
  and everything in the negative controls

A finding is a true positive if its cell is a credential cell and its value
is that cell's value; anything else is a false positive.

## Metrics

Per split, and per family, variant and source grade within it:

- recall of password credentials, by cell and by distinct value
- precision of all findings at confidence >= 120
- false positives per negative-control file
- secondary: recall of TOTP secrets

## Procedure

1. Commit this protocol.
2. Write the generator; commit it with a manifest of every generated file's
   SHA-256. The data is regenerated from the seed, not committed.
3. Baseline: run the current `main` on **design and holdout** and record the
   results.
4. Design detector changes using the **design** split only. Every Enron
   true positive labelled so far must still be found (no-regression gate).
5. Freeze, then run **holdout** once. If it fails and the detector is changed
   again, the holdout is spent and the docs say so.
6. Run **test** once with the frozen detector, plus the baseline on test for
   before and after. Report everything, including failures; do not inspect
   individual test files.

## Known limits

- Synthetic values: shapes are plausible but chosen by us, not observed.
- `.xlsx` variants are written with openpyxl, not saved by Excel.
- The negative controls are drawn from patterns the current detector was
  already designed against, so their precision is optimistic.
- The family list is ours; real exports may vary between versions.

## Amendments

1. **2026-10-13, after the baseline on design and holdout.** For a TOTP
   credential stored as an `otpauth://` URI (Apple `OTPAuth`), a finding on
   that cell is also a true positive when its value is exactly the URI's
   `secret` parameter. Reason: the secret, not the URI, is the credential, and
   the original definition (value equals the cell) would count a correct
   finding as a false positive. Prompted by the holdout baseline (75 such
   findings), so the holdout split is no longer clean for any change to how
   inline values are delimited; that change will be validated on test only.
2. **2026-10-13, generator fix found while analysing design misses.** openpyxl
   stored generated passwords beginning with `=` as formulas, so those `.xlsx`
   cells were formulas while the answer key called them passwords (4 such
   cells on design). The generator now stores them as text and the data and
   manifest were regenerated. Results recorded before this fix are marked as
   such in RESULTS.md.
