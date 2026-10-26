"""Tests for doormat.cli: the ``doormat scan`` command."""

from __future__ import annotations

import json
import os
import subprocess
import sys
from dataclasses import replace
from pathlib import Path

import openpyxl
import pytest
from doormat import cli
from doormat.api import SheetError
from doormat.cli import EXIT_CLEAN, EXIT_FINDINGS, EXIT_INCOMPLETE, MASK, main

FIXTURES = Path(__file__).resolve().parents[2] / "bench" / "fixtures"
GROUND_TRUTH = json.loads((FIXTURES / "ground_truth.json").read_text(encoding="utf-8"))
ALL_VALUES = [c["value"] for creds in GROUND_TRUTH.values() for c in creds]

# A cell with a long prefix before its value: the label carries other text.
LONG_PREFIX = "host db01 user admin note see-the-wiki-page-x " + "tail-marker-q"
TWO_IN_ONE = f"{LONG_PREFIX} password: not-real-value-1"


def _xlsx(path: Path, cells: dict[str, str], title: str = "Sheet") -> Path:
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = title
    for ref, value in cells.items():
        ws[ref] = value
    wb.save(path)
    wb.close()
    return path


@pytest.fixture
def cred_dir(tmp_path: Path) -> Path:
    """A directory with one credential file, one clean file and one ignored file."""
    _xlsx(tmp_path / "creds.xlsx", {"A1": "Password", "B1": "not-real-pw-9"})
    (tmp_path / "sub").mkdir()
    _xlsx(tmp_path / "sub" / "clean.xlsx", {"A1": "Region", "B1": "North"})
    (tmp_path / "notes.txt").write_text("Password: not-real-pw-9", encoding="utf-8")
    return tmp_path


def _run(capsys: pytest.CaptureFixture[str], *argv: str) -> tuple[int, str, str]:
    code = main(list(argv))
    out, err = capsys.readouterr()
    return code, out, err


# ---------- exit status and discovery ----------


def test_findings_exit_1_and_masked_table(capsys, cred_dir):
    code, out, err = _run(capsys, "scan", "-j1", str(cred_dir))
    assert code == EXIT_FINDINGS
    assert "creds.xlsx: Sheet!B1: Password = ********" in out
    assert "not-real-pw-9" not in out + err
    assert "1 finding(s) in 1 of 2 file(s)" in err


def test_clean_exit_0(capsys, tmp_path):
    _xlsx(tmp_path / "clean.xlsx", {"A1": "Region", "B1": "North"})
    code, out, _ = _run(capsys, "scan", str(tmp_path / "clean.xlsx"))
    assert code == EXIT_CLEAN
    assert out == ""


def test_min_confidence_filters(capsys, cred_dir):
    code, out, _ = _run(capsys, "scan", "-j1", "--min-confidence", "1000", str(cred_dir))
    assert code == EXIT_CLEAN
    assert out == ""


@pytest.mark.parametrize(
    ("make", "kind"),
    [
        (lambda p: p / "missing.xlsx", "NotFound"),
        (lambda p: (p / "a.txt").write_text("x") and p / "a.txt", "Unsupported"),
        (lambda p: (p / "empty").mkdir() or p / "empty", "Empty"),
    ],
)
def test_bad_paths_exit_2(capsys, tmp_path, make, kind):
    target = make(tmp_path)
    code, _, err = _run(capsys, "scan", str(target))
    assert code == EXIT_INCOMPLETE
    assert f"({kind})" in err


def test_unreadable_file_exit_2_and_other_files_scanned(capsys, cred_dir):
    (cred_dir / "broken.xlsx").write_bytes(b"PK\x03\x04 not really a zip")
    code, out, err = _run(capsys, "scan", "-j1", str(cred_dir))
    assert code == EXIT_INCOMPLETE
    assert "creds.xlsx: Sheet!B1" in out
    assert "broken.xlsx: not read (ValueError)" in err
    assert "--reveal shows error messages" in err


def test_sheet_error_exit_2(capsys, tmp_path, monkeypatch):
    path = _xlsx(tmp_path / "a.xlsx", {"A1": "Password", "B1": "not-real-pw-9"})

    real_load = cli.load

    def fake_load(p, **kw):
        doc = real_load(p, **kw)
        return replace(doc, errors=(SheetError("Hidden", "TypeError", "quoted <x>"),))

    monkeypatch.setattr(cli, "load", fake_load)
    code, out, err = _run(capsys, "scan", "-f", "json", str(path))
    assert code == EXIT_INCOMPLETE
    records = [json.loads(line) for line in out.splitlines()]
    assert {r["type"] for r in records} == {"finding", "error"}
    error = next(r for r in records if r["type"] == "error")
    assert error == {"type": "error", "path": str(path), "sheet": "Hidden", "kind": "TypeError"}
    assert "quoted" not in out + err
    code, out, err = _run(capsys, "scan", "-f", "json", "--reveal", str(path))
    assert "quoted <x>" in out and "quoted <x>" in err


def test_include_exclude(capsys, cred_dir):
    code, _, err = _run(capsys, "scan", "-j1", "--exclude", "creds.*", str(cred_dir))
    assert code == EXIT_CLEAN
    assert "0 finding(s) in 0 of 1 file(s)" in err
    code, _, err = _run(capsys, "scan", "-j1", "--include", "sub/*", str(cred_dir))
    assert code == EXIT_CLEAN
    assert "of 1 file(s)" in err
    _, _, err = _run(capsys, "scan", "--exclude", "*.xlsx", str(cred_dir / "creds.xlsx"))
    assert "of 0 file(s)" in err


def test_usage_error_exit_2(capsys):
    with pytest.raises(SystemExit) as exc:
        main(["scan"])
    assert exc.value.code == EXIT_INCOMPLETE
    for bad in (["scan", "-j", "0", "x"], ["scan", "--max-cells", "-1", "x"]):
        with pytest.raises(SystemExit):
            main(bad)


def test_limits_passed_through(capsys, cred_dir):
    code, _, err = _run(capsys, "scan", "-j1", "--max-cells", "1", str(cred_dir / "creds.xlsx"))
    assert code == EXIT_INCOMPLETE
    assert "(CellLimitError)" in err
    code, _, _ = _run(capsys, "scan", "--max-cells", "0", str(cred_dir / "creds.xlsx"))
    assert code == EXIT_FINDINGS


# ---------- masking ----------


@pytest.mark.parametrize("fmt", ["table", "json", "sarif"])
def test_no_value_in_masked_output(capsys, fmt):
    code, out, err = _run(capsys, "scan", "-j1", "-f", fmt, str(FIXTURES))
    assert code == EXIT_FINDINGS
    leaked = [v for v in ALL_VALUES if v in out or v in err]
    assert leaked == []


def test_sarif_never_carries_values_even_with_reveal(capsys):
    _, out, _ = _run(capsys, "scan", "-j1", "-f", "sarif", "--reveal", str(FIXTURES))
    assert [v for v in ALL_VALUES if v in out] == []


def test_reveal_shows_values(capsys):
    _, out, _ = _run(capsys, "scan", "-j1", "--reveal", str(FIXTURES / "01_adjacent_right.xlsx"))
    assert GROUND_TRUTH["01_adjacent_right.xlsx"][0]["value"] in out
    assert "distance=" in out


@pytest.mark.parametrize("fmt", ["table", "json", "sarif"])
def test_long_label_cut_when_masked(capsys, tmp_path, fmt):
    path = _xlsx(tmp_path / "a.xlsx", {"A1": TWO_IN_ONE})
    _, out, _ = _run(capsys, "scan", "-f", fmt, str(path))
    assert "host db01" in out
    assert "tail-marker-q" not in out
    assert "not-real-value-1" not in out
    _, out, _ = _run(capsys, "scan", "-f", fmt, "--reveal", str(path))
    assert ("tail-marker-q" in out) == (fmt != "sarif")


def test_mask_is_fixed_width(capsys):
    _, out, _ = _run(capsys, "scan", "-j1", "-f", "json", str(FIXTURES))
    values = {json.loads(line)["value"] for line in out.splitlines()}
    assert values == {MASK}


# ---------- untrusted text ----------


def test_control_and_bidi_characters_escaped(capsys, tmp_path):
    path = _xlsx(
        tmp_path / "a.xlsx",
        {"A1": "note\u202e\x9b password: not-real-pw-9"},
        title="S\u202eheet",
    )
    _, out, _ = _run(capsys, "scan", "--min-confidence", "0", str(path))
    assert "\u202e" not in out
    assert out.count("\\u202e") == 2
    assert "\\x9b" in out
    assert cli._safe("a\x1b[31mb\x7f") == "a\\x1b[31mb\\x7f"


def test_non_ascii_sheet_name_on_ascii_console(tmp_path):
    path = _xlsx(
        tmp_path / "a.xlsx",
        {"A1": "Password", "B1": "not-real-pw-9"},
        title="\u041f\u0430\u0440\u043e\u043b\u0438",
    )
    proc = subprocess.run(
        [sys.executable, "-m", "doormat", "scan", str(path)],
        capture_output=True,
        env={**os.environ, "PYTHONIOENCODING": "ascii"},
        check=False,
    )
    assert proc.returncode == EXIT_FINDINGS
    assert b"\\u041f" in proc.stdout


def test_module_entry_point_with_workers():
    """``python -m doormat`` must not rerun the scan in each spawned worker."""
    files = [str(FIXTURES / n) for n in sorted(GROUND_TRUTH)[:3]]
    proc = subprocess.run(
        [sys.executable, "-m", "doormat", "scan", "-j2", "-f", "json", *files],
        capture_output=True,
        timeout=120,
        check=False,
    )
    assert proc.returncode == EXIT_FINDINGS
    assert len(proc.stdout.splitlines()) == 3
    assert proc.stderr.count(b"finding(s)") == 1


# ---------- formats ----------


def test_json_records(capsys):
    _, out, _ = _run(capsys, "scan", "-f", "json", str(FIXTURES / "09_hidden_sheet.xlsx"))
    (record,) = [json.loads(line) for line in out.splitlines()]
    assert record["type"] == "finding"
    assert record["sheet"] == "Secrets"
    assert record["hidden"] is True
    assert record["cell"] == "B1"
    assert record["header_cell"] == "A1"
    assert "reason" not in record


def test_sarif_structure(capsys, monkeypatch):
    monkeypatch.chdir(FIXTURES.parent)
    code, out, _ = _run(capsys, "scan", "-j1", "-f", "sarif", "fixtures", "fixtures/nope.xlsx")
    assert code == EXIT_INCOMPLETE
    log = json.loads(out)
    assert log["version"] == "2.1.0"
    run = log["runs"][0]
    assert run["tool"]["driver"]["rules"][0]["id"] == "doormat/credential"
    assert not run["invocations"][0]["executionSuccessful"]
    first = run["results"][0]
    location = first["locations"][0]
    assert location["physicalLocation"]["artifactLocation"]["uri"] == (
        "fixtures/01_adjacent_right.xlsx"
    )
    assert location["logicalLocations"][0]["fullyQualifiedName"] == "Sheet!B1"
    fingerprints = {r["partialFingerprints"]["doormatLocation/v1"] for r in run["results"]}
    assert len(fingerprints) == len(run["results"])


def test_sarif_uri_outside_cwd(capsys, tmp_path, monkeypatch):
    path = _xlsx(tmp_path / "a.xlsx", {"A1": "Password", "B1": "not-real-pw-9"})
    monkeypatch.chdir(FIXTURES)
    _, out, _ = _run(capsys, "scan", "-f", "sarif", str(path))
    uri = json.loads(out)["runs"][0]["results"][0]["locations"][0]["physicalLocation"]
    assert uri["artifactLocation"]["uri"].startswith("file:")


def test_output_file(capsys, tmp_path):
    target = tmp_path / "report.json"
    code, out, _ = _run(capsys, "scan", "-f", "json", "-o", str(target), str(FIXTURES))
    assert code == EXIT_FINDINGS
    assert out == ""
    assert len(target.read_text(encoding="utf-8").splitlines()) == 19


# ---------- parallelism ----------


def test_jobs_output_identical(capsys):
    _, one, _ = _run(capsys, "scan", "-j1", "-f", "json", str(FIXTURES))
    _, many, _ = _run(capsys, "scan", "-j3", "-f", "json", str(FIXTURES))
    assert one == many


def test_pool_never_forks(monkeypatch):
    """Workers forked after the core's thread pool started can deadlock (Linux, Python < 3.14)."""
    contexts = []
    real = cli.ProcessPoolExecutor

    def spy(**kw):
        contexts.append(kw["mp_context"].get_start_method())
        return real(**kw)

    monkeypatch.setattr(cli, "ProcessPoolExecutor", spy)
    cli.load(FIXTURES / "08_multi_sheet.xlsx")
    jobs = [cli._Job(str(FIXTURES / n), None, None) for n in sorted(GROUND_TRUTH)[:4]]
    assert len(list(cli.run_scan(jobs, workers=2))) == 4
    assert contexts == ["spawn"]


def test_broken_pool_reports_remaining_files(monkeypatch):
    class Broken:
        def __init__(self, **_kw):
            pass

        def __enter__(self):
            return self

        def __exit__(self, *_exc):
            return False

        def map(self, fn, jobs):
            yield fn(jobs[0])
            raise cli.BrokenProcessPool

    monkeypatch.setattr(cli, "ProcessPoolExecutor", Broken)
    jobs = [cli._Job(str(FIXTURES / n), None, None) for n in sorted(GROUND_TRUTH)[:3]]
    results = list(cli.run_scan(jobs, workers=2))
    assert [r.path for r in results] == [j.path for j in jobs]
    assert results[0].findings
    assert [e.kind for r in results[1:] for e in r.errors] == ["BrokenProcessPool"] * 2


def test_version(capsys):
    with pytest.raises(SystemExit) as exc:
        main(["--version"])
    assert exc.value.code == 0
    assert capsys.readouterr().out.startswith("doormat ")
