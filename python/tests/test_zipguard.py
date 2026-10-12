"""Tests for the zip-bomb pre-check that runs before .xlsx/.xlsm/.ods files are parsed."""

from __future__ import annotations

import shutil
import zipfile
from pathlib import Path

import openpyxl
import pytest

import doormat


@pytest.fixture
def workbook(tmp_path: Path) -> Path:
    """A small, valid workbook with one credential."""
    wb = openpyxl.Workbook()
    wb.active["A1"] = "Password"
    wb.active["B1"] = "s3cret!!"
    path = tmp_path / "book.xlsx"
    wb.save(path)
    wb.close()
    return path


def _with_bomb(src: Path, dst: Path, size: int = 20 * 1024 * 1024) -> Path:
    """Copy ``src`` with its worksheet replaced by ``size`` bytes of repetitive data."""
    with zipfile.ZipFile(src) as zin, zipfile.ZipFile(dst, "w", zipfile.ZIP_DEFLATED) as zout:
        for item in zin.infolist():
            data = zin.read(item.filename)
            if item.filename == "xl/worksheets/sheet1.xml":
                data = b"<worksheet>" + b" " * size + b"</worksheet>"
            zout.writestr(item.filename, data)
    return dst


def test_zip_bomb_is_refused(workbook: Path, tmp_path: Path) -> None:
    bomb = _with_bomb(workbook, tmp_path / "bomb.xlsx")
    assert bomb.stat().st_size < 200_000  # tiny on disk, 20 MiB unpacked
    with pytest.raises(ValueError, match="zip bombs"):
        doormat.load(bomb)


def test_zip_bomb_is_refused_even_without_size_limit(workbook: Path, tmp_path: Path) -> None:
    bomb = _with_bomb(workbook, tmp_path / "bomb.xlsx")
    with pytest.raises(ValueError, match="zip bombs"):
        doormat.load(bomb, max_uncompressed_bytes=None)


def test_zip_bomb_is_refused_for_ods(workbook: Path, tmp_path: Path) -> None:
    bomb = _with_bomb(workbook, tmp_path / "bomb.xlsx")
    ods = shutil.copy(bomb, tmp_path / "bomb.ods")
    with pytest.raises(ValueError, match="zip bombs"):
        doormat.load(ods)


def test_too_many_parts_is_refused(tmp_path: Path) -> None:
    path = tmp_path / "parts.xlsx"
    with zipfile.ZipFile(path, "w") as archive:
        for i in range(10_001):
            archive.writestr(f"part{i}.xml", b"")
    with pytest.raises(ValueError, match="archive parts"):
        doormat.load(path)


def test_size_limit_is_configurable(workbook: Path) -> None:
    with pytest.raises(ValueError, match="max_uncompressed_bytes"):
        doormat.load(workbook, max_uncompressed_bytes=100)
    assert len(doormat.load(workbook, max_uncompressed_bytes=None).credentials(120)) == 1
    assert len(doormat.load(workbook).credentials(120)) == 1


def test_non_zip_formats_are_not_checked(tmp_path: Path) -> None:
    path = tmp_path / "data.csv"
    path.write_text("Password,s3cret!!\n")
    assert len(doormat.load(path, max_uncompressed_bytes=1).credentials(120)) == 1
