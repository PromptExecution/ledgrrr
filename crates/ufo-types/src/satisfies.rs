//! Satisfies trait — uniform constraint satisfaction pattern.
//!
//! Every domain predicate (R&D eligibility, crypto cost basis rules, etc.)
//! implements Constraint; every domain entity implements Satisfies<C> against
//! the constraint types it must answer.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;
use crate::ufo::MomentStereotype;

/// Opaque node identifier for evidence graph integration.
///
/// Format matches arc-kit-au NodeId: `{type_prefix}:{blake3_hex}`.
/// Using a newtype here keeps ufo-types free of an arc-kit-au dependency;
/// callers convert between the two as needed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct NodeId(pub String);

impl NodeId {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Sealed marker for types that can act as constraints.
pub trait Constraint: Send + Sync {}

/// Outcome of a constraint satisfaction check.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum Disposition {
    /// All criteria met.
    Satisfied,
    /// One or more criteria failed; reason is human-readable.
    Violated { reason: String },
    /// Insufficient evidence to determine satisfaction.
    Unknown,
}

impl Disposition {
    pub fn is_satisfied(&self) -> bool {
        matches!(self, Disposition::Satisfied)
    }
}

/// Structured result of `Satisfies::satisfies`.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SatisfiesResult {
    /// Pass / fail / unknown verdict.
    pub disposition: Disposition,
    /// Confidence in the verdict [0.0, 1.0].
    pub confidence: f64,
    /// Evidence graph nodes that support or contradict this verdict.
    pub evidence_nodes: Vec<NodeId>,
    /// UFO moment stereotype that best describes this satisfaction relation.
    pub ufo_category: MomentStereotype,
}

impl SatisfiesResult {
    pub fn satisfied(confidence: f64, evidence_nodes: Vec<NodeId>) -> Self {
        Self {
            disposition: Disposition::Satisfied,
            confidence,
            evidence_nodes,
            ufo_category: MomentStereotype::Mode,
        }
    }

    pub fn violated(reason: impl Into<String>) -> Self {
        Self {
            disposition: Disposition::Violated { reason: reason.into() },
            confidence: 0.0,
            evidence_nodes: vec![],
            ufo_category: MomentStereotype::Mode,
        }
    }

    pub fn unknown() -> Self {
        Self {
            disposition: Disposition::Unknown,
            confidence: 0.0,
            evidence_nodes: vec![],
            ufo_category: MomentStereotype::Mode,
        }
    }
}

/// Core trait: does `self` satisfy constraint `C`?
pub trait Satisfies<C: Constraint + ?Sized> {
    fn satisfies(&self, constraint: &C) -> SatisfiesResult;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysSatisfied;
    impl Constraint for AlwaysSatisfied {}

    struct AlwaysViolated;
    impl Constraint for AlwaysViolated {}

    struct Subject;
    impl Satisfies<AlwaysSatisfied> for Subject {
        fn satisfies(&self, _: &AlwaysSatisfied) -> SatisfiesResult {
            SatisfiesResult::satisfied(1.0, vec![NodeId::new("doc:abc")])
        }
    }
    impl Satisfies<AlwaysViolated> for Subject {
        fn satisfies(&self, _: &AlwaysViolated) -> SatisfiesResult {
            SatisfiesResult::violated("intentionally violated")
        }
    }

    #[test]
    fn satisfied_result() {
        let r = Subject.satisfies(&AlwaysSatisfied);
        assert!(r.disposition.is_satisfied());
        assert_eq!(r.confidence, 1.0);
        assert_eq!(r.evidence_nodes.len(), 1);
    }

    #[test]
    fn violated_result() {
        let r = Subject.satisfies(&AlwaysViolated);
        assert!(!r.disposition.is_satisfied());
        assert_eq!(r.confidence, 0.0);
        assert!(matches!(r.disposition, Disposition::Violated { .. }));
    }

    #[test]
    fn unknown_result_roundtrip() {
        let r = SatisfiesResult::unknown();
        let json = serde_json::to_string(&r).unwrap();
        let back: SatisfiesResult = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.disposition, Disposition::Unknown));
    }
}
