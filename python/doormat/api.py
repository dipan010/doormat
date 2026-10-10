"""User-facing API for doormat: load(), GridDoc, and Relationship."""

from __future__ import annotations

from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from doormat import _core
from doormat._types import Sheet
from doormat.extract import extract_xlsx

# Format registry: extension -> (magic_bytes or None, extractor function)
# Lazy imports for optional deps are handled inside the extractor functions.
_FORMAT_REGISTRY: dict[str, tuple[bytes | None, Callable[..., list[Sheet]]]] = {}


def _build_format_registry() -> dict[str, tuple[bytes | None, Callable[..., list[Sheet]]]]:
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


def _get_format_registry() -> dict[str, tuple[bytes | None, Callable[..., list[Sheet]]]]:
    """Return the format registry, building it on first access."""
    global _FORMAT_REGISTRY  # noqa: PLW0603
    if not _FORMAT_REGISTRY:
        _FORMAT_REGISTRY = _build_format_registry()
    return _FORMAT_REGISTRY


_HIDDEN_SUFFIX = "[HIDDEN]"


def _column_letter(col: int) -> str:
    """Convert a 1-based column number to its spreadsheet letters (1 -> A, 28 -> AB)."""
    letters = ""
    while col > 0:
        col, rem = divmod(col - 1, 26)
        letters = chr(ord("A") + rem) + letters
    return letters


@dataclass(frozen=True)
class Relationship:
    """A credential found in a spreadsheet, with where it was found.

    Attributes:
        key: The header or label text (e.g., "Password").
        value: The detected credential value.
        confidence: Heuristic confidence score. Values above 120 typically
            indicate real credentials; above 200 indicates high-confidence
            inline or formula-embedded detections.
        reason: Semicolon-separated breakdown of scoring factors
            (e.g., "distance=100;upper;lower;digit;special;length>=8").
        sheet: Name of the sheet holding the value. For CSV and TSV files
            this is the file name without its extension.
        row: 1-based row of the cell holding the value.
        col: 1-based column of the cell holding the value.
        header_row: 1-based row of the label the value was paired with.
            Equal to ``row`` for inline, formula and comment detections.
        header_col: 1-based column of the label the value was paired with.
        hidden: True if the sheet is hidden in the workbook.
    """

    key: str
    value: str
    confidence: float
    reason: str
    sheet: str
    row: int
    col: int
    header_row: int
    header_col: int
    hidden: bool = False

    @property
    def coordinate(self) -> str:
        """Spreadsheet reference of the value cell, e.g. ``"B3"``."""
        return f"{_column_letter(self.col)}{self.row}"

    @property
    def header_coordinate(self) -> str:
        """Spreadsheet reference of the label cell, e.g. ``"A3"``."""
        return f"{_column_letter(self.header_col)}{self.header_row}"

    def __repr__(self) -> str:
        # The value is a secret; keep it out of logs and tracebacks.
        return (
            f"Relationship(key={self.key!r}, sheet={self.sheet!r}, "
            f"cell={self.coordinate!r}, confidence={self.confidence:.0f})"
        )


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
        with filepath.open("rb") as f:
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
            sheet=r["sheet"].removesuffix(_HIDDEN_SUFFIX),
            row=r["row"],
            col=r["col"],
            header_row=r["header_row"],
            header_col=r["header_col"],
            hidden=r["sheet"].endswith(_HIDDEN_SUFFIX),
        )
        for r in raw_results
    ]

    return GridDoc(
        filepath=filepath,
        sheet_count=sheet_count,
        cell_count=cell_count,
        _relationships=rels,
    )
