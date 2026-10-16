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
