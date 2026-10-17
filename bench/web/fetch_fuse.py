"""Download part of the Fuse spreadsheet corpus into data/spreadsheets/ (see PROTOCOL.md).

Source: Barik et al., MSR 2015, https://zenodo.org/record/581678, CC BY 4.0.
``fuse.zip`` (9.4 GB) holds a 7z archive split into 140 volumes; the
spreadsheets are one ``fuse-cc-binaries.tar.gz`` in the archive's second
LZMA2 block. Only volumes 20-34 are fetched, with HTTP range requests, each
checked against the CRC-32 and size in the zip directory. LZMA2, gzip and tar
are decoded as a stream, every complete tar member is written out, and the
volume data is never stored on disk. The cut-off member at the end is dropped.

The block's offset and LZMA2 dictionary size are constants: they were read
once from the archive index (volume 140) with py7zr, which this script does
not need. A wrong offset fails the LZMA2 or gzip decoding immediately.

Needs a Python built with ``lzma`` (the project's pyenv 3.12.0 is not; any
python3 with lzma works, as the script does not import doormat).

Usage:
    python3 bench/web/fetch_fuse.py [--peek N]
"""

from __future__ import annotations

import argparse
import gzip
import hashlib
import http.client
import io
import json
import lzma
import struct
import sys
import tarfile
import urllib.request
import zipfile
import zlib
from collections.abc import Iterator
from pathlib import Path
from typing import Any

URL = "https://zenodo.org/records/581678/files/fuse.zip?download=1"
ZIP_SIZE = 9_392_830_118
VOLUME_SIZE = 67_108_864
FIRST_VOLUME, LAST_VOLUME = 20, 34
# Byte offset of the binaries block in the joined 7z archive, and its coder:
# LZMA2 alone, properties byte 0x18 (a 16 MiB dictionary).
BLOCK1_OFFSET = 1_279_666_563
LZMA2_DICT_SIZE = 16 * 1024 * 1024

WEB_DIR = Path(__file__).resolve().parent
OUT_DIR = WEB_DIR / "data" / "spreadsheets"
RESULTS_DIR = WEB_DIR / "results"
ENRON_DIR = WEB_DIR.parent / "corpus" / "data" / "spreadsheets"


def sniff(data: bytes) -> str:
    """Return the spreadsheet extension for a file's content, or "" if it is not one.

    Fuse names files by UUID without an extension. An OLE2 compound file is
    taken as ``.xls`` (a non-spreadsheet one fails to load and is reported);
    a zip is ``.xlsm``, ``.xlsx`` or ``.ods`` by the parts it holds.
    """
    if data.startswith(b"\xd0\xcf\x11\xe0\xa1\xb1\x1a\xe1"):
        return ".xls"
    if not data.startswith(b"PK\x03\x04"):
        return ""
    try:
        with zipfile.ZipFile(io.BytesIO(data)) as archive:
            names = set(archive.namelist())
            if "xl/workbook.xml" in names:
                return ".xlsm" if "xl/vbaProject.bin" in names else ".xlsx"
            if "mimetype" in names and archive.read("mimetype").startswith(b"application/vnd.oasis.opendocument.spreadsheet"):
                return ".ods"
    except zipfile.BadZipFile:
        return ""
    return ""


def fetch(start: int, end: int, attempts: int = 5) -> bytes:
    """Return bytes ``start..end`` (inclusive) of fuse.zip, resuming a dropped transfer."""
    data = bytearray()
    for _ in range(attempts):
        request = urllib.request.Request(URL, headers={"Range": f"bytes={start + len(data)}-{end}"})
        try:
            with urllib.request.urlopen(request, timeout=600) as response:  # nosec B310  # nosemgrep: dynamic-urllib-use-detected
                if response.status != 206:
                    raise SystemExit(f"range request not honoured (HTTP {response.status})")
                while chunk := response.read(1 << 20):
                    data += chunk
        except (OSError, http.client.HTTPException):
            continue  # resume from the bytes already received
        if len(data) == end - start + 1:
            return bytes(data)
    raise SystemExit(f"could not fetch bytes {start}-{end} after {attempts} attempts")


def zip_directory() -> dict[str, tuple[int, int, int, int]]:
    """Map each fuse.zip member to (local header offset, compressed size, size, CRC-32)."""
    tail = fetch(ZIP_SIZE - 65536, ZIP_SIZE - 1)
    base = ZIP_SIZE - len(tail)
    eocd64 = tail.rfind(b"PK\x06\x06")
    cd_offset = struct.unpack("<Q", tail[eocd64 + 48 : eocd64 + 56])[0]
    pos = cd_offset - base
    members = {}
    while tail[pos : pos + 4] == b"PK\x01\x02":
        crc, csize, usize = struct.unpack("<III", tail[pos + 16 : pos + 28])
        name_len, extra_len, comment_len = struct.unpack("<HHH", tail[pos + 28 : pos + 34])
        offset = struct.unpack("<I", tail[pos + 42 : pos + 46])[0]
        name = tail[pos + 46 : pos + 46 + name_len].decode()
        extra = tail[pos + 46 + name_len : pos + 46 + name_len + extra_len]
        q = 0
        while q < len(extra):  # zip64 extended information
            header_id, size = struct.unpack("<HH", extra[q : q + 4])
            if header_id == 1:
                values = list(struct.unpack(f"<{size // 8}Q", extra[q + 4 : q + 4 + size // 8 * 8]))
                usize = values.pop(0) if usize == 0xFFFFFFFF else usize
                csize = values.pop(0) if csize == 0xFFFFFFFF else csize
                offset = values.pop(0) if offset == 0xFFFFFFFF else offset
            q += 4 + size
        members[name] = (offset, csize, usize, crc)
        pos += 46 + name_len + extra_len + comment_len
    return members


def volume(members: dict[str, tuple[int, int, int, int]], number: int) -> bytes:
    """Download one 7z volume, inflate it and check it against the zip directory."""
    offset, csize, usize, crc = members[f"FUSE.7z.{number:03d}"]
    local = fetch(offset, offset + 29)
    name_len, extra_len = struct.unpack("<HH", local[26:30])
    start = offset + 30 + name_len + extra_len
    data = zlib.decompress(fetch(start, start + csize - 1), -15)
    if len(data) != usize or zlib.crc32(data) != crc:
        raise SystemExit(f"volume {number}: size or CRC-32 does not match the zip directory")
    return data


def block_stream(members: dict[str, tuple[int, int, int, int]], last: int) -> Iterator[bytes]:
    """Yield the binaries block's tar.gz bytes, decoded from LZMA2, volume by volume."""
    decoder = lzma.LZMADecompressor(
        format=lzma.FORMAT_RAW, filters=[{"id": lzma.FILTER_LZMA2, "dict_size": LZMA2_DICT_SIZE}]
    )
    skip = BLOCK1_OFFSET - (FIRST_VOLUME - 1) * VOLUME_SIZE
    for number in range(FIRST_VOLUME, last + 1):
        data = volume(members, number)[skip:]
        skip = 0
        print(f"volume {number} verified", file=sys.stderr, flush=True)
        for i in range(0, len(data), 1 << 20):
            yield decoder.decompress(data[i : i + (1 << 20)])


class _IterRaw(io.RawIOBase):
    """A raw read-only stream over an iterator of byte chunks."""

    def __init__(self, chunks: Iterator[bytes]) -> None:
        self._chunks = chunks
        self._chunk = b""
        self._pos = 0

    def readable(self) -> bool:
        """Always readable."""
        return True

    def readinto(self, buffer: Any) -> int:
        """Copy the next bytes into ``buffer``; 0 once the iterator is exhausted."""
        while self._pos >= len(self._chunk):
            chunk = next(self._chunks, None)
            if chunk is None:
                return 0
            self._chunk, self._pos = chunk, 0
        n = min(len(buffer), len(self._chunk) - self._pos)
        buffer[:n] = self._chunk[self._pos : self._pos + n]
        self._pos += n
        return n


def enron_md5s() -> set[str]:
    """MD5 of every Enron corpus file, to exclude byte-identical copies."""
    if not ENRON_DIR.is_dir():
        raise SystemExit(f"Enron corpus not found at {ENRON_DIR}; run bench/corpus/fetch_enron.py")
    # MD5 is a file fingerprint, not a security control.
    return {hashlib.md5(p.read_bytes(), usedforsecurity=False).hexdigest() for p in ENRON_DIR.iterdir() if p.is_file()}  # nosemgrep: insecure-hash-algorithm-md5


def peek(count: int) -> None:
    """Print the first tar members' names, sizes and leading bytes, from volume 20 only."""
    raw = io.BufferedReader(_IterRaw(block_stream(zip_directory(), FIRST_VOLUME)), buffer_size=1 << 20)
    with tarfile.open(fileobj=gzip.GzipFile(fileobj=raw), mode="r|") as archive:
        for i, member in enumerate(archive):
            handle = archive.extractfile(member) if member.isfile() else None
            head = handle.read(8).hex() if handle else ""
            print(f"{member.name!r} {member.size} {head}")
            if i + 1 >= count:
                break


def main() -> None:
    """Fetch volumes 20-34, extract the complete spreadsheets and report counts."""
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--peek", type=int, default=0, help="list the first N tar members and exit")
    opts = parser.parse_args()
    if not URL.startswith("https://"):
        raise SystemExit(f"refusing non-https download URL: {URL}")
    if opts.peek:
        peek(opts.peek)
        return
    last = LAST_VOLUME
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    RESULTS_DIR.mkdir(exist_ok=True)
    enron = enron_md5s()
    counts = {"members": 0, "written": 0, "enron_duplicate": 0, "other_format": 0, "name_clash": 0}
    extensions: dict[str, int] = {}
    raw = io.BufferedReader(_IterRaw(block_stream(zip_directory(), last)), buffer_size=1 << 20)
    stream = gzip.GzipFile(fileobj=raw)
    try:
        with tarfile.open(fileobj=stream, mode="r|") as archive:
            for member in archive:
                if not member.isfile():
                    continue
                handle = archive.extractfile(member)
                if handle is None:
                    continue
                data = handle.read()
                if len(data) != member.size:
                    break  # cut off at the end of the last volume
                counts["members"] += 1
                ext = sniff(data)
                extensions[ext or "other"] = extensions.get(ext or "other", 0) + 1
                # Only the base name is used, so no member can write outside OUT_DIR.
                name = Path(member.name).name + ext
                if not ext:
                    counts["other_format"] += 1
                elif hashlib.md5(data, usedforsecurity=False).hexdigest() in enron:  # nosemgrep: insecure-hash-algorithm-md5
                    counts["enron_duplicate"] += 1
                elif (OUT_DIR / name).exists():
                    counts["name_clash"] += 1
                else:
                    (OUT_DIR / name).write_bytes(data)
                    counts["written"] += 1
    except (EOFError, tarfile.ReadError, zlib.error):
        pass  # the stream ends mid-member: everything before it is complete
    summary = {"counts": counts, "extensions": dict(sorted(extensions.items(), key=lambda kv: -kv[1]))}
    (RESULTS_DIR / "fetch_summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
