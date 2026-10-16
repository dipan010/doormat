"""Generate evaluation set A2: spreadsheets listing configuration variables.

Implements bench/configs/PROTOCOL.md. Variable names and meanings come from
the Docker Official Images documentation; every value is synthetic and seeded,
so the same command always writes byte-identical files. Each spreadsheet is
written next to a ``.truth.json`` listing its credential cells, and
``manifest.json`` records the SHA-256 of every file.

Usage:
    python bench/configs/generate.py            # writes bench/configs/data/
    python bench/configs/generate.py --check    # regenerate and compare with manifest.json
"""

from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import string
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
# Set A's seeded value generators and byte-reproducible writers.
sys.path.insert(0, str(HERE.parent / "exports"))
from generate import Rng, seed_for, write_csv, write_xlsx

DATA_DIR = HERE / "data"
MANIFEST = HERE / "manifest.json"

SPLITS = {
    "design": ["joomla", "influxdb", "wordpress", "mongo-express", "mysql-mariadb"],
    "holdout": ["rabbitmq", "couchdb", "nextcloud"],
    "test": ["postgres", "mongo", "ghost"],
}
# Family -> (variable, kind) pairs. Kinds starting with "secret" are credentials (PROTOCOL.md).
FAMILIES: dict[str, list[tuple[str, str]]] = {
    "joomla": [
        ("JOOMLA_DB_HOST", "hostport"), ("JOOMLA_DB_USER", "user"), ("JOOMLA_DB_PASSWORD", "secret"),
        ("JOOMLA_DB_PASSWORD_FILE", "secretfile"), ("JOOMLA_DB_NAME", "db"), ("JOOMLA_DB_TYPE", "dbtype"),
        ("JOOMLA_SITE_NAME", "title"), ("JOOMLA_ADMIN_USER", "fullname"), ("JOOMLA_ADMIN_USERNAME", "user"),
        ("JOOMLA_ADMIN_PASSWORD", "secret"), ("JOOMLA_ADMIN_EMAIL", "email"), ("JOOMLA_SMTP_HOST", "host"),
        ("JOOMLA_SMTP_HOST_PORT", "port"),
    ],
    "influxdb": [
        ("DOCKER_INFLUXDB_INIT_MODE", "mode"), ("DOCKER_INFLUXDB_INIT_USERNAME", "user"),
        ("DOCKER_INFLUXDB_INIT_PASSWORD", "secret"), ("DOCKER_INFLUXDB_INIT_ORG", "org"),
        ("DOCKER_INFLUXDB_INIT_BUCKET", "bucket"),
    ],
    "wordpress": [
        ("WORDPRESS_DB_HOST", "hostport"), ("WORDPRESS_DB_USER", "user"), ("WORDPRESS_DB_PASSWORD", "secret"),
        ("WORDPRESS_DB_NAME", "db"), ("WORDPRESS_TABLE_PREFIX", "prefix"), ("WORDPRESS_AUTH_KEY", "secretkey"),
        ("WORDPRESS_SECURE_AUTH_KEY", "secretkey"), ("WORDPRESS_LOGGED_IN_KEY", "secretkey"),
        ("WORDPRESS_NONCE_KEY", "secretkey"), ("WORDPRESS_AUTH_SALT", "secretkey"),
        ("WORDPRESS_SECURE_AUTH_SALT", "secretkey"), ("WORDPRESS_LOGGED_IN_SALT", "secretkey"),
        ("WORDPRESS_NONCE_SALT", "secretkey"), ("WORDPRESS_DEBUG", "flag"),
    ],
    "mongo-express": [
        ("ME_CONFIG_BASICAUTH_USERNAME", "user"), ("ME_CONFIG_BASICAUTH_PASSWORD", "secret"),
        ("ME_CONFIG_MONGODB_ENABLE_ADMIN", "bool"), ("ME_CONFIG_MONGODB_ADMINUSERNAME", "user"),
        ("ME_CONFIG_MONGODB_ADMINPASSWORD", "secret"), ("ME_CONFIG_MONGODB_PORT", "port"),
        ("ME_CONFIG_MONGODB_SERVER", "host"), ("ME_CONFIG_OPTIONS_EDITORTHEME", "theme"),
        ("ME_CONFIG_SITE_BASEURL", "basepath"), ("ME_CONFIG_SITE_COOKIESECRET", "secretkey"),
        ("ME_CONFIG_SITE_SESSIONSECRET", "secretkey"), ("ME_CONFIG_SITE_SSL_ENABLED", "bool"),
        ("ME_CONFIG_SITE_SSL_CRT_PATH", "certpath"), ("ME_CONFIG_SITE_SSL_KEY_PATH", "keypath"),
        ("ME_CONFIG_MONGODB_AUTH_DATABASE", "db"), ("ME_CONFIG_MONGODB_AUTH_USERNAME", "user"),
        ("ME_CONFIG_MONGODB_AUTH_PASSWORD", "secret"),
    ],
    "mysql-mariadb": [
        ("MYSQL_ROOT_PASSWORD", "secret"), ("MYSQL_DATABASE", "db"), ("MYSQL_USER", "user"), ("MYSQL_PASSWORD", "secret"),
        ("MYSQL_ALLOW_EMPTY_PASSWORD", "flag"), ("MYSQL_RANDOM_ROOT_PASSWORD", "flag"),
        ("MYSQL_ONETIME_PASSWORD", "flag"), ("MYSQL_INITDB_SKIP_TZINFO", "flag"),
        ("MYSQL_ROOT_PASSWORD_FILE", "secretfile"), ("MARIADB_ROOT_PASSWORD", "secret"),
        ("MARIADB_ALLOW_EMPTY_ROOT_PASSWORD", "flag"), ("MARIADB_RANDOM_ROOT_PASSWORD", "flag"),
        ("MARIADB_USER", "user"), ("MARIADB_PASSWORD", "secret"), ("MARIADB_DATABASE", "db"),
        ("MARIADB_ROOT_PASSWORD_HASH", "hash"), ("MARIADB_AUTO_UPGRADE", "flag"),
        ("MARIADB_ROOT_PASSWORD_FILE", "secretfile"),
    ],
    "rabbitmq": [
        ("RABBITMQ_DEFAULT_USER", "user"), ("RABBITMQ_DEFAULT_PASS", "secret"), ("RABBITMQ_DEFAULT_VHOST", "vhost"),
        ("RABBITMQ_SERVER_ADDITIONAL_ERL_ARGS", "erlargs"),
    ],
    "couchdb": [
        ("COUCHDB_USER", "user"), ("COUCHDB_PASSWORD", "secret"), ("COUCHDB_SECRET", "secretkey"),
        ("NODENAME", "host"), ("ERL_FLAGS", "erlargs"),
    ],
    "nextcloud": [
        ("MYSQL_DATABASE", "db"), ("MYSQL_USER", "user"), ("MYSQL_PASSWORD", "secret"), ("MYSQL_HOST", "host"),
        ("POSTGRES_DB", "db"), ("POSTGRES_USER", "user"), ("POSTGRES_PASSWORD", "secret"), ("POSTGRES_HOST", "host"),
        ("NEXTCLOUD_ADMIN_USER", "user"), ("NEXTCLOUD_ADMIN_PASSWORD", "secret"), ("NEXTCLOUD_DATA_DIR", "dir"),
        ("NEXTCLOUD_TRUSTED_DOMAINS", "domains"), ("REDIS_HOST", "host"), ("REDIS_HOST_PORT", "port"),
        ("REDIS_HOST_PASSWORD", "secret"), ("SMTP_HOST", "host"), ("SMTP_SECURE", "tls"), ("SMTP_PORT", "port"),
        ("SMTP_NAME", "email"), ("SMTP_PASSWORD", "secret"), ("MAIL_FROM_ADDRESS", "user"),
        ("OBJECTSTORE_S3_BUCKET", "bucket"), ("OBJECTSTORE_S3_REGION", "region"), ("OBJECTSTORE_S3_KEY", "accesskeyid"),
        ("OBJECTSTORE_S3_SECRET", "secretkey"), ("OBJECTSTORE_S3_SSE_C_KEY", "secretkey"),
        ("OBJECTSTORE_SWIFT_USER_NAME", "user"), ("OBJECTSTORE_SWIFT_USER_PASSWORD", "secret"),
        ("PHP_MEMORY_LIMIT", "size"),
    ],
    "postgres": [
        ("POSTGRES_PASSWORD", "secret"), ("POSTGRES_USER", "user"), ("POSTGRES_DB", "db"),
        ("POSTGRES_INITDB_ARGS", "initargs"), ("POSTGRES_INITDB_WALDIR", "dir"), ("POSTGRES_HOST_AUTH_METHOD", "authmethod"),
        ("PGDATA", "dir"), ("POSTGRES_PASSWORD_FILE", "secretfile"), ("POSTGRES_USER_FILE", "secretfile"),
        ("POSTGRES_DB_FILE", "secretfile"), ("LANG", "locale"),
    ],
    "mongo": [
        ("MONGO_INITDB_ROOT_USERNAME", "user"), ("MONGO_INITDB_ROOT_PASSWORD", "secret"), ("MONGO_INITDB_DATABASE", "db"),
        ("MONGO_INITDB_ROOT_USERNAME_FILE", "secretfile"), ("MONGO_INITDB_ROOT_PASSWORD_FILE", "secretfile"),
    ],
    "ghost": [
        ("NODE_ENV", "nodeenv"), ("url", "url"), ("database__client", "dbtype"), ("database__connection__host", "host"),
        ("database__connection__user", "user"), ("database__connection__password", "secret"),
        ("database__connection__password_FILE", "secretfile"), ("database__connection__database", "db"),
        ("mail__options__auth__pass", "secret"),
    ],
}
LAYOUTS = ["L1", "L2", "L3", "L4", "L5", "L6"]
COPIES = 3  # seeded files per family and layout (protocol amendment 1)
PLACEHOLDERS = ["****", "TBD", "see vault"]
WORDS = ["atlas", "beacon", "cobalt", "delta", "ember", "falcon", "harbor", "juniper", "nimbus", "orbit"]


def value_for(r: Rng, name: str, kind: str) -> str:
    """A synthetic value of the documented type."""
    if kind == "secret":
        return r.password()
    if kind == "secretkey":
        pool = string.ascii_letters + string.digits + "!@#$%^&*()-_=+[]{}<>~"
        return "".join(r.choice(pool) for _ in range(r.randint(40, 64)))
    simple = {
        "user": lambda: r.handle(), "fullname": lambda: f"{r.choice(WORDS).capitalize()} Admin",
        "email": lambda: f"{r.handle()}@example-mail.test", "db": lambda: f"{r.choice(WORDS)}_db",
        "host": lambda: f"{r.choice(WORDS)}.internal.test", "hostport": lambda: f"{r.choice(WORDS)}.internal.test:3306",
        "port": lambda: str(r.choice([25, 587, 3306, 5432, 6379, 27017])), "flag": lambda: r.choice(["yes", "1", "true"]),
        "bool": lambda: r.choice(["true", "false"]), "mode": lambda: "setup", "org": lambda: f"{r.choice(WORDS)}-org",
        "bucket": lambda: f"{r.choice(WORDS)}-bucket", "prefix": lambda: "wp_", "dbtype": lambda: r.choice(["mysqli", "mysql"]),
        "title": lambda: f"{r.choice(WORDS).capitalize()} Site", "theme": lambda: "rubyblue", "basepath": lambda: "/",
        "certpath": lambda: "/etc/ssl/certs/server.crt", "keypath": lambda: "/etc/ssl/private/server.key",
        "vhost": lambda: "/", "erlargs": lambda: "+K true", "dir": lambda: f"/var/lib/{r.choice(WORDS)}/data",
        "domains": lambda: "cloud.example.test cloud2.example.test", "tls": lambda: "tls", "region": lambda: "us-east-1",
        "accesskeyid": lambda: "AKIA" + "".join(r.choice(string.ascii_uppercase + string.digits) for _ in range(16)),
        "size": lambda: "512M", "initargs": lambda: "--data-checksums", "authmethod": lambda: "scram-sha-256",
        "locale": lambda: "en_US.utf8", "nodeenv": lambda: "production", "url": lambda: "https://blog.example.test",
        "secretfile": lambda: f"/run/secrets/{name.lower().removesuffix('_file')}",
        "hash": lambda: "$2y$10$" + "".join(r.choice(string.ascii_letters + string.digits + "./") for _ in range(53)),
    }
    return simple[kind]()


def choose(r: Rng, family: str) -> list[tuple[str, str]]:
    """A seeded subset of the family's variables, always with at least one secret."""
    items = [(n, k) for n, k in FAMILIES[family] if r.random() < 0.8]
    if not any(k.startswith("secret") and k != "secretfile" for _, k in items):
        items.append(next((n, k) for n, k in FAMILIES[family] if k == "secret"))
    return items


def is_secret(kind: str) -> bool:
    return kind in ("secret", "secretkey")


def build(r: Rng, family: str, layout: str) -> tuple[list, list[tuple[int, int, str]]]:
    """Return (grid, credentials as (row, col, value)) for one file."""
    items = choose(r, family)
    if layout == "L4":
        envs = ["dev", "staging", "prod"]
        grid: list = [["Environment", *[n for n, _ in items]]]
        creds = []
        for i, env in enumerate(envs):
            row = [env]
            for j, (name, kind) in enumerate(items):
                v = value_for(r, name, kind)
                row.append(v)
                if is_secret(kind):
                    creds.append((i + 2, j + 2, v))
            grid.append(row)
        return grid, creds
    rows = [(n, k, value_for(r, n, k)) for n, k in items]
    if layout == "L6":
        changed = []
        for n, k, v in rows:
            if is_secret(k):
                roll = r.random()
                v = "" if roll < 0.2 else (r.choice(PLACEHOLDERS) if roll < 0.3 else v)
            changed.append((n, k, v))
        rows = changed
    if layout == "L3":
        grid = [[f"# {family} settings"]] + [[f"{n}={v}"] for n, _, v in rows]
        return grid, [(i + 2, 1, v) for i, (_, k, v) in enumerate(rows) if is_secret(k) and v]
    if layout == "L5":
        notes = ["rotate quarterly", "from the vault", "", "set by ops"]
        grid = [["Service", "Variable", "Value", "Notes"]] + [[family, n, v or None, r.choice(notes) or None] for n, _, v in rows]
        return grid, [(i + 2, 3, v) for i, (_, k, v) in enumerate(rows) if is_secret(k) and v and v not in PLACEHOLDERS]
    grid = [["Variable", "Value"]] + [[n, v or None] for n, _, v in rows]
    return grid, [(i + 2, 2, v) for i, (_, k, v) in enumerate(rows) if is_secret(k) and v and v not in PLACEHOLDERS]


def generate(target: Path) -> dict[str, str]:
    """Write the whole set under target; return {relative path: sha256}."""
    if target.exists():
        shutil.rmtree(target)
    for split, families in SPLITS.items():
        for family in families:
            out = target / split / family
            out.mkdir(parents=True)
            for layout in LAYOUTS:
                for copy in range(COPIES):
                    r = Rng(seed_for("configs", split, family, layout, str(copy)))
                    grid, creds = build(r, family, layout)
                    ext = "csv" if layout == "L2" else "xlsx"
                    path = out / f"{family}_{layout}_{copy + 1}.{ext}"
                    sheet = path.stem if ext == "csv" else "Sheet1"
                    if ext == "csv":
                        write_csv(path, grid)
                    else:
                        write_xlsx(path, [(sheet, grid, False)])
                    truth = {"family": family, "layout": layout, "variant": layout, "split": split, "grade": "primary",
                             "credentials": [{"sheet": sheet, "row": rr, "col": c, "kind": "password", "value": v}
                                             for rr, c, v in creds],
                             "non_secret_variables": sorted(n for n, k in FAMILIES[family] if not is_secret(k))}
                    path.with_name(path.name + ".truth.json").write_text(json.dumps(truth, indent=1) + "\n")
    return {str(p.relative_to(target)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(target.rglob("*")) if p.is_file()}


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="regenerate and compare with manifest.json")
    opts = parser.parse_args()
    digests = generate(DATA_DIR)
    if opts.check:
        expected = json.loads(MANIFEST.read_text())
        changed = sorted(k for k in expected.keys() | digests.keys() if expected.get(k) != digests.get(k))
        if changed:
            raise SystemExit(f"{len(changed)} files differ from the manifest, e.g. {changed[:3]}")
        print(f"{len(digests)} files match the manifest")
        return
    MANIFEST.write_text(json.dumps(digests, indent=1) + "\n")
    print(f"{len(digests)} files written to {DATA_DIR}")


if __name__ == "__main__":
    main()
