"""
SHACL shape validation tests for kr0ki SysML-v2 process models (ledgrrr#254).

Run:  pytest ontology/sysml-v2/tests/
      just validate-process-shapes

Requires: pyshacl >= 0.22 (pip install pyshacl)
"""

from pathlib import Path

import pytest
import pyshacl

SHAPES_DIR = Path(__file__).parent.parent / "shapes"
FIXTURES_DIR = Path(__file__).parent.parent / "fixtures"
SHAPES_FILE = SHAPES_DIR / "process_model.ttl"


def _validate(fixture_name: str):
    """Run pyshacl against a fixture; return (conforms, report_text)."""
    data_path = FIXTURES_DIR / fixture_name
    conforms, _results_graph, report_text = pyshacl.validate(
        data_graph=str(data_path),
        shacl_graph=str(SHAPES_FILE),
        data_graph_format="turtle",
        shacl_graph_format="turtle",
        inference="none",
    )
    return conforms, report_text


# ── shapes sanity ─────────────────────────────────────────────────────────────

def test_shapes_file_exists():
    assert SHAPES_FILE.exists(), f"Shapes file missing: {SHAPES_FILE}"


def test_shapes_parse_without_error():
    """Shapes file must load as valid Turtle without a parse error."""
    from rdflib import Graph
    g = Graph()
    g.parse(str(SHAPES_FILE), format="turtle")
    assert len(g) > 0, "Shapes graph is empty after parse"


# ── valid fixture ─────────────────────────────────────────────────────────────

def test_valid_process_conforms():
    """A well-formed 3-step process must pass all shapes."""
    conforms, report = _validate("valid_process.ttl")
    assert conforms, f"Expected full conformance, got violations:\n{report}"


# ── invalid fixtures ──────────────────────────────────────────────────────────

def test_unnamed_action_fails():
    """An ActionUsage without sysml:name must fail ActionUsageShape."""
    conforms, report = _validate("invalid_action_unnamed.ttl")
    assert not conforms, "Expected a violation for unnamed ActionUsage"
    # The report must identify the path that failed
    assert "name" in report.lower(), (
        f"Expected 'name' in violation report, got:\n{report}"
    )


def test_flow_wrong_target_fails():
    """A SuccessionFlowUsage pointing to a PartUsage must fail SuccessionFlowShape."""
    conforms, report = _validate("invalid_flow_wrong_target.ttl")
    assert not conforms, "Expected a violation for flow with non-ActionUsage target"
    assert "target" in report.lower() or "PartUsage" in report, (
        f"Expected target-type violation, got:\n{report}"
    )


def test_decision_single_outbound_fails():
    """A DecisionNode with only 1 outbound edge must fail DecisionNodeShape."""
    conforms, report = _validate("invalid_decision_single_outbound.ttl")
    assert not conforms, "Expected a violation for DecisionNode with 1 outbound edge"


# ── violation report fields ───────────────────────────────────────────────────

def test_violation_report_contains_focus_node():
    """Every violation report must include the focus node IRI."""
    conforms, report = _validate("invalid_action_unnamed.ttl")
    assert not conforms
    # pyshacl text reports always include "Focus Node:" for violations
    assert "Focus Node" in report or "focusNode" in report, (
        f"Violation report missing focus node:\n{report}"
    )
