"""Allowlists and baselines for ``doormat scan`` (design decision 0020).

Both are JSON files keyed by where a finding is, never by its value. Paths in
them are ``/``-separated and relative to the file's own directory, so a scan
started from any directory, or on the few files pre-commit passes, matches the
same entries. A finding in a file outside that directory matches nothing.

A baseline may also carry, per entry, an HMAC-SHA256 tag of the value bound to
its location, keyed by the ``DOORMAT_BASELINE_KEY`` environment variable; a
changed value in a baselined cell is then reported as new.
"""

from __future__ import annotations

import fnmatch
import hashlib
import hmac
import json
import os
import tempfile
from collections.abc import Iterable
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

from doormat.api import Relationship

FORMAT_VERSION = 1
KEY_ENV = "DOORMAT_BASELINE_KEY"
MIN_KEY_BYTES = 16
KEY_LIMIT = 40
"""Labels longer than this are cut in masked output and in baselines. For
inline, formula and comment findings the label is all the text before the
value, which can hold a user name or another secret."""

_TAG_CONTEXT = "doormat-baseline-v1"

DETECTORS = {
    "distance": "spatial",
    "split_password": "split",  # nosec B105 - a detection pathway name, not a password
    "table_column": "table",
    "variable_value": "variable",
    "inline_same_cell": "inline",
    "formula_concatenated": "formula",
    "formula_split_strings": "formula",
    "comment_on_header_cell": "comment",
    "comment_contains_credential": "comment",
}
"""The first token of a finding's ``reason`` -> its detection pathway. The
pathway is part of a baseline entry, so these names are a file-format contract."""

_ENTRY_FIELDS = ("path", "sheet", "cell", "header_cell", "label", "detector")

Entry = tuple[str, ...]
"""A baseline entry without its path: sheet, cell, label cell, label, detector, tag."""


class SuppressionError(ValueError):
    """A baseline or allowlist that cannot be used, or a missing or short key."""


def detector(reason: str) -> str:
    """The detection pathway named by the first token of a finding's ``reason``."""
    token = reason.split(";", 1)[0].split("=", 1)[0]
    return DETECTORS.get(token, token)


def label(rel: Relationship) -> str:
    """The finding's label, cut to ``KEY_LIMIT`` characters."""
    return rel.key if len(rel.key) <= KEY_LIMIT else rel.key[:KEY_LIMIT] + "..."


def relative_path(path: str, base: Path) -> str | None:
    """``path`` relative to the directory ``base``, ``/``-separated, or None outside it."""
    try:
        return Path(path).resolve().relative_to(base).as_posix()
    except ValueError:
        return None


def baseline_key() -> bytes | None:
    """The baseline key from ``DOORMAT_BASELINE_KEY``, or None when it is unset or empty."""
    raw = os.environ.get(KEY_ENV, "")
    if not raw:
        return None
    key = raw.encode()
    if len(key) < MIN_KEY_BYTES:
        raise SuppressionError(f"{KEY_ENV} must be at least {MIN_KEY_BYTES} bytes")
    return key


def _tag(key: bytes, path: str, rel: Relationship) -> str:
    # The location is part of the message, so cells sharing a value get different tags.
    message = json.dumps([_TAG_CONTEXT, path, rel.sheet, rel.coordinate, rel.value])
    return hmac.new(key, message.encode(), hashlib.sha256).hexdigest()


def _read_json(file: Path, kind: str) -> dict[str, Any]:
    try:
        data = json.loads(file.read_text(encoding="utf-8"))
    except FileNotFoundError:
        raise SuppressionError(f"{kind} not found: {file}") from None
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise SuppressionError(f"{kind} {file} is not readable JSON: {exc}") from None
    if not isinstance(data, dict) or data.get("version") != FORMAT_VERSION:
        raise SuppressionError(f'{kind} {file}: expected an object with "version": 1')
    return data


def _check_keys(item: dict[str, Any], required: set[str], optional: set[str], where: str) -> None:
    unknown = sorted(set(item) - required - optional)
    missing = sorted(required - set(item))
    if unknown:
        raise SuppressionError(f"{where}: unknown field(s) {unknown}")
    if missing:
        raise SuppressionError(f"{where}: missing field(s) {missing}")


def _check_fields(
    item: object, required: Iterable[str], optional: Iterable[str], where: str
) -> dict[str, str]:
    if not isinstance(item, dict):
        raise SuppressionError(f"{where}: expected an object")
    _check_keys(item, set(required), set(optional), where)
    if not all(isinstance(v, str) for v in item.values()):
        raise SuppressionError(f"{where}: every field must be a string")
    return item


@dataclass
class Baseline:
    """Known findings, by path relative to the baseline file's directory."""

    file: Path
    keyed: bool
    entries: dict[str, set[Entry]] = field(default_factory=dict)

    @property
    def base(self) -> Path:
        """The directory that entry paths are relative to."""
        return self.file.resolve().parent

    @classmethod
    def load(cls, file: Path, key: bytes | None, *, missing_ok: bool = False) -> Baseline:
        """Read and validate a baseline file.

        Raises:
            SuppressionError: The file is missing (unless ``missing_ok``),
                malformed, or keyed while no key is set.
        """
        if missing_ok and not file.exists():
            return cls(file, keyed=key is not None)
        data = _read_json(file, "baseline")
        _check_keys(data, {"version", "keyed", "findings"}, set(), f"baseline {file}")
        keyed, findings = data["keyed"], data["findings"]
        if not isinstance(keyed, bool) or not isinstance(findings, list):
            raise SuppressionError(f'baseline {file}: "keyed" must be a boolean, "findings" a list')
        if keyed and key is None:
            raise SuppressionError(
                f"baseline {file} holds value tags: set {KEY_ENV} to the key it was written with"
            )
        fields = (*_ENTRY_FIELDS, "tag") if keyed else _ENTRY_FIELDS
        baseline = cls(file, keyed)
        for i, item in enumerate(findings):
            entry = _check_fields(item, fields, (), f"baseline {file}, finding {i}")
            baseline.entries.setdefault(entry["path"], set()).add(
                tuple(entry[f] for f in fields[1:])
            )
        return baseline

    def entry(self, path: str, rel: Relationship, key: bytes | None) -> Entry:
        """The entry a finding at ``path`` (relative to ``base``) would have."""
        fields = (
            rel.sheet,
            rel.coordinate,
            rel.header_coordinate,
            label(rel),
            detector(rel.reason),
        )
        if self.keyed:
            if key is None:  # load() refuses a keyed baseline without a key
                raise SuppressionError(f"{KEY_ENV} is not set")
            return (*fields, _tag(key, path, rel))
        return fields

    def write(self) -> int:
        """Write the baseline: sorted, ASCII only, replaced atomically. Returns the entry count."""
        names = (*_ENTRY_FIELDS, "tag") if self.keyed else _ENTRY_FIELDS
        findings = [
            dict(zip(names, (path, *entry), strict=True))
            for path in sorted(self.entries)
            for entry in sorted(self.entries[path])
        ]
        text = json.dumps(
            {"version": FORMAT_VERSION, "keyed": self.keyed, "findings": findings}, indent=2
        )
        fd, tmp = tempfile.mkstemp(dir=self.file.resolve().parent, prefix=".doormat-baseline-")
        try:
            with os.fdopen(fd, "w", encoding="utf-8", newline="\n") as out:
                out.write(text + "\n")
            os.replace(tmp, self.file)
        except BaseException:
            Path(tmp).unlink(missing_ok=True)
            raise
        return len(findings)


@dataclass(frozen=True)
class AllowRule:
    """One allowlist entry: a path pattern, optionally narrowed to a sheet and a cell."""

    path: str
    sheet: str | None = None
    cell: str | None = None

    def allows(self, path: str, rel: Relationship) -> bool:
        """True if the finding at ``path`` (relative to the allowlist's directory) matches."""
        return (
            fnmatch.fnmatchcase(path, self.path)
            and (self.sheet is None or self.sheet == rel.sheet)
            and (self.cell is None or self.cell == rel.coordinate)
        )


@dataclass(frozen=True)
class Allowlist:
    """Findings to ignore, written by hand."""

    file: Path
    rules: tuple[AllowRule, ...]

    @property
    def base(self) -> Path:
        """The directory that path patterns are relative to."""
        return self.file.resolve().parent

    @classmethod
    def load(cls, file: Path) -> Allowlist:
        """Read and validate an allowlist file.

        Raises:
            SuppressionError: The file is missing or malformed.
        """
        data = _read_json(file, "allowlist")
        _check_keys(data, {"version", "allow"}, set(), f"allowlist {file}")
        if not isinstance(data["allow"], list):
            raise SuppressionError(f'allowlist {file}: "allow" must be a list')
        rules = []
        for i, item in enumerate(data["allow"]):
            rule = _check_fields(
                item, ("path",), ("sheet", "cell", "note"), f"allowlist {file}, rule {i}"
            )
            cell = rule.get("cell")
            rules.append(AllowRule(rule["path"], rule.get("sheet"), cell.upper() if cell else None))
        return cls(file, tuple(rules))

    def allows(self, path: str, rel: Relationship) -> bool:
        """True if any rule matches the finding at ``path`` (relative to ``base``)."""
        return any(rule.allows(path, rel) for rule in self.rules)
