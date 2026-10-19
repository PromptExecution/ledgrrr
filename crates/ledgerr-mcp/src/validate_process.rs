//! Process model SHACL validation — ledgrrr#255.
//!
//! Validates a SysML-v2 process graph (Turtle or JSON-LD) against the
//! kr0ki process-model SHACL shapes (`ontology/sysml-v2/shapes/process_model.ttl`).
//! Delegates to `pyshacl` via a subprocess; returns a typed `ProcessValidationResult`
//! that kr0ki's FR6 reference resolver can consume directly.
//!
//! # Shape file resolution
//!
//! `LEDGRRR_SHAPES_DIR` env var overrides the default, which is resolved
//! relative to the binary's location at runtime:
//!   `<binary_dir>/../ontology/sysml-v2/shapes/process_model.ttl`
//!
//! In tests, set `LEDGRRR_SHAPES_DIR` to the repo root's
//! `ontology/sysml-v2/shapes/` directory.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::ToolError;

// ── public types ──────────────────────────────────────────────────────────────

/// Result of validating a process graph against the kr0ki SHACL shapes.
/// Schema is stable across ledgrrr versions; consumers key on `conformant`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessValidationResult {
    /// `true` iff the graph passes all shapes with no violations.
    pub conformant: bool,
    /// Empty when `conformant == true`.
    pub violations: Vec<Violation>,
}

/// One SHACL constraint violation from the validation report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Violation {
    /// IRI of the node that failed the shape (sh:focusNode).
    pub focus_node: String,
    /// IRI of the property that was checked, when present (sh:resultPath).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result_path: Option<String>,
    /// Human-readable explanation from the shape's sh:message.
    pub result_message: String,
    /// `"Violation"` | `"Warning"` | `"Info"` (mirrors sh:Violation / sh:Warning / sh:Info).
    pub severity: String,
}

/// Accepted input graph formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphFormat {
    Turtle,
    JsonLd,
}

impl GraphFormat {
    fn as_pyshacl_flag(self) -> &'static str {
        match self {
            GraphFormat::Turtle => "turtle",
            GraphFormat::JsonLd => "json-ld",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            GraphFormat::Turtle => "ttl",
            GraphFormat::JsonLd => "json",
        }
    }
}

// ── public entry point ────────────────────────────────────────────────────────

/// Validate `graph_text` (Turtle or JSON-LD) against the process-model SHACL shapes.
///
/// Writes the graph to a temp file, invokes `pyshacl`, and parses the report.
/// Returns `Err` only for I/O or subprocess failures — a conformance failure
/// is a successful validation that happens to have `conformant: false`.
pub fn validate_process_graph(
    graph_text: &str,
    format: GraphFormat,
) -> Result<ProcessValidationResult, ToolError> {
    let shapes_path = resolve_shapes_path()?;
    let result = run_pyshacl(graph_text, format, &shapes_path)?;
    Ok(result)
}

// ── internal ──────────────────────────────────────────────────────────────────

fn resolve_shapes_path() -> Result<PathBuf, ToolError> {
    if let Ok(dir) = std::env::var("LEDGRRR_SHAPES_DIR") {
        let p = PathBuf::from(dir).join("process_model.ttl");
        if p.exists() {
            return Ok(p);
        }
        return Err(ToolError::Internal(format!(
            "LEDGRRR_SHAPES_DIR set but process_model.ttl not found at {p:?}"
        )));
    }

    // Heuristic: walk up from the binary or from CWD to find the repo root.
    // Works for `cargo test` (CWD = repo root) and installed binaries.
    let candidates = [
        PathBuf::from("ontology/sysml-v2/shapes/process_model.ttl"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .map(|p| p.join("../ontology/sysml-v2/shapes/process_model.ttl"))
            .unwrap_or_default(),
    ];

    for c in &candidates {
        let resolved = c.canonicalize().unwrap_or_else(|_| c.clone());
        if resolved.exists() {
            return Ok(resolved);
        }
    }

    Err(ToolError::Internal(
        "process_model.ttl not found; set LEDGRRR_SHAPES_DIR to ontology/sysml-v2/shapes/"
            .to_string(),
    ))
}

fn run_pyshacl(
    graph_text: &str,
    format: GraphFormat,
    shapes_path: &Path,
) -> Result<ProcessValidationResult, ToolError> {
    // Write the data graph to a named temp file (pyshacl needs a path, not stdin).
    let mut tmp = tempfile::Builder::new()
        .suffix(&format!(".{}", format.extension()))
        .tempfile()
        .map_err(|e| ToolError::Internal(format!("tempfile: {e}")))?;
    tmp.write_all(graph_text.as_bytes())
        .map_err(|e| ToolError::Internal(format!("write tempfile: {e}")))?;
    tmp.flush()
        .map_err(|e| ToolError::Internal(format!("flush tempfile: {e}")))?;
    let tmp_path = tmp.path().to_path_buf();

    // pyshacl CLI: pyshacl -s <shapes> -df <format> <data>
    let output = Command::new("pyshacl")
        .args([
            "-s",
            shapes_path
                .to_str()
                .ok_or_else(|| ToolError::Internal("non-UTF8 shapes path".into()))?,
            "-df",
            format.as_pyshacl_flag(),
            tmp_path
                .to_str()
                .ok_or_else(|| ToolError::Internal("non-UTF8 temp path".into()))?,
        ])
        .output()
        .map_err(|e| ToolError::Internal(format!("pyshacl subprocess: {e}")))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // pyshacl exits 0 for conformant, non-zero for violations or errors.
    // A parse error from pyshacl prints to stderr and exits non-zero with no report.
    if !stderr.is_empty() && stdout.is_empty() {
        return Err(ToolError::InvalidInput(format!(
            "pyshacl parse/validation error: {stderr}"
        )));
    }

    parse_pyshacl_report(&stdout, output.status.success())
}

/// Parse pyshacl's text report into a `ProcessValidationResult`.
///
/// pyshacl text report format (abridged):
/// ```text
/// Validation Report
/// Conforms: False
/// Results (2):
/// Constraint Violation in MinCountConstraintComponent ...
///   Focus Node: <urn:...>
///   Result Path: sysml:name
///   Message: Every ActionUsage must have a sysml:name ...
///   Severity: sh:Violation
/// ```
fn parse_pyshacl_report(
    report: &str,
    exit_ok: bool,
) -> Result<ProcessValidationResult, ToolError> {
    let conforms = exit_ok
        || report
            .lines()
            .any(|l| l.trim().eq_ignore_ascii_case("conforms: true"));

    if conforms {
        return Ok(ProcessValidationResult {
            conformant: true,
            violations: Vec::new(),
        });
    }

    let violations = extract_violations(report);
    Ok(ProcessValidationResult {
        conformant: false,
        violations,
    })
}

fn extract_violations(report: &str) -> Vec<Violation> {
    // Each violation block starts with "Constraint Violation" and ends before
    // the next one or end of input.
    let mut violations = Vec::new();
    let mut in_block = false;
    let mut focus_node = String::new();
    let mut result_path: Option<String> = None;
    let mut result_message = String::new();
    let mut severity = "Violation".to_string();

    let flush = |v: &mut Vec<Violation>,
                 focus: &str,
                 path: Option<String>,
                 msg: &str,
                 sev: &str| {
        if !focus.is_empty() {
            v.push(Violation {
                focus_node: focus.trim().to_string(),
                result_path: path,
                result_message: msg.trim().to_string(),
                severity: sev.to_string(),
            });
        }
    };

    for line in report.lines() {
        let t = line.trim();
        if t.starts_with("Constraint Violation") || t.starts_with("Constraint Warning") {
            if in_block {
                flush(
                    &mut violations,
                    &focus_node,
                    result_path.take(),
                    &result_message,
                    &severity,
                );
            }
            in_block = true;
            focus_node.clear();
            result_path = None;
            result_message.clear();
            severity = if t.contains("Warning") {
                "Warning".to_string()
            } else {
                "Violation".to_string()
            };
        } else if in_block {
            if let Some(val) = t.strip_prefix("Focus Node:") {
                focus_node = val.trim().to_string();
            } else if let Some(val) = t.strip_prefix("Result Path:") {
                result_path = Some(val.trim().to_string());
            } else if let Some(val) = t.strip_prefix("Message:") {
                result_message = val.trim().to_string();
            } else if let Some(val) = t.strip_prefix("Severity:") {
                let s = val.trim();
                severity = if s.contains("Warning") {
                    "Warning".to_string()
                } else if s.contains("Info") {
                    "Info".to_string()
                } else {
                    "Violation".to_string()
                };
            }
        }
    }
    if in_block {
        flush(
            &mut violations,
            &focus_node,
            result_path.take(),
            &result_message,
            &severity,
        );
    }

    violations
}

// ── tests ──────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn shapes_dir() -> PathBuf {
        // Walk up from CARGO_MANIFEST_DIR to find repo root
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let root = manifest
            .ancestors()
            .find(|p| p.join("ontology").exists())
            .unwrap_or(&manifest)
            .to_path_buf();
        root.join("ontology/sysml-v2/shapes")
    }

    fn fixture(name: &str) -> String {
        let p = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find(|p| p.join("ontology").exists())
            .unwrap()
            .join("ontology/sysml-v2/fixtures")
            .join(name);
        std::fs::read_to_string(&p)
            .unwrap_or_else(|e| panic!("cannot read fixture {name}: {e}"))
    }

    fn validate(fixture_name: &str) -> ProcessValidationResult {
        std::env::set_var("LEDGRRR_SHAPES_DIR", shapes_dir());
        let text = fixture(fixture_name);
        validate_process_graph(&text, GraphFormat::Turtle)
            .unwrap_or_else(|e| panic!("validation error for {fixture_name}: {e}"))
    }

    #[test]
    fn valid_process_conforms() {
        let r = validate("valid_process.ttl");
        assert!(r.conformant, "expected conformant; violations: {:#?}", r.violations);
        assert!(r.violations.is_empty());
    }

    #[test]
    fn unnamed_action_fails() {
        let r = validate("invalid_action_unnamed.ttl");
        assert!(!r.conformant, "expected violation for unnamed ActionUsage");
        assert!(!r.violations.is_empty());
    }

    #[test]
    fn flow_wrong_target_fails() {
        let r = validate("invalid_flow_wrong_target.ttl");
        assert!(!r.conformant, "expected violation for wrong-type target");
    }

    #[test]
    fn decision_single_outbound_fails() {
        let r = validate("invalid_decision_single_outbound.ttl");
        assert!(!r.conformant, "expected violation for DecisionNode with 1 outbound");
    }

    #[test]
    fn non_rdf_input_returns_err() {
        std::env::set_var("LEDGRRR_SHAPES_DIR", shapes_dir());
        let result = validate_process_graph("this is not turtle", GraphFormat::Turtle);
        assert!(
            result.is_err(),
            "expected Err for unparseable input, got: {result:?}"
        );
    }

    #[test]
    fn conformant_result_has_empty_violations() {
        let r = validate("valid_process.ttl");
        assert_eq!(r.violations.len(), 0);
    }
}
