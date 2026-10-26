"""Tests for allowlists and baselines: doormat._suppress and their ``doormat scan`` flags."""

from __future__ import annotations

import json
import shutil
from pathlib import Path

import doormat
import openpyxl
import pytest
from doormat import _suppress
from doormat._suppress import DETECTORS, KEY_ENV, Allowlist, Baseline, SuppressionError
from doormat.cli import EXIT_CLEAN, EXIT_FINDINGS, EXIT_INCOMPLETE, main

FIXTURES = Path(__file__).resolve().parents[2] / "bench" / "fixtures"
GROUND_TRUTH = json.loads((FIXTURES / "ground_truth.json").read_text(encoding="utf-8"))
ALL_VALUES = [c["value"] for creds in GROUND_TRUTH.values() for c in creds]
TEST_KEY = "unit-test-key-not-a-secret"


def _xlsx(path: Path, cells: dict[str, str], title: str = "Sheet") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    wb = openpyxl.Workbook()
    ws = wb.active
    ws.title = title
    for ref, value in cells.items():
        ws[ref] = value
    wb.save(path)
    wb.close()
    return path


def _cred(path: Path, value: str = "not-real-pw-9", title: str = "Sheet") -> Path:
    return _xlsx(path, {"A1": "Password", "B1": value}, title)


def _run(capsys: pytest.CaptureFixture[str], *argv: str) -> tuple[int, str, str]:
    code = main(["scan", "-j1", *argv])
    out, err = capsys.readouterr()
    return code, out, err


@pytest.fixture
def repo(tmp_path: Path, monkeypatch: pytest.MonkeyPatch) -> Path:
    """A directory with two credential files, used as the working directory."""
    _cred(tmp_path / "a.xlsx")
    _cred(tmp_path / "sub" / "b.xlsx")
    monkeypatch.chdir(tmp_path)
    monkeypatch.delenv(KEY_ENV, raising=False)
    return tmp_path


def _entries(file: Path) -> list[dict[str, str]]:
    data = json.loads(file.read_text(encoding="utf-8"))
    return data["findings"]


# ---------- detector vocabulary (a file-format contract) ----------


def test_detector_vocabulary_is_frozen(tmp_path):
    vocab = _xlsx(
        tmp_path / "vocab.xlsx",
        {
            "A1": '=IF(B9,"token","not-real-val-3")',
            "D1": "MYSQL_USER",
            "E1": "MYSQL_ROOT_PASSWORD",
            "F1": "MYSQL_HOST",
            **{f"D{r}": f"user{r}" for r in range(2, 5)},
            **{f"E{r}": f"fakeroot{r}x" for r in range(2, 5)},
            **{f"F{r}": "db.local" for r in range(2, 5)},
        },
    )
    files = [*sorted(FIXTURES.glob("*.xlsx")), vocab]
    tokens = {
        r.reason.split(";")[0].split("=")[0] for f in files for r in doormat.load(f).relationships()
    }
    assert tokens == set(DETECTORS)
    assert set(DETECTORS.values()) == {
        "spatial",
        "split",
        "table",
        "variable",
        "inline",
        "formula",
        "comment",
    }
    assert _suppress.detector("brand_new_reason;x") == "brand_new_reason"


# ---------- baselines ----------


def test_write_then_scan_is_clean(capsys, repo):
    code, out, err = _run(capsys, "--write-baseline", "baseline.json", ".")
    assert code == EXIT_CLEAN
    assert "wrote 2 baseline entry(ies)" in err
    entries = _entries(repo / "baseline.json")
    assert [e["path"] for e in entries] == ["a.xlsx", "sub/b.xlsx"]
    assert entries[0] == {
        "path": "a.xlsx",
        "sheet": "Sheet",
        "cell": "B1",
        "header_cell": "A1",
        "label": "Password",
        "detector": "spatial",
    }
    code, out, err = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_CLEAN
    assert out == ""
    assert "2 suppressed (0 allowlist, 2 baseline)" in err


def test_new_finding_reported(capsys, repo):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    _cred(repo / "c.xlsx")
    code, out, _ = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_FINDINGS
    assert out.startswith("c.xlsx: Sheet!B1")


def test_scan_from_subdirectory_matches(capsys, repo, monkeypatch):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    monkeypatch.chdir(repo / "sub")
    code, _, err = _run(capsys, "--baseline", "../baseline.json", "b.xlsx")
    assert code == EXIT_CLEAN
    assert "1 suppressed" in err


def test_partial_write_keeps_other_files(capsys, repo):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    _cred(repo / "c.xlsx")
    code, _, _ = _run(capsys, "--write-baseline", "baseline.json", "c.xlsx")
    assert code == EXIT_CLEAN
    paths = [e["path"] for e in _entries(repo / "baseline.json")]
    assert paths == ["a.xlsx", "c.xlsx", "sub/b.xlsx"]


def test_rewrite_drops_fixed_findings(capsys, repo):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    _xlsx(repo / "a.xlsx", {"A1": "Region", "B1": "North"})
    code, _, err = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_CLEAN
    assert "1 baseline entry(ies) no longer found" in err
    _run(capsys, "--write-baseline", "baseline.json", ".")
    assert [e["path"] for e in _entries(repo / "baseline.json")] == ["sub/b.xlsx"]


def test_stale_counts_only_scanned_files(capsys, repo):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    _, _, err = _run(capsys, "--baseline", "baseline.json", "a.xlsx")
    assert "no longer found" not in err


def test_incomplete_file_keeps_old_entries(capsys, repo, monkeypatch):
    from dataclasses import replace

    from doormat import cli
    from doormat.api import SheetError

    _run(capsys, "--write-baseline", "baseline.json", ".")
    real_load = cli.load

    def unreadable_sheet(p, **kw):
        doc = real_load(p, **kw)
        return replace(doc, _relationships=[], errors=(SheetError("S2", "TypeError", "x"),))

    monkeypatch.setattr(cli, "load", unreadable_sheet)
    code, _, _ = _run(capsys, "--write-baseline", "baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert len(_entries(repo / "baseline.json")) == 2
    code, _, err = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert "no longer found" not in err


def test_outside_directory_not_recorded(capsys, repo, tmp_path_factory):
    elsewhere = _cred(tmp_path_factory.mktemp("elsewhere") / "x.xlsx")
    code, _, err = _run(capsys, "--write-baseline", "baseline.json", str(elsewhere))
    assert code == EXIT_CLEAN
    assert "1 finding(s) outside" in err
    assert _entries(repo / "baseline.json") == []
    code, _, _ = _run(capsys, "--baseline", "baseline.json", str(elsewhere))
    assert code == EXIT_FINDINGS


def test_baseline_file_is_deterministic_and_ascii(capsys, repo):
    _cred(repo / "u.xlsx", title="S\u202eheet")
    _run(capsys, "--write-baseline", "baseline.json", ".")
    first = (repo / "baseline.json").read_bytes()
    _run(capsys, "--write-baseline", "baseline.json", ".")
    assert (repo / "baseline.json").read_bytes() == first
    assert first.isascii()
    assert first.endswith(b"}\n")
    assert b"\\u202e" in first
    assert not list(repo.glob(".doormat-baseline-*"))


def test_baseline_write_failure_exit_2(capsys, repo, monkeypatch):
    code, _, err = _run(capsys, "--write-baseline", "missing-dir/baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert "cannot write" in err

    def fail(*_args):
        raise PermissionError("denied")

    monkeypatch.setattr(_suppress.os, "replace", fail)
    code, _, err = _run(capsys, "--write-baseline", "baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert not list(repo.glob(".doormat-baseline-*"))
    assert not (repo / "baseline.json").exists()


def test_baseline_never_holds_values(capsys, tmp_path, monkeypatch):
    shutil.copytree(FIXTURES, tmp_path / "f", ignore=shutil.ignore_patterns("*.py", "*.json"))
    monkeypatch.chdir(tmp_path)
    monkeypatch.delenv(KEY_ENV, raising=False)
    _run(capsys, "--write-baseline", "plain.json", "f")
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    _run(capsys, "--write-baseline", "keyed.json", "f")
    for name in ("plain.json", "keyed.json"):
        text = (tmp_path / name).read_text(encoding="utf-8")
        assert len(_entries(tmp_path / name)) == 19
        assert [v for v in ALL_VALUES if v in text] == []


@pytest.mark.parametrize(
    ("content", "message"),
    [
        ("not json", "not readable JSON"),
        ('{"version": 2, "keyed": false, "findings": []}', '"version": 1'),
        ('{"version": 1, "keyed": false}', "missing field(s) ['findings']"),
        ('{"version": 1, "keyed": false, "findings": [], "x": 1}', "unknown field(s) ['x']"),
        ('{"version": 1, "keyed": "no", "findings": []}', "must be a boolean"),
        ('{"version": 1, "keyed": false, "findings": [1]}', "expected an object"),
        (
            (
                '{"version": 1, "keyed": false, "findings": [{"path": "a", "sheet": "S", '
                '"cell": "B1", "header_cell": "A1", "label": "P", "detector": 3}]}'
            ),
            "must be a string",
        ),
    ],
)
def test_malformed_baseline_exit_2(capsys, repo, content, message):
    (repo / "bad.json").write_text(content, encoding="utf-8")
    code, out, err = _run(capsys, "--baseline", "bad.json", ".")
    assert code == EXIT_INCOMPLETE
    assert message in err
    assert out == ""


def test_missing_baseline_exit_2(capsys, repo):
    code, _, err = _run(capsys, "--baseline", "nope.json", ".")
    assert code == EXIT_INCOMPLETE
    assert "baseline not found" in err


def test_baseline_flags_exclusive(repo):
    with pytest.raises(SystemExit) as exc:
        main(["scan", "--baseline", "a.json", "--write-baseline", "b.json", "."])
    assert exc.value.code == EXIT_INCOMPLETE


# ---------- keyed baselines ----------


def test_keyed_baseline_resurfaces_changed_value(capsys, repo, monkeypatch):
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    _run(capsys, "--write-baseline", "baseline.json", ".")
    assert all(len(e["tag"]) == 64 for e in _entries(repo / "baseline.json"))
    code, _, _ = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_CLEAN
    _cred(repo / "a.xlsx", value="not-real-pw-10")
    code, out, _ = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_FINDINGS
    assert out.startswith("a.xlsx: Sheet!B1")


def test_keyed_tags_differ_for_shared_values(capsys, repo, monkeypatch):
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    _run(capsys, "--write-baseline", "baseline.json", ".")
    tags = [e["tag"] for e in _entries(repo / "baseline.json")]
    assert len(tags) == 2
    assert len(set(tags)) == 2


def test_rotated_key_resurfaces_everything(capsys, repo, monkeypatch):
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    _run(capsys, "--write-baseline", "baseline.json", ".")
    monkeypatch.setenv(KEY_ENV, TEST_KEY + "-rotated")
    code, out, _ = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_FINDINGS
    assert len(out.splitlines()) == 2


def test_keyed_baseline_without_key_exit_2(capsys, repo, monkeypatch):
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    _run(capsys, "--write-baseline", "baseline.json", ".")
    monkeypatch.delenv(KEY_ENV)
    for flag in ("--baseline", "--write-baseline"):
        code, out, err = _run(capsys, flag, "baseline.json", ".")
        assert code == EXIT_INCOMPLETE
        assert f"set {KEY_ENV}" in err
        assert out == ""


def test_short_key_exit_2(capsys, repo, monkeypatch):
    monkeypatch.setenv(KEY_ENV, "short")
    code, _, err = _run(capsys, "--write-baseline", "baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert "at least 16 bytes" in err


def test_key_with_unkeyed_baseline(capsys, repo, monkeypatch):
    _run(capsys, "--write-baseline", "baseline.json", ".")
    monkeypatch.setenv(KEY_ENV, TEST_KEY)
    code, _, _ = _run(capsys, "--baseline", "baseline.json", ".")
    assert code == EXIT_CLEAN
    code, _, err = _run(capsys, "--write-baseline", "baseline.json", ".")
    assert code == EXIT_INCOMPLETE
    assert "has no value tags" in err


def test_entry_needs_key_when_keyed(tmp_path):
    baseline = Baseline(tmp_path / "b.json", keyed=True)
    rel = doormat.load(FIXTURES / "01_adjacent_right.xlsx").relationships()[0]
    with pytest.raises(SuppressionError):
        baseline.entry("a.xlsx", rel, None)


# ---------- allowlists ----------


def _allow(repo: Path, *rules: dict[str, str]) -> str:
    (repo / "allow.json").write_text(
        json.dumps({"version": 1, "allow": list(rules)}), encoding="utf-8"
    )
    return "allow.json"


@pytest.mark.parametrize(
    ("rule", "suppressed"),
    [
        ({"path": "sub/*"}, 1),
        ({"path": "*.xlsx"}, 2),
        ({"path": "a.xlsx", "sheet": "Sheet", "cell": "b1", "note": "sample data"}, 1),
        ({"path": "a.xlsx", "sheet": "Other"}, 0),
        ({"path": "a.xlsx", "cell": "C9"}, 0),
    ],
)
def test_allowlist_rules(capsys, repo, rule, suppressed):
    _, _, err = _run(capsys, "--allowlist", _allow(repo, rule), ".")
    if suppressed:
        assert f"{suppressed} suppressed ({suppressed} allowlist, 0 baseline)" in err
    else:
        assert "suppressed" not in err


def test_allowlist_relative_to_its_directory(capsys, repo, monkeypatch):
    name = _allow(repo, {"path": "sub/b.xlsx"})
    monkeypatch.chdir(repo / "sub")
    code, _, _ = _run(capsys, "--allowlist", f"../{name}", "b.xlsx")
    assert code == EXIT_CLEAN


def test_allowlist_outside_directory_matches_nothing(capsys, repo, tmp_path_factory):
    elsewhere = _cred(tmp_path_factory.mktemp("elsewhere") / "x.xlsx")
    code, _, _ = _run(capsys, "--allowlist", _allow(repo, {"path": "*"}), str(elsewhere))
    assert code == EXIT_FINDINGS


def test_allowlisted_findings_not_baselined(capsys, repo):
    _run(capsys, "--allowlist", _allow(repo, {"path": "sub/*"}), "--write-baseline", "b.json", ".")
    assert [e["path"] for e in _entries(repo / "b.json")] == ["a.xlsx"]


def test_allowlist_keeps_read_errors(capsys, repo):
    (repo / "broken.xlsx").write_bytes(b"PK\x03\x04 not a zip")
    code, _, _ = _run(capsys, "--allowlist", _allow(repo, {"path": "*"}), ".")
    assert code == EXIT_INCOMPLETE


@pytest.mark.parametrize(
    ("content", "message"),
    [
        ('{"version": 1, "allow": [{"sheet": "S"}]}', "missing field(s) ['path']"),
        ('{"version": 1, "allow": [{"path": "*", "cel": "B1"}]}', "unknown field(s) ['cel']"),
        ('{"version": 1, "allow": {}}', '"allow" must be a list'),
        ('{"version": 1}', "missing field(s) ['allow']"),
    ],
)
def test_malformed_allowlist_exit_2(capsys, repo, content, message):
    (repo / "allow.json").write_text(content, encoding="utf-8")
    code, _, err = _run(capsys, "--allowlist", "allow.json", ".")
    assert code == EXIT_INCOMPLETE
    assert message in err


def test_allowlist_load_api(tmp_path):
    _allow(tmp_path, {"path": "a/*", "cell": "c3"})
    allowlist = Allowlist.load(tmp_path / "allow.json")
    assert allowlist.rules[0].cell == "C3"
    assert allowlist.base == tmp_path.resolve()
