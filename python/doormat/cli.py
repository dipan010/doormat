"""Command-line scanner: ``doormat scan PATH...``.

A thin layer over :func:`doormat.load`: it finds spreadsheet files, loads each
one in a worker process (one FFI call per workbook), and reports the findings
as a table, JSON lines or SARIF 2.1.0. It adds no detection logic.

Values are masked unless ``--reveal`` is given, and SARIF output never carries
them. Exit status: 0 no findings, 1 findings at or above ``--min-confidence``,
2 a file or sheet was not fully read or the command line was invalid. Findings
are still reported when the status is 2.
"""

from __future__ import annotations

import argparse
import fnmatch
import hashlib
import io
import json
import multiprocessing
import os
import struct
import sys
import warnings
from collections.abc import Iterable, Iterator, Sequence
from concurrent.futures import ProcessPoolExecutor
from concurrent.futures.process import BrokenProcessPool
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, TextIO

from doormat._types import SHEET_ERRORS
from doormat.api import Relationship, _get_format_registry, load

EXIT_CLEAN = 0
EXIT_FINDINGS = 1
EXIT_INCOMPLETE = 2

MASK = "********"
"""Shown in place of every value: fixed width, so it does not give away the length."""

KEY_LIMIT = 40
"""Labels longer than this are cut in masked and SARIF output. For inline,
formula and comment findings the label is all the text before the value, which
can hold a user name or another secret."""

DEFAULT_MIN_CONFIDENCE = 120.0

_SARIF_SCHEMA = "https://json.schemastore.org/sarif-2.1.0.json"
_RULE_ID = "doormat/credential"
_INFO_URI = "https://dipan010.github.io/doormat/"

# Every exception a malformed or unreadable file is known to raise from load().
# Anything else is a bug and stops the scan.
_FILE_ERRORS: tuple[type[Exception], ...] = (
    *SHEET_ERRORS,
    OSError,
    ImportError,
    RuntimeError,  # a panic in the Rust core
    struct.error,  # truncated .xls records
)
try:
    from xlrd.biffh import XLRDError
    from xlrd.compdoc import CompDocError
    from xlrd.formula import FormulaError
except ImportError:  # .xls support is optional
    pass
else:
    _FILE_ERRORS = (*_FILE_ERRORS, XLRDError, CompDocError, FormulaError)

# Messages of these errors come from doormat itself, never from file content.
_SAFE_MESSAGE_ERRORS = ("ImportError", "FileNotFoundError", "NotFound", "Unsupported", "Empty")


@dataclass(frozen=True)
class FileError:
    """A file, or one sheet of it, that was not fully read."""

    sheet: str | None
    kind: str
    message: str


@dataclass(frozen=True)
class FileResult:
    """Everything the scan learned about one file."""

    path: str
    findings: tuple[Relationship, ...] = ()
    errors: tuple[FileError, ...] = ()


@dataclass(frozen=True)
class _Job:
    """Arguments for one worker call (picklable for the process pool)."""

    path: str
    max_cells: int | None
    max_uncompressed_bytes: int | None


@dataclass
class _Report:
    """Results in input order, plus the totals used for the summary and exit status."""

    results: list[FileResult] = field(default_factory=list)

    @property
    def findings(self) -> int:
        """Number of findings across all files."""
        return sum(len(r.findings) for r in self.results)

    @property
    def incomplete(self) -> int:
        """Number of files that were not fully read."""
        return sum(1 for r in self.results if r.errors)


def scan_file(job: _Job) -> FileResult:
    """Load one file and return its findings or the errors that stopped it.

    Runs in a worker process. Every exception a bad file can raise becomes a
    ``FileError``; the scan goes on with the next file.
    """
    with warnings.catch_warnings():
        # openpyxl warns about workbook features it ignores; they do not affect cells.
        warnings.simplefilter("ignore")
        try:
            doc = load(
                job.path,
                max_cells=job.max_cells,
                max_uncompressed_bytes=job.max_uncompressed_bytes,
            )
        except _FILE_ERRORS as exc:
            return FileResult(job.path, errors=(FileError(None, type(exc).__name__, str(exc)),))
    errors = tuple(FileError(e.sheet, e.kind, e.message) for e in doc.errors)
    return FileResult(job.path, tuple(doc.relationships()), errors)


def _matches(rel: str, patterns: Sequence[str]) -> bool:
    """True if ``rel`` (a ``/``-separated path) matches any fnmatch pattern."""
    return any(fnmatch.fnmatchcase(rel, p) for p in patterns)


def discover(
    paths: Sequence[str], include: Sequence[str], exclude: Sequence[str]
) -> tuple[list[Path], list[FileResult]]:
    """Expand the command-line paths into the files to scan.

    Directories are walked recursively (symlinked directories are not
    followed) for files with a supported extension. Patterns are matched with
    ``fnmatch`` against the ``/``-separated path relative to the directory
    given, or against a file path as given; ``*`` also matches ``/``.

    Returns:
        The files to scan, and an error result for each path that does not
        exist, is a file of an unsupported type, or is a directory holding no
        spreadsheets at all.
    """
    extensions = set(_get_format_registry())
    files: list[Path] = []
    problems: list[FileResult] = []

    def keep(rel: str) -> bool:
        return (not include or _matches(rel, include)) and not _matches(rel, exclude)

    for arg in paths:
        root = Path(arg)
        if root.is_dir():
            found = sorted(
                Path(dirpath, name)
                for dirpath, _dirs, names in os.walk(root)
                for name in names
                if Path(name).suffix.lower() in extensions
            )
            if not found:
                problems.append(_problem(arg, "Empty", "no spreadsheet files in this directory"))
            files.extend(p for p in found if keep(p.relative_to(root).as_posix()))
        elif root.is_file():
            if root.suffix.lower() not in extensions:
                problems.append(_problem(arg, "Unsupported", "not a supported spreadsheet type"))
            elif keep(root.as_posix()):
                files.append(root)
        else:
            problems.append(_problem(arg, "NotFound", "no such file or directory"))
    return files, problems


def _problem(path: str, kind: str, message: str) -> FileResult:
    return FileResult(path, errors=(FileError(None, kind, message),))


def run_scan(jobs: list[_Job], workers: int) -> Iterator[FileResult]:
    """Scan files in input order, in ``workers`` processes (inline when 1).

    If a worker process dies, the files without a result are reported as
    errors rather than lost.
    """
    if workers <= 1 or len(jobs) <= 1:
        yield from map(scan_file, jobs)
        return
    done = 0
    try:
        # Never fork: a child forked after the Rust core's thread pool has
        # started can deadlock (the default on Linux before Python 3.14).
        context = multiprocessing.get_context("spawn")
        with ProcessPoolExecutor(max_workers=workers, mp_context=context) as pool:
            for result in pool.map(scan_file, jobs):
                done += 1
                yield result
    except BrokenProcessPool:
        for job in jobs[done:]:
            yield _problem(job.path, "BrokenProcessPool", "a worker process stopped")


# ---------- Output ----------

_UNSAFE = {c: f"\\x{c:02x}" for c in [*range(0x20), *range(0x7F, 0xA0)]}
_UNSAFE.update({c: f"\\u{c:04x}" for c in [*range(0x202A, 0x202F), *range(0x2066, 0x206A)]})


def _safe(text: str) -> str:
    """Escape control and bidirectional characters so file text cannot drive the terminal."""
    return text.translate(_UNSAFE)


def _key(rel: Relationship, reveal: bool) -> str:
    if reveal or len(rel.key) <= KEY_LIMIT:
        return rel.key
    return rel.key[:KEY_LIMIT] + "..."


def _error_message(err: FileError, reveal: bool) -> str | None:
    """The error's message when it is safe to show; parser messages can quote file content."""
    if reveal or err.kind in _SAFE_MESSAGE_ERRORS:
        return err.message
    return None


def write_table(report: _Report, out: TextIO, reveal: bool) -> None:
    """One line per finding: ``path: Sheet!B3: key = value (confidence N)``."""
    for result in report.results:
        for rel in result.findings:
            hidden = " [hidden sheet]" if rel.hidden else ""
            value = rel.value if reveal else MASK
            extra = f", {rel.reason}" if reveal else ""
            out.write(
                f"{_safe(result.path)}: {_safe(rel.sheet)}!{rel.coordinate}{hidden}: "
                f"{_safe(_key(rel, reveal))} = {_safe(value)} "
                f"(confidence {rel.confidence:.0f}{extra})\n"
            )


def _json_record(path: str, rel: Relationship, reveal: bool) -> dict[str, Any]:
    record: dict[str, Any] = {
        "type": "finding",
        "path": path,
        "sheet": rel.sheet,
        "cell": rel.coordinate,
        "row": rel.row,
        "col": rel.col,
        "header_cell": rel.header_coordinate,
        "hidden": rel.hidden,
        "key": _key(rel, reveal),
        "confidence": rel.confidence,
        "value": rel.value if reveal else MASK,
    }
    if reveal:
        record["reason"] = rel.reason
    return record


def write_json(report: _Report, out: TextIO, reveal: bool) -> None:
    """JSON lines: one ``finding`` or ``error`` object per line, ASCII only."""
    for result in report.results:
        out.writelines(
            json.dumps(_json_record(result.path, rel, reveal)) + "\n" for rel in result.findings
        )
        for err in result.errors:
            record = {"type": "error", "path": result.path, "sheet": err.sheet, "kind": err.kind}
            message = _error_message(err, reveal)
            if message is not None:
                record["message"] = message
            out.write(json.dumps(record) + "\n")


def _artifact_uri(path: str) -> str:
    """A path relative to the working directory, ``/``-separated, or a file URI outside it."""
    resolved = Path(path).resolve()
    try:
        return resolved.relative_to(Path.cwd().resolve()).as_posix()
    except ValueError:
        return resolved.as_uri()


def _fingerprint(uri: str, rel: Relationship) -> str:
    """Location-only fingerprint: file, sheet, value cell and label cell, never the value."""
    parts = (uri, rel.sheet, rel.coordinate, rel.header_coordinate)
    return hashlib.sha256("\0".join(parts).encode()).hexdigest()


def sarif_log(report: _Report, version: str) -> dict[str, Any]:
    """Build a SARIF 2.1.0 log. Values are never included, with or without ``--reveal``."""
    results = []
    for file_result in report.results:
        uri = _artifact_uri(file_result.path)
        for rel in file_result.findings:
            label = _key(rel, reveal=False)
            results.append(
                {
                    "ruleId": _RULE_ID,
                    "ruleIndex": 0,
                    "level": "error" if rel.confidence >= 200 else "warning",
                    "message": {
                        "text": f"Possible credential in {rel.sheet}!{rel.coordinate} "
                        f"(label {label!r}, confidence {rel.confidence:.0f})."
                    },
                    "locations": [
                        {
                            "physicalLocation": {
                                "artifactLocation": {"uri": uri},
                                "region": {"startLine": 1},
                            },
                            "logicalLocations": [
                                {
                                    "name": rel.coordinate,
                                    "fullyQualifiedName": f"{rel.sheet}!{rel.coordinate}",
                                    "kind": "element",
                                }
                            ],
                        }
                    ],
                    "partialFingerprints": {"doormatLocation/v1": _fingerprint(uri, rel)},
                    "properties": {
                        "sheet": rel.sheet,
                        "cell": rel.coordinate,
                        "headerCell": rel.header_coordinate,
                        "hiddenSheet": rel.hidden,
                        "confidence": rel.confidence,
                    },
                }
            )
    notifications = [
        {
            "level": "error",
            "message": {
                "text": f"Not fully read ({err.kind})"
                + (f": sheet {err.sheet!r}" if err.sheet is not None else "")
            },
            "locations": [
                {"physicalLocation": {"artifactLocation": {"uri": _artifact_uri(r.path)}}}
            ],
        }
        for r in report.results
        for err in r.errors
    ]
    rule = {
        "id": _RULE_ID,
        "name": "SpreadsheetCredential",
        "shortDescription": {"text": "Credential in a spreadsheet"},
        "fullDescription": {
            "text": "A password, key or other secret stored in a spreadsheet cell, "
            "formula or comment, paired with the label or column header around it."
        },
        "help": {
            "text": "Remove the secret from the spreadsheet and rotate it. "
            "Run doormat scan --reveal locally to see the value."
        },
        "defaultConfiguration": {"level": "error"},
        "properties": {"tags": ["security", "secrets"], "security-severity": "8.0"},
    }
    return {
        "$schema": _SARIF_SCHEMA,
        "version": "2.1.0",
        "runs": [
            {
                "tool": {
                    "driver": {
                        "name": "doormat",
                        "version": version,
                        "informationUri": _INFO_URI,
                        "rules": [rule],
                    }
                },
                "invocations": [
                    {
                        "executionSuccessful": not notifications,
                        "toolExecutionNotifications": notifications,
                    }
                ],
                "results": results,
            }
        ],
    }


def write_summary(report: _Report, err: TextIO, reveal: bool) -> None:
    """Errors and totals for people, on stderr."""
    for result in report.results:
        for e in result.errors:
            where = f" sheet {_safe(repr(e.sheet))}" if e.sheet is not None else ""
            message = _error_message(e, reveal)
            detail = f": {_safe(message)}" if message is not None else ""
            err.write(f"doormat: {_safe(result.path)}:{where} not read ({e.kind}){detail}\n")
    hidden = any(_error_message(e, reveal) is None for r in report.results for e in r.errors)
    files = len(report.results)
    with_findings = sum(1 for r in report.results if r.findings)
    err.write(
        f"doormat: {report.findings} finding(s) in {with_findings} of {files} file(s)"
        + (f"; {report.incomplete} not fully read" if report.incomplete else "")
        + (" (--reveal shows error messages)" if hidden else "")
        + "\n"
    )


# ---------- Command line ----------


def _limit(text: str) -> int | None:
    """Parse a size limit; 0 means no limit."""
    value = int(text)
    if value < 0:
        raise argparse.ArgumentTypeError("must be 0 (no limit) or more")
    return value or None


def _jobs(text: str) -> int:
    value = int(text)
    if value < 1:
        raise argparse.ArgumentTypeError("must be 1 or more")
    return value


def build_parser() -> argparse.ArgumentParser:
    """Return the ``doormat`` argument parser."""
    from doormat import __version__

    parser = argparse.ArgumentParser(
        prog="doormat", description="Find passwords and secrets hidden in spreadsheets."
    )
    parser.add_argument("--version", action="version", version=f"doormat {__version__}")
    commands = parser.add_subparsers(dest="command", required=True, metavar="COMMAND")
    scan = commands.add_parser(
        "scan",
        help="scan files and directories",
        description="Scan spreadsheets for credentials. Exit status: 0 no findings, "
        "1 findings, 2 a file or sheet not fully read (findings are still reported) "
        "or a usage error.",
    )
    scan.add_argument("paths", nargs="+", metavar="PATH", help="file or directory to scan")
    scan.add_argument(
        "-f",
        "--format",
        choices=("table", "json", "sarif"),
        default="table",
        help="output format (default: table)",
    )
    scan.add_argument("-o", "--output", type=Path, help="write the report to this file")
    scan.add_argument(
        "--min-confidence",
        type=float,
        default=DEFAULT_MIN_CONFIDENCE,
        metavar="N",
        help=f"report findings at or above this score (default: {DEFAULT_MIN_CONFIDENCE:.0f})",
    )
    scan.add_argument(
        "--reveal",
        action="store_true",
        help="show values, full labels, scores' reasons and error messages (never in SARIF)",
    )
    scan.add_argument(
        "--include",
        action="append",
        default=[],
        metavar="GLOB",
        help="scan only matching paths (repeatable)",
    )
    scan.add_argument(
        "--exclude",
        action="append",
        default=[],
        metavar="GLOB",
        help="skip matching paths (repeatable)",
    )
    scan.add_argument(
        "-j",
        "--jobs",
        type=_jobs,
        default=os.cpu_count() or 1,
        metavar="N",
        help="worker processes (default: one per CPU)",
    )
    scan.add_argument(
        "--max-cells",
        type=_limit,
        default=2_000_000,
        metavar="N",
        help="refuse a file holding more cells than this; 0 for no limit (default: 2000000)",
    )
    scan.add_argument(
        "--max-uncompressed-bytes",
        type=_limit,
        default=512 * 1024 * 1024,
        metavar="N",
        help="refuse a zip-based file larger than this unpacked; 0 for no limit "
        "(default: 536870912)",
    )
    return parser


def _collect(results: Iterable[FileResult], min_confidence: float) -> _Report:
    report = _Report()
    for r in results:
        kept = tuple(f for f in r.findings if f.confidence >= min_confidence)
        report.results.append(FileResult(r.path, kept, r.errors))
    return report


def main(argv: Sequence[str] | None = None) -> int:
    """Run the ``doormat`` command and return its exit status."""
    from doormat import __version__

    args = build_parser().parse_args(argv)
    files, problems = discover(args.paths, args.include, args.exclude)
    jobs = [_Job(str(p), args.max_cells, args.max_uncompressed_bytes) for p in files]
    report = _collect([*problems, *run_scan(jobs, args.jobs)], args.min_confidence)

    out: TextIO
    if args.output is not None:
        out = args.output.open("w", encoding="utf-8", newline="\n")
    else:
        out = sys.stdout
    for stream in (sys.stdout, sys.stderr):
        if isinstance(stream, io.TextIOWrapper):
            # A non-UTF-8 console (Windows) must not fail on a sheet name.
            stream.reconfigure(errors="backslashreplace")
    try:
        if args.format == "table":
            write_table(report, out, args.reveal)
        elif args.format == "json":
            write_json(report, out, args.reveal)
        else:
            json.dump(sarif_log(report, __version__), out, indent=2)
            out.write("\n")
    finally:
        if out is not sys.stdout:
            out.close()
    write_summary(report, sys.stderr, args.reveal)

    if report.incomplete:
        return EXIT_INCOMPLETE
    return EXIT_FINDINGS if report.findings else EXIT_CLEAN
