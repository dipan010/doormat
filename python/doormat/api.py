"""User-facing API for doormat: load(), GridDoc, and Relationship."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path
from typing import Callable

from doormat import _core
from doormat.extract import extract_xlsx

# Format registry: extension -> (magic_bytes or None, extractor function)
# Lazy imports for optional deps are handled inside the extractor functions.
_FORMAT_REGISTRY: dict[str, tuple[bytes | None, Callable[..., list[list[tuple]]]]] = {}


def _build_format_registry() -> dict[str, tuple[bytes | None, Callable[..., list[list[tuple]]]]]:
    """Build the format dispatch table.

    Extractor functions for optional dependencies (xlrd, odfpy) use lazy
    imports so that ImportError is raised only when the format is used.
    """
    from doormat.extract.csv_tsv import extract_csv
    from doormat.extract.ods import extract_ods
    from doormat.extract.xls import extract_xls

    return {
        ".xlsx": (b"PK\x03\x04", extract_xlsx),
        ".xlsm": (b"PK\x03\x04", extract_xlsx),
        ".xls": (b"\xd0\xcf\x11\xe0", extract_xls),
        ".csv": (None, extract_csv),
        ".tsv": (None, extract_csv),
        ".ods": (b"PK\x03\x04", extract_ods),
    }


def _get_format_registry() -> dict[str, tuple[bytes | None, Callable[..., list[list[tuple]]]]]:
    """Return the format registry, building it on first access."""
    global _FORMAT_REGISTRY  # noqa: PLW0603
    if not _FORMAT_REGISTRY:
        _FORMAT_REGISTRY = _build_format_registry()
    return _FORMAT_REGISTRY


@dataclass(frozen=True)
class Relationship:
    """A detected key-value relationship between two cells.

    Attributes:
        key: The header or label text (e.g., "Password").
        value: The detected credential value.
        confidence: Heuristic confidence score. Values above 120 typically
            indicate real credentials; above 200 indicates high-confidence
            inline or formula-embedded detections.
        reason: Semicolon-separated breakdown of scoring factors
            (e.g., "distance=100;upper;lower;digit;special;length>=8").
        header_cell_id: Internal cell index of the header/label cell.
        value_cell_id: Internal cell index of the value cell.
    """

    key: str
    value: str
    confidence: float
    reason: str
    header_cell_id: int
    value_cell_id: int

    def __repr__(self) -> str:
        return f"Relationship({self.key}={self.value} ({self.confidence}))"


@dataclass(frozen=True)
class GridDoc:
    """Result of processing a spreadsheet file through the doormat engine.

    Attributes:
        filepath: Path to the source spreadsheet file.
        sheet_count: Number of sheets (or 1 for single-sheet formats like CSV).
        cell_count: Total number of non-empty cells across all sheets.
    """

    filepath: Path
    sheet_count: int
    cell_count: int
    _relationships: list[Relationship] = field(repr=False)

    def relationships(self) -> list[Relationship]:
        """Return all inferred key-value relationships.

        Returns:
            A list of all Relationship objects detected in the workbook,
            regardless of confidence score.
        """
        return list(self._relationships)

    def credentials(self, min_confidence: float = 0.0) -> list[Relationship]:
        """Return relationships at or above a minimum confidence threshold.

        Args:
            min_confidence: Minimum confidence score to include.
                Defaults to 0.0 (all relationships). Use 120.0 for
                typical credential filtering.

        Returns:
            A filtered list of Relationship objects with confidence
            >= min_confidence.
        """
        return [r for r in self._relationships if r.confidence >= min_confidence]


def load(filepath: str | Path) -> GridDoc:
    """Load a spreadsheet file and run the credential detection engine.

    Supports .xlsx, .xlsm, .xls, .csv, .tsv, and .ods formats.
    Opens the file, extracts all cells, sends them through the Rust
    detection pipeline, and returns the results as a GridDoc.

    Args:
        filepath: Path to a spreadsheet file. Accepts both str and
            pathlib.Path. Supported extensions: .xlsx, .xlsm, .xls,
            .csv, .tsv, .ods.

    Returns:
        A GridDoc containing all detected relationships and file metadata.

    Raises:
        FileNotFoundError: If the file does not exist.
        ValueError: If the file extension is unsupported or the file
            content does not match the expected format.
        ImportError: If an optional dependency is missing for the
            requested format (.xls requires xlrd, .ods requires odfpy).

    Example::

        import doormat

        doc = doormat.load("workbook.xlsx")
        for cred in doc.credentials(min_confidence=120):
            print(f"{cred.key} = {cred.value}")
    """
    filepath = Path(filepath)

    if not filepath.exists():
        raise FileNotFoundError(f"File not found: {filepath}")

    registry = _get_format_registry()
    ext = filepath.suffix.lower()

    if ext not in registry:
        supported = ", ".join(sorted(registry.keys()))
        raise ValueError(
            f"Unsupported file extension: {ext}. "
            f"Supported formats: {supported}"
        )

    magic_bytes, extractor = registry[ext]

    if magic_bytes is not None:
        with open(filepath, "rb") as f:
            magic = f.read(len(magic_bytes))
        if magic != magic_bytes:
            raise ValueError(
                f"File does not appear to be a valid {ext} file "
                f"(expected {magic_bytes!r} signature, got {magic!r})"
            )

    sheets = extractor(filepath)

    sheet_count = len(sheets)
    cell_count = sum(len(s) for s in sheets)

    raw_results = _core.process_workbook(sheets)

    rels = [
        Relationship(
            key=r["key"],
            value=r["value"],
            confidence=r["confidence"],
            reason=r["reason"],
            header_cell_id=r["header_cell_id"],
            value_cell_id=r["value_cell_id"],
        )
        for r in raw_results
    ]

    return GridDoc(
        filepath=filepath,
        sheet_count=sheet_count,
        cell_count=cell_count,
        _relationships=rels,
    )
