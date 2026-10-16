# Results: credential exports (set A)

Method: [PROTOCOL.md](PROTOCOL.md). Reproduce with
`python bench/exports/generate.py --check` and
`python bench/exports/evaluate.py <splits>`.

## Baseline: `main` at b17dc8e (2026-10-13), design and holdout

Recorded before amendment 2 (generator fix for passwords beginning with `=`).

| Split | Password recall | Precision | Negative-control false positives | TOTP recall |
|---|---|---|---|---|
| design (keepassxc, firefox, protonpass, dashlane) | 623/844 (73.8%) | 623/636 (98.0%) | 12 in 5 files | 0/178 |
| holdout (apple-passwords, lastpass) | 449/451 (99.6%) | 449/536 (83.8%) | 11 in 5 files | 0/140 |

Per family (passwords): keepassxc 194/195, firefox 229/231, protonpass
189/189, **dashlane 11/229**, apple-passwords 239/240, lastpass 210/211.

Causes found on the design split:

1. Dashlane puts `username` 4 columns from `password`; the credential-table
   rule only looks 3 columns away, so plain pairing reports one value per
   header (218 of the 221 design misses).
2. Help-desk control: a `Password` category value in a ticket row pairs with
   the `Unlock`/`Reset` cell beside it at exactly 120 (11 false positives).
3. Login-form control: an empty `Password:` field pairs with the `Date`
   header below it (1 false positive; issue #7).
4. TOTP columns (`totp`, `TOTP`, `otpSecret`) are not credential labels.

Holdout: 75 of the 87 false positives are on Apple `OTPAuth` cells. A probe
on a synthetic URI (not holdout data) shows inline detection finds the secret
in `otpauth://...?secret=...&issuer=...` but extends the value past `&`. See
amendment 1.

## Design changes (design split only)

Each change was checked on the design split and then on the full Enron
corpus as a no-regression gate (every labelled true positive must still be
found).

| Commit | Change | Design split | Enron gate |
|---|---|---|---|
| 8bfb96f | account label up to 10 columns from the password header | passwords 623 -> 824/844 | no change |
| 8bfb96f | label repeated 3+ times in its column is a category value | negative-control FPs 12 -> 1 | 47 FPs removed, 0 TPs lost |
| 5053cea | xlsx text starting with `=` is a value (extractor bug found by set A) | passwords 824 -> 828 | no change |
| 1d66cf7 | table rows follow the account column; `totp`/`otpsecret` labels; rubric placeholders skipped | passwords 828 -> 844/844, TOTP 0 -> 178/178 | no change |
| f34cc22 | empty `Password:` form field is not paired (#7) | negative-control FPs 1 -> 0, precision 100% | 51 FPs removed, 0 TPs lost |
| 94c1ede | inline value in a URL query ends at the next `&` | no change | no change |

The first version of the category rule also suppressed headers of stacked
credential tables and lost 8 Enron true positives; it was changed so a
repeated label may still head a table. The first version of the
account-column change took a password label as the account column; both were
caught before commit.

## Frozen detector 94c1ede: holdout and test, each run once

| Split | Password recall | Precision | Negative-control FPs | TOTP recall |
|---|---|---|---|---|
| holdout (apple-passwords, lastpass) | 451/451 (100%) | 591/591 (100%) | 0 | 140/140 |
| **test (bitwarden, chromium)** | **140/605 (23.1%)** | 140/140 (100%) | 0 | 0/144 |
| test, baseline b17dc8e | 135/605 (22.3%) | 135/151 (89.4%) | 15 | 0/144 |

Holdout caveats: its 74 Apple TOTP secrets are found by the URL-query fix,
for which the holdout is not clean (amendment 1); the 66 LastPass TOTP
secrets come from the `totp` label and are clean.

**Test, per family:** chromium 140/140; **bitwarden 0/465 passwords, 0/144
TOTP**. Bitwarden's headers are `login_password` and `login_totp`, which
normalise to `loginpassword` and `logintotp`; header labels must match a
keyword exactly, so neither is recognised. This is a real generalisation gap,
not a defect of the set: compound labels (a prefix plus a credential keyword)
are not modelled. It was not fixed by adding Bitwarden's names, which would
tune the detector to the test split; it is tracked as a separate change that
needs new evaluation data.

Precision is 100% on every split, but the negative controls are drawn from
patterns the detector was designed against, and values are synthetic; read
the precision figures as a check against known traps, not as field
precision.

## Effect on the Enron corpus (all changes)

| | b17dc8e | 94c1ede |
|---|---|---|
| test precision | 80.6% (108/134) | 90.0% (108/120) |
| test recall | 61.4% (108/176) | 61.4% (108/176) |
| dev precision | 66.0% (188/285) | 93.5% (188/201) |

No labelled true positive was lost. The false positives removed belong to
families first seen on Enron dev (help-desk categories, empty form fields),
which also shaped the negative controls, so this is not out-of-sample
evidence on Enron.
