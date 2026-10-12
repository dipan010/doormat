"""Generate evaluation set A: spreadsheets laid out like credential exports.

Implements bench/exports/PROTOCOL.md. Every value is synthetic and comes from
a seeded generator, so the same command always writes byte-identical files.
Each spreadsheet is written next to a ``.truth.json`` file listing its
credential cells, and ``manifest.json`` records the SHA-256 of every file.

Usage:
    python bench/exports/generate.py            # writes bench/exports/data/
    python bench/exports/generate.py --check    # regenerate and compare with manifest.json
"""

from __future__ import annotations

import argparse
import base64
import csv
import hashlib
import io
import json
import random
import re
import shutil
import string
import uuid
import zipfile
from collections.abc import Callable
from pathlib import Path

import openpyxl

HERE = Path(__file__).resolve().parent
DATA_DIR = HERE / "data"
MANIFEST = HERE / "manifest.json"
FIXED_ZIP_TIME = (1980, 1, 1, 0, 0, 0)
FIXED_DOC_TIME = "2026-01-01T00:00:00Z"

SPLITS = {
    "design": ["keepassxc", "firefox", "protonpass", "dashlane"],
    "holdout": ["apple-passwords", "lastpass"],
    "test": ["bitwarden", "chromium"],
}
GRADES = {
    "bitwarden": "primary", "keepassxc": "primary", "firefox": "primary", "chromium": "primary",
    "lastpass": "secondary", "protonpass": "secondary", "apple-passwords": "secondary", "dashlane": "secondary",
}
# Layout -> columns as (header, kind). Kinds are filled by Rng.value().
LAYOUTS: dict[str, list[tuple[str, str]]] = {
    "bitwarden": [
        ("folder", "folder"), ("favorite", "flag"), ("type", "login"), ("name", "title"), ("notes", "note"),
        ("fields", "empty"), ("reprompt", "zero"), ("login_uri", "url"), ("login_username", "username"),
        ("login_password", "password"), ("login_totp", "totp"),
    ],
    "bitwarden-org": [
        ("collections", "folder"), ("type", "login"), ("name", "title"), ("notes", "note"), ("fields", "empty"),
        ("reprompt", "zero"), ("login_uri", "url"), ("login_username", "username"),
        ("login_password", "password"), ("login_totp", "totp"),
    ],
    "keepassxc": [
        ("Group", "group"), ("Title", "title"), ("Username", "username"), ("Password", "password"),
        ("URL", "url"), ("Notes", "note"), ("TOTP", "totp"), ("Icon", "zero"),
        ("Last Modified", "timestamp"), ("Created", "timestamp"),
    ],
    "firefox": [
        ("url", "url"), ("username", "username"), ("password", "password"), ("httpRealm", "empty"),
        ("formActionOrigin", "url"), ("guid", "guid"), ("timeCreated", "epoch_ms"),
        ("timeLastUsed", "epoch_ms"), ("timePasswordChanged", "epoch_ms"),
    ],
    "chromium": [("name", "title"), ("url", "url"), ("username", "username"), ("password", "password"), ("note", "note")],
    "lastpass": [
        ("url", "url"), ("username", "username"), ("password", "password"), ("totp", "totp"), ("extra", "note"),
        ("name", "title"), ("grouping", "folder"), ("fav", "flag"),
    ],
    "protonpass": [
        ("name", "title"), ("url", "url"), ("email", "email"), ("username", "username"), ("password", "password"),
        ("note", "note"), ("totp", "totp"), ("vault", "folder"),
    ],
    "apple-passwords": [
        ("Title", "title"), ("URL", "url"), ("Username", "username"), ("Password", "password"),
        ("Notes", "note"), ("OTPAuth", "otpauth"),
    ],
    "dashlane": [
        ("username", "username"), ("username2", "empty"), ("username3", "empty"), ("title", "title"),
        ("password", "password"), ("note", "note"), ("url", "url"), ("category", "folder"), ("otpSecret", "totp"),
    ],
}
FAMILY_LAYOUTS = {"bitwarden": ["bitwarden", "bitwarden-org"]}
PLACEHOLDERS = ["****", "N/A", "TBD", "see vault"]
WORDS = ["anchor", "apple", "arrow", "aspen", "autumn", "badger", "bamboo", "banner", "barrel", "beacon", "birch", "blanket", "bottle", "breeze", "bridge", "bronze", "cactus", "candle", "canyon", "carbon", "castle", "cedar", "chalk", "cherry", "cinder", "cloud", "cobalt", "comet", "copper", "coral", "cotton", "crane", "crystal", "dawn", "delta", "desert", "dolphin", "dragon", "ember", "falcon", "feather", "fern", "fiddle", "forest", "fossil", "garden", "garnet", "ginger", "glacier", "granite", "gravel", "harbor", "hazel", "heron", "hollow", "honey", "island", "ivory", "jasper", "kettle", "lagoon", "lantern", "lemon", "lily", "linen", "lotus", "maple", "marble", "meadow", "mercury", "mint", "mosaic", "nectar", "nickel", "oasis", "ocean", "olive", "onyx", "orbit", "orchid", "otter", "pebble", "pepper", "pine", "planet", "plaza", "prairie", "quartz", "quill", "raven", "reef", "ridge", "river", "robin", "saffron", "salmon", "sandal", "shadow", "silver", "sparrow", "spruce", "stone", "summit", "sunset", "thistle", "thunder", "timber", "topaz", "tulip", "velvet", "violet", "walnut", "willow", "winter", "zephyr"]
FIRST = ["alex", "sam", "jordan", "casey", "morgan", "riley", "taylor", "jamie", "avery", "quinn", "drew", "blake"]
LAST = ["hart", "lane", "moss", "reyes", "stone", "vance", "wells", "young", "cole", "diaz", "ford", "grant"]
DOMAINS = ["example-mail.test", "corp-sample.test", "acme-demo.test", "northwind.test"]
SITES = ["portal", "billing", "intranet", "crm", "payroll", "vpn", "git", "wiki", "mail", "backup"]
NOTES = [
    "Shared with the ops team.", "Renewed last quarter.", "Used for the monthly report.",
    "Ask before changing.", "Legacy account, keep for audits.", "",
]
FOLDERS = ["Work", "Finance", "Personal", "Infrastructure"]


class Rng(random.Random):
    """Seeded generator for synthetic account values."""

    def password(self) -> str:
        style = self.randrange(4)
        if style == 0:
            pool = string.ascii_letters + string.digits + "!#$%&*+-=?@^_"
            while True:
                value = "".join(self.choice(pool) for _ in range(self.randint(8, 20)))
                if any(c.isdigit() for c in value) and any(c.isalpha() for c in value):
                    return value
        if style == 1:
            return "-".join(self.choice(WORDS) for _ in range(self.randint(3, 4)))
        if style == 2:
            return f"{self.choice(WORDS).capitalize()}{self.randint(1990, 2026)}{self.choice('!#$%&*?@')}"
        return "".join(self.choice(string.digits) for _ in range(self.randint(4, 8)))

    def handle(self) -> str:
        return f"{self.choice(FIRST)}.{self.choice(LAST)}{self.randint(1, 99)}"

    def totp(self) -> str:
        return base64.b32encode(bytes(self.getrandbits(8) for _ in range(20))).decode()

    def value(self, kind: str, folder: str) -> str | int:
        if kind == "password":
            return self.password()
        if kind == "username":
            return self.handle() if self.random() < 0.5 else f"{self.handle()}@{self.choice(DOMAINS)}"
        if kind == "email":
            return f"{self.handle()}@{self.choice(DOMAINS)}"
        if kind == "url":
            return f"https://{self.choice(SITES)}.{self.choice(DOMAINS)}/login"
        if kind == "title":
            return f"{self.choice(SITES).capitalize()} {self.choice(['account', 'login', 'admin', 'service'])}"
        if kind == "note":
            return self.choice(NOTES)
        if kind == "totp":
            return self.totp() if self.random() < 0.3 else ""
        if kind == "otpauth":
            return f"otpauth://totp/{self.handle()}?secret={self.totp()}&issuer=Sample" if self.random() < 0.3 else ""
        if kind == "folder":
            return folder
        if kind == "group":
            return f"Root/{folder}"
        if kind == "flag":
            return self.choice([0, 1])
        if kind == "zero":
            return 0
        if kind == "login":
            return "login"
        if kind == "guid":
            return "{" + str(uuid.UUID(int=self.getrandbits(128))) + "}"
        if kind == "epoch_ms":
            return self.randint(1_600_000_000_000, 1_790_000_000_000)
        if kind == "timestamp":
            return f"2025-{self.randint(1, 12):02d}-{self.randint(1, 28):02d}T{self.randint(0, 23):02d}:00:00Z"
        return ""


def seed_for(*parts: str) -> int:
    """Stable seed from names, independent of Python's hash randomisation."""
    return int(hashlib.sha256("/".join(parts).encode()).hexdigest()[:16], 16)


# ---------- Building sheets ----------

Grid = list[list[str | int | None]]  # rows of cells; None is an empty cell
Truth = list[dict]


def accounts(rng: Rng, layout: str, n: int, folders: list[str]) -> list[dict[str, str | int]]:
    """Generate n accounts as {header: value}."""
    rows = []
    for _ in range(n):
        folder = rng.choice(folders)
        rows.append({header: rng.value(kind, folder) for header, kind in LAYOUTS[layout]})
    return rows


def recase(header: str, how: str) -> str:
    if how == "title":
        return re.sub(r"(^|[_ ])([a-z])", lambda m: m.group(1) + m.group(2).upper(), header)
    if how == "upper":
        return header.upper()
    return header


def sheet_grid(layout: str, rows: list[dict], *, case: str = "", title_row: str = "", lead: bool = False,
               gap_every: int = 0, start_index: int = 1) -> tuple[Grid, list[tuple[int, int, str, str]]]:
    """Lay accounts out as a grid; return it and the (row, col, kind, value) credential cells (1-based)."""
    headers = [h for h, _ in LAYOUTS[layout]]
    kinds = dict(LAYOUTS[layout])
    grid: Grid = []
    if title_row:
        grid += [[title_row], []]
    grid.append((["#", "Owner"] if lead else []) + [recase(h, case) for h in headers])
    secrets = []
    offset = 2 if lead else 0
    for i, account in enumerate(rows):
        cells: list[str | int | None] = ([start_index + i, f"{FIRST[i % len(FIRST)].capitalize()} {LAST[i % len(LAST)].capitalize()}"] if lead else [])
        cells += [account[h] if account[h] != "" else None for h in headers]
        grid.append(cells)
        r = len(grid)
        for j, h in enumerate(headers):
            v = account[h]
            if kinds[h] == "password" and v not in ("", None) and v not in PLACEHOLDERS:
                secrets.append((r, offset + j + 1, "password", str(v)))
            if kinds[h] in ("totp", "otpauth") and v:
                secrets.append((r, offset + j + 1, "totp", str(v)))
        if gap_every and (i + 1) % gap_every == 0 and i + 1 < len(rows):
            grid.append([])
    return grid, secrets


def thin(rng: Rng, rows: list[dict], layout: str, share: float, replacement: Callable[[], str]) -> None:
    """Replace about `share` of the password cells with replacement()."""
    header = next(h for h, k in LAYOUTS[layout] if k == "password")
    for account in rows:
        if rng.random() < share:
            account[header] = replacement()


# ---------- Writing files ----------

def deterministic_xlsx(wb: openpyxl.Workbook, path: Path) -> None:
    """Save with fixed zip timestamps and document dates so the bytes are reproducible."""
    buf = io.BytesIO()
    wb.save(buf)
    wb.close()
    with zipfile.ZipFile(io.BytesIO(buf.getvalue())) as src, zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as dst:
        for item in src.infolist():
            data = src.read(item.filename)
            if item.filename == "docProps/core.xml":
                data = re.sub(rb"(<dcterms:(?:created|modified)[^>]*>)[^<]*", rb"\g<1>" + FIXED_DOC_TIME.encode(), data)
            info = zipfile.ZipInfo(item.filename, FIXED_ZIP_TIME)
            info.compress_type = zipfile.ZIP_DEFLATED
            info.external_attr = 0o600 << 16
            dst.writestr(info, data)


def write_xlsx(path: Path, sheets: list[tuple[str, Grid, bool]]) -> None:
    wb = openpyxl.Workbook()
    wb.remove(wb.active)
    for title, grid, hidden in sheets:
        ws = wb.create_sheet(title)
        for row in grid:
            ws.append(row)
        if hidden:
            ws.sheet_state = "hidden"
    deterministic_xlsx(wb, path)


def write_csv(path: Path, grid: Grid) -> None:
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.writer(f, lineterminator="\n")
        for row in grid:
            writer.writerow(["" if v is None else v for v in row])


def write_truth(path: Path, family: str, layout: str, variant: str, split: str,
                cells: list[tuple[str, int, int, str, str]]) -> None:
    truth = {
        "family": family, "layout": layout, "variant": variant, "split": split, "grade": GRADES.get(family, ""),
        "credentials": [{"sheet": s, "row": r, "col": c, "kind": k, "value": v} for s, r, c, k, v in cells],
    }
    path.with_name(path.name + ".truth.json").write_text(json.dumps(truth, indent=1, ensure_ascii=False) + "\n")


# ---------- Variants ----------

def generate_layout(out: Path, split: str, family: str, layout: str) -> None:
    def rng(variant: str) -> Rng:
        return Rng(seed_for(split, layout, variant))

    def base(variant: str) -> tuple[Rng, list[dict]]:
        r = rng(variant)
        return r, accounts(r, layout, r.randint(5, 40), FOLDERS[:1])

    def single(variant: str, ext: str, grid: Grid, secrets: list, sheet: str = "Sheet1") -> None:
        path = out / f"{layout}_{variant}.{ext}"
        sheet_name = path.stem if ext == "csv" else sheet
        if ext == "csv":
            write_csv(path, grid)
        else:
            write_xlsx(path, [(sheet, grid, False)])
        write_truth(path, family, layout, variant, split, [(sheet_name, r, c, k, v) for r, c, k, v in secrets])

    _, rows = base("V1")
    single("V1", "csv", *sheet_grid(layout, rows))
    _, rows = base("V2")
    single("V2", "xlsx", *sheet_grid(layout, rows))
    for how in ("title", "upper"):
        _, rows = base(f"V3-{how}")
        single(f"V3-{how}", "xlsx", *sheet_grid(layout, rows, case=how))
    _, rows = base("V4")
    single("V4", "xlsx", *sheet_grid(layout, rows, title_row=f"{family} export 2026-01-15", lead=True))
    _, rows = base("V5")
    single("V5", "xlsx", *sheet_grid(layout, rows, gap_every=5))
    r, rows = base("V6")
    thin(r, rows, layout, 0.2, lambda: "")
    single("V6", "xlsx", *sheet_grid(layout, rows))
    r, rows = base("V7")
    thin(r, rows, layout, 0.1, lambda: r.choice(PLACEHOLDERS))
    single("V7", "xlsx", *sheet_grid(layout, rows))

    r = rng("V8")
    folders = FOLDERS[: r.randint(2, 4)]
    hidden = r.choice(folders)
    sheets, cells = [], []
    for folder in folders:
        rows = accounts(r, layout, r.randint(5, 15), [folder])
        grid, secrets = sheet_grid(layout, rows)
        sheets.append((folder, grid, folder == hidden))
        cells += [(folder, rr, c, k, v) for rr, c, k, v in secrets]
    path = out / f"{layout}_V8.xlsx"
    write_xlsx(path, sheets)
    write_truth(path, family, layout, "V8", split, cells)


def generate_negatives(out: Path, split: str) -> None:
    r = Rng(seed_for(split, "negatives"))
    tickets: Grid = [["Ticket", "Opened", "Customer", "Category", "Action", "Notes"]]
    for i in range(r.randint(15, 30)):
        tickets.append([
            f"T{r.randint(100000, 999999)}", f"2025-{r.randint(1, 12):02d}-{r.randint(1, 28):02d}",
            f"{r.choice(WORDS).capitalize()} {r.choice(['Energy', 'Gas', 'Power', 'Services'])}",
            r.choice(["Password", "Password", "Account Lockout", "Access", "Other"]), r.choice(["Reset", "Unlock", "Other"]),
            r.choice(["Reset her password and she logged in.", "Unlocked the account.", "Called back, no answer.",
                      "Walked him through the login page."]),
        ])
    form: Grid = [["User ID:", None, None, "Region"], ["Password:"], [], ["Date", "Price", "Basis", "Index"]]
    form += [[f"2025-01-{d:02d}", round(r.uniform(2, 9), 3), round(r.uniform(-1, 1), 3), r.choice(["HH", "NYMEX"])] for d in range(1, 11)]
    books = [f"{r.choice(['ADCCFXX', 'ASTSTXX', 'AGDCTXX'])}.{r.randint(1, 19):02d}-PW" for _ in range(6)]
    trading: Grid = [["Date", *books]] + [[f"2025-02-{d:02d}", *[round(r.uniform(-5000, 5000), 2) for _ in books]] for d in range(1, 21)]
    parts: Grid = [["Item", "Part No", "Description", "Qty", "Price"]]
    parts += [[i, f"{r.randint(100, 999)}A{r.randint(1000, 9999)}P{r.randint(1, 99):03d}",
               r.choice(["PIN-DOWEL", "GROOVE PIN", "PIN-LOCK", "PIN-STRT HDLS", "BEARING"]), r.randint(1, 20),
               round(r.uniform(1, 90), 2)] for i in range(1, r.randint(12, 25))]
    policy: Grid = [["Password"], ["Must be at least 12 characters long."], ["Change it every 90 days."],
                    ["Do not reuse the last 5 passwords."], ["Never share it by email."]]
    for name, grid in [("helpdesk", tickets), ("login_form", form), ("trading_pw", trading), ("parts_pin", parts), ("policy", policy)]:
        path = out / f"negative_{name}.xlsx"
        write_xlsx(path, [("Sheet1", grid, False)])
        write_truth(path, "negative", name, "negative", split, [])


def generate(target: Path) -> dict[str, str]:
    """Write the whole set under target; return {relative path: sha256}."""
    if target.exists():
        shutil.rmtree(target)
    for split, families in SPLITS.items():
        for family in families:
            out = target / split / family
            out.mkdir(parents=True)
            for layout in FAMILY_LAYOUTS.get(family, [family]):
                generate_layout(out, split, family, layout)
        neg = target / split / "negative"
        neg.mkdir(parents=True)
        generate_negatives(neg, split)
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
