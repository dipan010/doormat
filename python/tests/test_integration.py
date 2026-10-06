"""Integration tests: run doormat.load() against all fixtures and verify ground truth."""

from __future__ import annotations

import json
from pathlib import Path

import pytest

import doormat

FIXTURES_DIR = Path(__file__).resolve().parent.parent.parent / "bench" / "fixtures"
GROUND_TRUTH_PATH = FIXTURES_DIR / "ground_truth.json"


def load_ground_truth() -> dict:
    with open(GROUND_TRUTH_PATH) as f:
        return json.load(f)


GROUND_TRUTH = load_ground_truth()


def fixture_params():
    """Yield (fixture_path, expected_list) for parametrize."""
    for filename, expected in GROUND_TRUTH.items():
        fixture_path = FIXTURES_DIR / filename
        yield pytest.param(fixture_path, expected, id=filename)


def finding_matches(rel, expected_entry: dict) -> bool:
    """Check if a Relationship matches a ground truth entry."""
    key_match = rel.key.lower() == expected_entry["key"].lower()
    value_match = rel.value == expected_entry["value"]
    confidence_match = rel.confidence >= expected_entry["min_confidence"]
    reason_match = expected_entry["reason_contains"] in rel.reason
    return key_match and value_match and confidence_match and reason_match


@pytest.mark.parametrize("fixture_path,expected", list(fixture_params()))
def test_fixture_findings(fixture_path: Path, expected: list[dict]):
    """Each fixture's findings must match ground truth expectations."""
    doc = doormat.load(fixture_path)
    rels = doc.relationships()

    # Every expected finding must be present
    for entry in expected:
        matched = any(finding_matches(r, entry) for r in rels)
        assert matched, (
            f"Expected finding not detected: key={entry['key']!r}, "
            f"value={entry['value']!r}, reason_contains={entry['reason_contains']!r}. "
            f"Got {len(rels)} relationships: {rels}"
        )


def test_no_false_positives():
    """13_no_credentials.xlsx must return zero findings."""
    fixture_path = FIXTURES_DIR / "13_no_credentials.xlsx"
    doc = doormat.load(fixture_path)
    rels = doc.relationships()
    assert rels == [], f"Expected no findings, got: {rels}"
