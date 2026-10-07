"""Download, verify and extract the Enron spreadsheet corpus into data/spreadsheets/.

Source: Hermans & Murphy-Hill, ICSE 2015, figshare article 1221767, CC BY 4.0.
Extraction uses the system ``tar`` (bsdtar on macOS reads 7z natively); on
Linux, install p7zip and the script falls back to ``7z x``.

Usage:
    python bench/corpus/fetch_enron.py
"""

from __future__ import annotations

import hashlib
import shutil
import subprocess  # nosec B404
import urllib.request
from pathlib import Path

URL = "https://ndownloader.figshare.com/files/3242531"
SHA256 = "d73bd1ea5be929236541331a58ef8e255857fb8f7b2397ad629a30425bd9e149"
DATA_DIR = Path(__file__).resolve().parent / "data"
ARCHIVE = DATA_DIR / "spreadsheets.7z"
OUT_DIR = DATA_DIR / "spreadsheets"


def sha256_of(path: Path) -> str:
    """Return the hex SHA-256 of a file, read in 1 MiB chunks."""
    digest = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            digest.update(chunk)
    return digest.hexdigest()


def main() -> None:
    """Fetch the archive if missing, verify its hash, and extract it."""
    DATA_DIR.mkdir(parents=True, exist_ok=True)
    if not ARCHIVE.exists():
        if not URL.startswith("https://"):
            raise SystemExit(f"refusing non-https download URL: {URL}")
        print(f"Downloading {URL} (~1 GB)")
        urllib.request.urlretrieve(URL, ARCHIVE)  # nosec B310  # nosemgrep: dynamic-urllib-use-detected
    actual = sha256_of(ARCHIVE)
    if actual != SHA256:
        raise SystemExit(f"sha256 mismatch: expected {SHA256}, got {actual}")

    OUT_DIR.mkdir(exist_ok=True)
    # Resolve the extractor to an absolute path; arguments are fixed, not user input.
    seven_zip = shutil.which("7z")
    tar = shutil.which("tar")
    if seven_zip:
        subprocess.run([seven_zip, "x", "-y", f"-o{OUT_DIR}", str(ARCHIVE)], check=True)  # nosec B603
    elif tar:
        subprocess.run([tar, "-xf", str(ARCHIVE), "-C", str(OUT_DIR)], check=True)  # nosec B603
    else:
        raise SystemExit("neither 7z nor tar found on PATH")
    count = sum(1 for p in OUT_DIR.iterdir() if p.is_file())
    print(f"Extracted {count} files to {OUT_DIR}")


if __name__ == "__main__":
    main()
