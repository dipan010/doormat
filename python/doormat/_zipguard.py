"""Refuse zip-based spreadsheets that would decompress to an unreasonable size.

.xlsx, .xlsm and .ods files are zip archives. A "zip bomb" is a small archive
whose parts decompress to gigabytes; parsing one exhausts memory. This check
reads only the archive's directory, before any part is decompressed.

The limits were calibrated on 15,871 real spreadsheets from the Enron corpus:
the largest total was 353 MB, the most compressed part of 10 MiB or more was
157:1, and the most parts in one file was 370.

Limitation: sizes in a zip directory are declared by the file's author and can
be forged. This stops ordinary zip bombs, not a deliberately falsified archive.
"""

from __future__ import annotations

import zipfile
from pathlib import Path

ZIP_MAGIC = b"PK\x03\x04"
DEFAULT_MAX_UNCOMPRESSED_BYTES = 512 * 1024 * 1024
MAX_COMPRESSION_RATIO = 500
RATIO_CHECK_MIN_BYTES = 10 * 1024 * 1024
MAX_MEMBERS = 10_000


def check_zip(path: Path, max_uncompressed_bytes: int | None) -> None:
    """Raise ValueError if the archive at ``path`` looks like a zip bomb.

    Args:
        path: A zip-based spreadsheet (.xlsx, .xlsm or .ods).
        max_uncompressed_bytes: Largest allowed total decompressed size, or
            None for no total limit. The ratio and part-count checks always run.

    Raises:
        ValueError: If the archive is unreadable, has too many parts, would
            decompress beyond the limit, or contains a part whose compression
            ratio is only plausible for a zip bomb.
    """
    try:
        with zipfile.ZipFile(path) as archive:
            members = archive.infolist()
    except zipfile.BadZipFile as exc:
        raise ValueError(f"Not a readable zip archive: {path.name}") from exc

    if len(members) > MAX_MEMBERS:
        raise ValueError(
            f"Refusing {path.name}: {len(members)} archive parts (limit {MAX_MEMBERS})"
        )

    total = sum(m.file_size for m in members)
    if max_uncompressed_bytes is not None and total > max_uncompressed_bytes:
        raise ValueError(
            f"Refusing {path.name}: decompresses to {total:,} bytes "
            f"(limit {max_uncompressed_bytes:,}; pass max_uncompressed_bytes to raise it)"
        )

    for member in members:
        if member.file_size < RATIO_CHECK_MIN_BYTES:
            continue
        ratio = member.file_size / max(member.compress_size, 1)
        if ratio > MAX_COMPRESSION_RATIO:
            raise ValueError(
                f"Refusing {path.name}: part {member.filename} compresses "
                f"{ratio:.0f}:1, which only zip bombs reach (limit {MAX_COMPRESSION_RATIO}:1)"
            )
