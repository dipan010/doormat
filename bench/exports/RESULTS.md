# Results: credential exports (set A)

Method: [PROTOCOL.md](PROTOCOL.md). Reproduce with
`python bench/exports/generate.py --check` and
`python bench/exports/evaluate.py <splits>`.

## Baseline: `main` at b17dc8e (2026-10-13), design and holdout

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
