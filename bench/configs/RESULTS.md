# Results: configuration variables (set A2)

Method: [PROTOCOL.md](PROTOCOL.md). Reproduce with
`python bench/configs/generate.py --check` and
`python bench/configs/evaluate.py <splits>`.

## Baseline: `main` at 9ff31ce (2026-10-16), design and holdout

| Split | Recall | Precision |
|---|---|---|
| design (joomla, influxdb, wordpress, mongo-express, mysql-mariadb) | 25/407 (6.1%) | 25/30 (83.3%) |
| holdout (rabbitmq, couchdb, nextcloud) | 26/219 (11.9%) | 26/26 (100%) |

Every true positive is in layout L3 (a pasted `.env` file), found by the
inline pattern (`..._PASSWORD=value`). Every table layout (L1, L2, L4, L5,
L6) finds 0: compound labels such as `MYSQL_ROOT_PASSWORD` are not credential
labels. The 5 design false positives are flag variables in `.env` form
(`MYSQL_ALLOW_EMPTY_PASSWORD`, `MARIADB_RANDOM_ROOT_PASSWORD`,
`MYSQL_ONETIME_PASSWORD`), the hard negatives the protocol anticipated.

## Design (design split only)

Commit f58038f, designed on the design split with the Enron corpus and set A
as no-regression gates. Design split: recall 25 -> 407/407, precision 83.3% ->
100%. Interim versions, caught before commit: a list label took a neighbouring
variable's value when its own was a switch (fixed by reading values across
from the label only); `..._KEY`/`..._SALT` needed a key-material test;
`GRIES_PASS`, a book name with money amounts, produced 2 Enron false positives
(fixed by never treating signed or decimal amounts as credentials).

## Frozen detector f58038f: holdout and test, each run once

| Split | Recall | Precision |
|---|---|---|
| holdout (rabbitmq, couchdb, nextcloud) | 219/219 (100%) | 219/219 (100%) |
| **test (postgres, mongo, ghost)** | **81/81 (100%)** | **81/82 (98.8%)** |
| test, baseline d6f4124 | 12/81 (14.8%) | 12/12 (100%) |

The holdout includes Nextcloud's `OBJECTSTORE_S3_KEY`, an access key ID that
is not a secret; it is not reported because its value has no lower-case
letters and fails the key-material test.

The one test false positive is on a `POSTGRES_PASSWORD` label in layout L6
(empty and placeholder values). Per the protocol, the test file was not
inspected.

**Set A test split, re-run (not independent: Bitwarden's labels motivated
#19).** Passwords 140 -> 592 of 605, TOTP 0 -> 26 of 144, precision 100%.
TOTP stays low because `login_username` is not recognised as a table's
account label and the TOTP column is mostly empty, so it ends early; that is
noted for later work, not fixed against this split.
