#!/usr/bin/env python3
"""
check-determinism.py
====================
Calls generate-training-data.py, captures output, diffs against committed
snapshot at scripts/training/data/training_pairs.json.

Also tracks coverage: what % of the 32 canonical DSL snippets appear in
training pairs, reports missing types, and generates a coverage report.

Usage:
  uv run python3 scripts/training/check-determinism.py
  uv run python3 scripts/training/check-determinism.py --update  # update snapshot

Returns exit code 0 if identical and coverage >= 80%, 1 otherwise.
"""

import argparse
import difflib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPTS_DIR = REPO_ROOT / "scripts" / "training"
SNAPSHOT_DIR = SCRIPTS_DIR / "data"
SNAPSHOT_PATH = SNAPSHOT_DIR / "training_pairs.json"
TRAINING_DATA_PATH = SCRIPTS_DIR / "train" / "training-data.jsonl"
COVERAGE_REPORT_PATH = SCRIPTS_DIR / "train" / "coverage-report.json"

# The 32 canonical DSL type IDs (matching build.rs module_map)
CANONICAL_DSL_TYPES = [
    "pipeline::PipelineState<Ingested>",
    "pipeline::PipelineState<Validated>",
    "pipeline::PipelineState<Classified>",
    "pipeline::PipelineState<Reconciled>",
    "pipeline::PipelineState<Committed>",
    "pipeline::PipelineState<NeedsReview>",
    "governance::GovernanceState<Submitted>",
    "governance::GovernanceState<PolicyChecked>",
    "governance::GovernanceState<Consented>",
    "governance::GovernanceState<Executed>",
    "governance::GovernanceState<Audited>",
    "governance::GovernanceState<Closed>",
    "constraints::ConstraintEvaluation",
    "constraints::VendorConstraintSet",
    "constraints::InvoiceConstraintSolver",
    "constraints::InvoiceVerification",
    "legal::Z3Result",
    "legal::LegalRule",
    "legal::LegalSolver",
    "legal::Jurisdiction",
    "legal::TransactionFacts",
    "validation::CommitGate",
    "validation::Issue",
    "validation::MetaFlag",
    "validation::MetaCtx",
    "validation::Disposition",
    "validation::StageResult<()>",
    "pipeline::KasuariSolver",
    "arc_kit_au::Classification",
    "arc_kit_au::ModelProposal",
    "arc_kit_au::OperatorApproval",
    "attest::AttestationSpec",
]


def run_generate() -> list[dict]:
    """Run generate-training-data.py and return parsed training pairs."""
    gen_script = SCRIPTS_DIR / "generate-training-data.py"
    if not gen_script.exists():
        print(f"ERROR: {gen_script} not found", file=sys.stderr)
        sys.exit(1)

    with tempfile.TemporaryDirectory() as tmpdir:
        # Run in isolated temp to avoid mixing with existing output
        env_dict = {k: v for k, v in os.environ.items()}
        # Run the gen script from the repo root so it finds all its paths
        result = subprocess.run(
            [sys.executable, str(gen_script)],
            cwd=str(REPO_ROOT),
            capture_output=True,
            text=True,
            timeout=120,
        )
        if result.returncode != 0:
            print(f"generate-training-data.py failed (exit {result.returncode}):")
            print(result.stderr)
            sys.exit(1)

        # Check for output in the expected train dir
        output_file = TRAINING_DATA_PATH
        if not output_file.exists():
            print(f"ERROR: no training-data.jsonl generated at {output_file}", file=sys.stderr)
            print("stdout:", result.stdout[:500], file=sys.stderr)
            print("stderr:", result.stderr[:500], file=sys.stderr)
            sys.exit(1)

        pairs = []
        with open(output_file) as f:
            for line in f:
                line = line.strip()
                if line:
                    pairs.append(json.loads(line))
        return pairs


def load_snapshot() -> list[dict] | None:
    """Load committed snapshot, return None if missing."""
    if not SNAPSHOT_PATH.exists():
        return None
    with open(SNAPSHOT_PATH) as f:
        return json.load(f)


def write_snapshot(pairs: list[dict]) -> None:
    """Write snapshot file."""
    SNAPSHOT_DIR.mkdir(parents=True, exist_ok=True)
    with open(SNAPSHOT_PATH, "w") as f:
        json.dump(pairs, f, indent=2, sort_keys=True)
    print(f"Snapshot written to {SNAPSHOT_PATH}")


def diff_pairs(generated: list[dict], snapshot: list[dict]) -> str:
    """Return unified diff between generated and snapshot content."""
    generated_json = json.dumps(generated, indent=2, sort_keys=True)
    snapshot_json = json.dumps(snapshot, indent=2, sort_keys=True)
    diff = list(
        difflib.unified_diff(
            snapshot_json.splitlines(keepends=True),
            generated_json.splitlines(keepends=True),
            fromfile="snapshot (training_pairs.json)",
            tofile="generated (training-data.jsonl)",
        )
    )
    return "".join(diff)


def compute_coverage(pairs: list[dict]) -> dict:
    """Compute what % of the 32 canonical DSL types appear in training pairs.

    The type_name in the training data metadata follows the 'module::TypeName'
    convention for viz-manifest.json entries, or 'ArtifactKind::X' /
    'RelationKind::X' / 'MCP::action' for ontology entries.

    We scan the type_name from metadata or derive from the prompt/entry.
    """
    covered = set()
    type_name_map = {}

    for pair in pairs:
        meta = pair.get("metadata", {})
        type_name = meta.get("type_name", "")
        type_name_map[type_name] = type_name_map.get(type_name, 0) + 1

        # Check exact match
        if type_name in CANONICAL_DSL_TYPES:
            covered.add(type_name)
            continue

        # Check approximate match (some entries may use slightly different naming)
        for canonical in CANONICAL_DSL_TYPES:
            short = canonical.split("::")[-1] if "::" in canonical else canonical
            if short in type_name or canonical in type_name:
                covered.add(canonical)
                break

    covered_count = len(covered)
    total = len(CANONICAL_DSL_TYPES)
    coverage_pct = round(covered_count / total * 100, 1) if total > 0 else 0.0

    missing = [t for t in CANONICAL_DSL_TYPES if t not in covered]

    return {
        "total_canonical_types": total,
        "covered_types": covered_count,
        "coverage_percent": coverage_pct,
        "covered_type_ids": sorted(covered),
        "missing_type_ids": missing,
        "total_training_pairs": len(pairs),
    }


def print_coverage_table(coverage: dict) -> None:
    """Print a formatted summary table of coverage."""
    print()
    print("=" * 70)
    print("  TRAINING DATA COVERAGE REPORT")
    print("=" * 70)
    print(f"  Total canonical DSL types:  {coverage['total_canonical_types']}")
    print(f"  Covered in training pairs:  {coverage['covered_types']}")
    print(f"  Coverage:                   {coverage['coverage_percent']}%")
    print(f"  Total training pairs:       {coverage['total_training_pairs']}")
    print()

    missing = coverage["missing_type_ids"]
    if missing:
        print(f"  MISSING TYPES ({len(missing)}):")
        for t in missing:
            print(f"    - {t}")
        print()
    else:
        print("  All 32 canonical types are covered!")
        print()

    covered = coverage["covered_type_ids"]
    print(f"  COVERED TYPES ({len(covered)}):")
    for t in covered:
        print(f"    + {t}")
    print()


def write_coverage_report(coverage: dict) -> None:
    """Write the coverage report as JSON."""
    COVERAGE_REPORT_PATH.parent.mkdir(parents=True, exist_ok=True)
    with open(COVERAGE_REPORT_PATH, "w") as f:
        json.dump(coverage, f, indent=2, sort_keys=True)
    print(f"Coverage report written to {COVERAGE_REPORT_PATH}")


def main():
    parser = argparse.ArgumentParser(
        description="Check determinism of training data generation with coverage tracking"
    )
    parser.add_argument(
        "--update",
        action="store_true",
        help="Update the committed snapshot with current output",
    )
    parser.add_argument(
        "--coverage-only",
        action="store_true",
        help="Only compute and print coverage from existing training data (no determinism check)",
    )
    args = parser.parse_args()

    # If --coverage-only, load existing training data and report coverage
    if args.coverage_only:
        if not TRAINING_DATA_PATH.exists():
            print(f"No training data found at {TRAINING_DATA_PATH}", file=sys.stderr)
            sys.exit(1)
        pairs = []
        with open(TRAINING_DATA_PATH) as f:
            for line in f:
                line = line.strip()
                if line:
                    pairs.append(json.loads(line))
        print(f"Loaded {len(pairs)} training pairs from {TRAINING_DATA_PATH}")
        coverage = compute_coverage(pairs)
        print_coverage_table(coverage)
        write_coverage_report(coverage)
        sys.exit(0)

    print("Running generate-training-data.py...")
    generated = run_generate()
    print(f"  Generated {len(generated)} training pairs")

    # Compute coverage
    coverage = compute_coverage(generated)
    print_coverage_table(coverage)
    write_coverage_report(coverage)

    if args.update:
        write_snapshot(generated)
        print("  Snapshot updated. PASS")
        sys.exit(0)

    snapshot = load_snapshot()
    if snapshot is None:
        print(f"No committed snapshot found at {SNAPSHOT_PATH}")
        print("Run with --update to create the initial snapshot.")
        print("  uv run python3 scripts/training/check-determinism.py --update")
        sys.exit(1)

    print(f"  Snapshot has {len(snapshot)} pairs")
    diff = diff_pairs(generated, snapshot)
    if not diff:
        print("  Output is identical to snapshot. PASS")
        if coverage["coverage_percent"] < 80.0:
            print(f"  WARNING: Coverage ({coverage['coverage_percent']}%) is below 80% threshold!")
            print(f"  Missing types: {coverage['missing_type_ids']}")
            sys.exit(1)
        sys.exit(0)
    else:
        print("  FAIL: Output differs from snapshot!")
        print(diff)
        sys.exit(1)


if __name__ == "__main__":
    main()
