"""Precision/recall benchmark harness for doormat fixtures.

Runs all fixtures through doormat.load() and compares against ground truth.
Prints a per-fixture and aggregate precision/recall report.

Usage:
    python bench/harness/run_harness.py
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

import doormat

FIXTURES_DIR = Path(__file__).resolve().parent.parent / "fixtures"
GROUND_TRUTH_PATH = FIXTURES_DIR / "ground_truth.json"


def load_ground_truth() -> dict:
    with open(GROUND_TRUTH_PATH) as f:
        return json.load(f)


def finding_matches(rel, expected_entry: dict) -> bool:
    """Check if a Relationship matches a ground truth entry."""
    key_match = rel.key.lower() == expected_entry["key"].lower()
    value_match = rel.value == expected_entry["value"]
    confidence_match = rel.confidence >= expected_entry["min_confidence"]
    reason_match = expected_entry["reason_contains"] in rel.reason
    return key_match and value_match and confidence_match and reason_match


def evaluate_fixture(
    filepath: Path, expected: list[dict]
) -> tuple[int, int, int]:
    """Evaluate a single fixture. Returns (true_positives, false_positives, false_negatives)."""
    doc = doormat.load(filepath)
    rels = doc.relationships()

    # Track which expected entries are matched
    matched_expected: set[int] = set()
    matched_rels: set[int] = set()

    for ei, entry in enumerate(expected):
        for ri, rel in enumerate(rels):
            if ri not in matched_rels and finding_matches(rel, entry):
                matched_expected.add(ei)
                matched_rels.add(ri)
                break

    tp = len(matched_expected)
    fn = len(expected) - tp
    fp = len(rels) - len(matched_rels)

    return tp, fp, fn


def run_all() -> None:
    """Run all fixtures and print precision/recall report."""
    ground_truth = load_ground_truth()

    header = f"{'Fixture':<30} {'TP':>4} {'FP':>4} {'FN':>4} {'Prec':>6} {'Recall':>6}"
    print(header)
    print("-" * len(header))

    total_tp = 0
    total_fp = 0
    total_fn = 0

    for filename, expected in sorted(ground_truth.items()):
        filepath = FIXTURES_DIR / filename
        if not filepath.exists():
            print(f"{filename:<30} {'MISSING':>4}")
            continue

        tp, fp, fn = evaluate_fixture(filepath, expected)
        total_tp += tp
        total_fp += fp
        total_fn += fn

        prec = tp / (tp + fp) if (tp + fp) > 0 else 1.0
        recall = tp / (tp + fn) if (tp + fn) > 0 else 1.0

        print(f"{filename:<30} {tp:>4} {fp:>4} {fn:>4} {prec:>6.2f} {recall:>6.2f}")

    print("-" * len(header))

    agg_prec = total_tp / (total_tp + total_fp) if (total_tp + total_fp) > 0 else 1.0
    agg_recall = total_tp / (total_tp + total_fn) if (total_tp + total_fn) > 0 else 1.0

    print(f"{'AGGREGATE':<30} {total_tp:>4} {total_fp:>4} {total_fn:>4} {agg_prec:>6.2f} {agg_recall:>6.2f}")
    print()
    print(f"Precision: {agg_prec:.4f}")
    print(f"Recall:    {agg_recall:.4f}")

    if agg_prec < 0.9 or agg_recall < 0.85:
        print("\n⚠ Below target (P >= 0.9, R >= 0.85)")
        sys.exit(1)
    else:
        print("\n✓ Meets target (P >= 0.9, R >= 0.85)")


if __name__ == "__main__":
    run_all()
