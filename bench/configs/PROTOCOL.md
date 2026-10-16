# Evaluation protocol: configuration variables (set A2)

Written and committed **before** any generator code, data or detection result
for this set exists. Later changes are listed under "Amendments" with a reason.

## Question

Issue #19: credential labels are matched as whole keywords, so compound labels
such as `login_password`, `MYSQL_ROOT_PASSWORD` or `RABBITMQ_DEFAULT_PASS` are
not recognised (set A's test split found 0 of 465 Bitwarden passwords). How
well does doormat find secrets in spreadsheets that list configuration
variables, a layout common in IT handover sheets, and how often does it flag a
variable that is not a secret?

The variable names come from the Docker Official Images documentation, not
from doormat, so they test a general rule rather than names we chose. They
include hard negatives that a naive "label contains `password`" rule would
flag: `*_PASSWORD_FILE` (a path), `MYSQL_RANDOM_ROOT_PASSWORD` and
`*_ALLOW_EMPTY_*PASSWORD` (flags), `MARIADB_ROOT_PASSWORD_HASH` (a hash) and
`ME_CONFIG_SITE_SSL_KEY_PATH` (a path).

## Families

Each family's variables and their meaning are taken from
`github.com/docker-library/docs/<image>/content.md` (checked 2026-10-16).
**Secret** variables hold a usable credential, key, salt or signing secret;
every other variable is **not a secret**.

| Family | Secret variables | Notable non-secrets |
|---|---|---|
| joomla | `JOOMLA_DB_PASSWORD`, `JOOMLA_ADMIN_PASSWORD` | `JOOMLA_DB_PASSWORD_FILE`, `JOOMLA_ADMIN_USERNAME`, `JOOMLA_DB_HOST` |
| influxdb | `DOCKER_INFLUXDB_INIT_PASSWORD` | `DOCKER_INFLUXDB_INIT_USERNAME`, `_MODE`, `_ORG`, `_BUCKET` |
| wordpress | `WORDPRESS_DB_PASSWORD`, `WORDPRESS_AUTH_KEY`, `_SECURE_AUTH_KEY`, `_LOGGED_IN_KEY`, `_NONCE_KEY`, `_AUTH_SALT`, `_SECURE_AUTH_SALT`, `_LOGGED_IN_SALT`, `_NONCE_SALT` | `WORDPRESS_DB_USER`, `_DB_HOST`, `_TABLE_PREFIX`, `_DEBUG` |
| mongo-express | `ME_CONFIG_BASICAUTH_PASSWORD`, `ME_CONFIG_MONGODB_ADMINPASSWORD`, `ME_CONFIG_MONGODB_AUTH_PASSWORD`, `ME_CONFIG_SITE_COOKIESECRET`, `ME_CONFIG_SITE_SESSIONSECRET` | `ME_CONFIG_MONGODB_ADMINUSERNAME`, `ME_CONFIG_SITE_SSL_KEY_PATH`, `ME_CONFIG_MONGODB_ENABLE_ADMIN` |
| mysql-mariadb | `MYSQL_ROOT_PASSWORD`, `MYSQL_PASSWORD`, `MARIADB_ROOT_PASSWORD`, `MARIADB_PASSWORD` | `MYSQL_ALLOW_EMPTY_PASSWORD`, `MYSQL_RANDOM_ROOT_PASSWORD`, `MYSQL_ONETIME_PASSWORD`, `MYSQL_ROOT_PASSWORD_FILE`, `MARIADB_ALLOW_EMPTY_ROOT_PASSWORD`, `MARIADB_RANDOM_ROOT_PASSWORD`, `MARIADB_ROOT_PASSWORD_HASH`, `MARIADB_ROOT_PASSWORD_FILE` |
| rabbitmq | `RABBITMQ_DEFAULT_PASS` | `RABBITMQ_DEFAULT_USER`, `RABBITMQ_DEFAULT_VHOST` |
| couchdb | `COUCHDB_PASSWORD`, `COUCHDB_SECRET` | `COUCHDB_USER`, `NODENAME`, `ERL_FLAGS` |
| nextcloud | `MYSQL_PASSWORD`, `POSTGRES_PASSWORD`, `NEXTCLOUD_ADMIN_PASSWORD`, `REDIS_HOST_PASSWORD`, `SMTP_PASSWORD`, `OBJECTSTORE_S3_SECRET`, `OBJECTSTORE_S3_SSE_C_KEY`, `OBJECTSTORE_SWIFT_USER_PASSWORD` | `OBJECTSTORE_S3_KEY` (an access key ID), `SMTP_NAME`, `REDIS_HOST_PORT`, `NEXTCLOUD_ADMIN_USER` |
| postgres | `POSTGRES_PASSWORD` | `POSTGRES_PASSWORD_FILE`, `POSTGRES_USER`, `POSTGRES_HOST_AUTH_METHOD`, `PGDATA` |
| mongo | `MONGO_INITDB_ROOT_PASSWORD` | `MONGO_INITDB_ROOT_PASSWORD_FILE`, `MONGO_INITDB_ROOT_USERNAME` |
| ghost | `database__connection__password`, `mail__options__auth__pass` | `database__connection__password_FILE`, `database__connection__user`, `url` |

MySQL and MariaDB are one family because the MariaDB image is a fork using
the same conventions. `MARIADB_ROOT_PASSWORD_HASH` is counted as not a secret:
a hash is sensitive but not a usable password. Each file uses a seeded subset
of its family's documented variables; the generator lists the full set per
family.

## Splits

By family, sorted by the SHA-256 of the family name: the first 5 are
**design**, the next 3 **holdout**, the last 3 **test**.

- design: joomla, influxdb, wordpress, mongo-express, mysql-mariadb
- holdout: rabbitmq, couchdb, nextcloud
- test: postgres, mongo, ghost

Some variable names recur across families (Nextcloud documents
`MYSQL_PASSWORD` and `POSTGRES_PASSWORD`). Therefore **no specific variable
name may be added to doormat's label lists**; only general rules about how a
label is built. Breaking that rule voids the holdout and test results.

## Layouts

| ID | Layout |
|---|---|
| L1 | Two columns with a header row `Variable`, `Value`; one variable per row (`.xlsx`) |
| L2 | L1 as `.csv` |
| L3 | A pasted `.env` file: one cell per line in column A, `NAME=value` |
| L4 | One row of variable names as headers; one row per environment (`dev`, `staging`, `prod`) below, with an `Environment` column first |
| L5 | A handover sheet: `Service`, `Variable`, `Value`, `Notes` |
| L6 | L1 with about 20% of secret values empty and about 10% holding a placeholder (`****`, `TBD`, `see vault`) |

## Values

Synthetic and seeded. Secrets use set A's four password generators; WordPress
keys and salts, cookie and session secrets and the S3 secret are 40-64
character random strings. Non-secrets get values of their documented type:
usernames, database and bucket names, hostnames and ports, `/run/secrets/...`
paths for `*_FILE` variables, `yes` or `true` for flags, a 60-character
`$2y$...` string for the password hash, `AKIA` plus 16 characters for the S3
access key ID, and so on.

## Ground truth

Every non-empty, non-placeholder value of a secret variable is a credential.
In L3, the credential is the part after `=`. A finding is a true positive if
its cell is a credential cell and its value is the credential; anything else
is a false positive.

## Metrics

Per split, and per family and layout within it: recall by cell and by
distinct value; precision of all findings at confidence >= 120; false
positives on non-secret variables, listed by variable, so that the
hard-negative traps are visible.

## Procedure

1. Commit this protocol.
2. Commit the generator with a manifest of SHA-256 digests; the data is
   regenerated from the seed, not committed.
3. Baseline: run `main` on **design and holdout**; record the results.
4. Design on **design** only. Every labelled Enron true positive must still be
   found, and set A's design and holdout results must not get worse.
5. Freeze; run **holdout** once. If it fails and the detector changes again,
   the holdout is spent and the docs say so.
6. Run **test** once with the frozen detector and the baseline on test. Report
   everything. Also re-run set A's test split and report it as not
   independent: Bitwarden's labels motivated #19.

## Known limits

- Synthetic values; variable names and meanings are real but the files are
  ours.
- Configuration sheets are one layout family of compound labels; credential
  exports with compound headers (Bitwarden) are only in set A's spent test
  split.

## Amendments

1. **2026-10-16, before any detection result.** Three seeded files per family
   and layout instead of one. With one file each, the test split held only 28
   credentials (its three families document one or two secret variables
   each); with three it holds 81. No result had been produced for any split.
