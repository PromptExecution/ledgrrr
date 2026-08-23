//! SysML v2 textual-syntax validation, shared across every crate that
//! generates SysML v2 text (`holon-viz`'s `SysmlV2Emitter`, `sysml-derive`'s
//! `#[derive(SysmlBlock)]`).
//!
//! Wired to `sysml-v2-parser` (`elan8/sysml-v2-parser` on GitHub,
//! `sysml-v2-parser` on crates.io) — a real, if young (0.x, ~5 months old)
//! SysML v2 grammar implementation, not a hand-rolled heuristic. See
//! `docs/sysml-v2-parser-spike.md` in this repo for the crate-health
//! writeup that led to adopting it (verdict: adopt-with-caveats — genuine
//! grammar coverage, but expect upstream gaps beyond basic
//! package/part/attribute constructs).
//!
//! This module intentionally validates *syntax only* (does it parse under
//! SysML v2's grammar), not semantic well-formedness (do referenced types
//! resolve, are multiplicities consistent, etc.) — that's a much larger
//! problem this crate does not attempt to solve.

use crate::satisfies::{Constraint, NodeId, SatisfiesResult, Satisfies};
use sysml_v2_parser::parse_for_editor;

/// Constraint: the subject text is syntactically valid SysML v2, per
/// `sysml-v2-parser`'s resilient-editor-mode parser.
pub struct SysmlV2Syntax;

impl Constraint for SysmlV2Syntax {}

impl Satisfies<SysmlV2Syntax> for str {
    fn satisfies(&self, _constraint: &SysmlV2Syntax) -> SatisfiesResult {
        validate_sysml_v2(self)
    }
}

impl Satisfies<SysmlV2Syntax> for String {
    fn satisfies(&self, constraint: &SysmlV2Syntax) -> SatisfiesResult {
        self.as_str().satisfies(constraint)
    }
}

/// Parse `text` under SysML v2's grammar (via `parse_for_editor`'s resilient
/// mode — never panics, always returns diagnostics) and report the result as
/// a [`SatisfiesResult`]: `Satisfied` if there are zero diagnostics,
/// `Violated` with the joined diagnostic messages (each with its
/// line/column, when available) otherwise.
pub fn validate_sysml_v2(text: &str) -> SatisfiesResult {
    let result = parse_for_editor(text);
    if result.is_ok() {
        return SatisfiesResult::satisfied(1.0, Vec::<NodeId>::new());
    }
    let reason = result
        .errors
        .iter()
        .map(|e| match (e.line, e.column) {
            (Some(l), Some(c)) => format!("line {l}, col {c}: {}", e.message),
            _ => e.message.clone(),
        })
        .collect::<Vec<_>>()
        .join("; ");
    SatisfiesResult::violated(reason)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_sysml_v2_is_satisfied() {
        let text = "package Foo {\n    part def Bar {\n        attribute x : ScalarValues::Boolean;\n    }\n}\n";
        let result = validate_sysml_v2(text);
        assert!(result.disposition.is_satisfied(), "{:?}", result.disposition);
    }

    #[test]
    fn sysml_v1_block_def_keyword_is_rejected() {
        // SysML v1 called this construct `Block`; SysML v2 renamed it to
        // `part def`. `block def` is not a SysML v2 keyword at all.
        let text = "package Foo {\n    block def Bar {\n    }\n}\n";
        let result = validate_sysml_v2(text);
        assert!(!result.disposition.is_satisfied());
    }

    #[test]
    fn comment_swallowing_closing_brace_is_rejected() {
        // A `//` line comment on the same line as a closing `}` comments the
        // brace out, leaving the block unclosed.
        let text = "package Foo {\n    part def Bar { // note\n}\n";
        let result = validate_sysml_v2(text);
        assert!(!result.disposition.is_satisfied());
    }

    #[test]
    fn satisfies_trait_is_usable_on_str_and_string() {
        let owned = String::from("package Foo {\n}\n");
        assert!(owned.satisfies(&SysmlV2Syntax).disposition.is_satisfied());
        assert!(owned.as_str().satisfies(&SysmlV2Syntax).disposition.is_satisfied());
    }
}
