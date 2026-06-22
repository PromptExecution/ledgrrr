//! UFO (Unified Foundational Ontology) category stereotypes.
//!
//! Based on Guizzardi (2005) "Ontological Foundations for Structural
//! Conceptual Models", CTIT PhD Thesis Series, No. 05-74.

use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

/// Top-level UFO ontological category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UfoCategory {
    /// Endurants: entities that exist wholly at each moment of time (individuals, universals).
    Endurant,
    /// Perdurants: entities that are "spread out" in time (events, processes).
    Perdurant,
    /// Moments: entities that are inherent to other entities (properties, relations).
    Moment,
    /// Abstract: mathematical, logical, or formal entities with no spatio-temporal location.
    Abstract,
}

impl std::fmt::Display for UfoCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UfoCategory::Endurant => write!(f, "Endurant"),
            UfoCategory::Perdurant => write!(f, "Perdurant"),
            UfoCategory::Moment => write!(f, "Moment"),
            UfoCategory::Abstract => write!(f, "Abstract"),
        }
    }
}

/// UFO endurant stereotypes (Guizzardi 2005, §4–5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EndurantStereotype {
    /// Substantial universal with its own identity criterion; most specific sortal.
    Kind,
    /// Specialises a Kind without introducing a new identity criterion.
    SubKind,
    /// Externally-defined role played by a Kind in a relator context.
    Role,
    /// Sortal whose instances change membership dynamically over time.
    Phase,
    /// Non-sortal universal that collects instances from multiple Kinds.
    Category,
    /// Non-sortal universal contributed by multiple Kinds (no own identity).
    Mixin,
    /// Non-sortal mixin that is also role-constrained.
    RoleMixin,
}

impl std::fmt::Display for EndurantStereotype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EndurantStereotype::Kind => "Kind",
            EndurantStereotype::SubKind => "SubKind",
            EndurantStereotype::Role => "Role",
            EndurantStereotype::Phase => "Phase",
            EndurantStereotype::Category => "Category",
            EndurantStereotype::Mixin => "Mixin",
            EndurantStereotype::RoleMixin => "RoleMixin",
        };
        write!(f, "{s}")
    }
}

/// UFO perdurant stereotypes (Guizzardi 2005, §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PerdurantStereotype {
    /// Ongoing temporal entity with internal causal structure.
    Process,
    /// Temporally extended but homogeneous condition.
    State,
    /// Punctual change of state; atomic perdurant.
    Event,
    /// Complex perdurant composed of heterogeneous sub-parts in temporal order.
    Scenario,
}

impl std::fmt::Display for PerdurantStereotype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            PerdurantStereotype::Process => "Process",
            PerdurantStereotype::State => "State",
            PerdurantStereotype::Event => "Event",
            PerdurantStereotype::Scenario => "Scenario",
        };
        write!(f, "{s}")
    }
}

/// UFO moment stereotypes (Guizzardi 2005, §7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MomentStereotype {
    /// Intrinsic moment — property inherent to a single individual (quality, disposition).
    Mode,
    /// Relator — moment that mediates a material relation between two or more individuals.
    Relator,
}

impl std::fmt::Display for MomentStereotype {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MomentStereotype::Mode => write!(f, "Mode"),
            MomentStereotype::Relator => write!(f, "Relator"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ufo_category_roundtrip() {
        for cat in [UfoCategory::Endurant, UfoCategory::Perdurant, UfoCategory::Moment, UfoCategory::Abstract] {
            let json = serde_json::to_string(&cat).unwrap();
            let back: UfoCategory = serde_json::from_str(&json).unwrap();
            assert_eq!(cat, back);
        }
    }

    #[test]
    fn endurant_display() {
        assert_eq!(EndurantStereotype::Kind.to_string(), "Kind");
        assert_eq!(EndurantStereotype::RoleMixin.to_string(), "RoleMixin");
    }

    #[test]
    fn moment_stereotype_variants() {
        let mode = MomentStereotype::Mode;
        let rel = MomentStereotype::Relator;
        assert_ne!(mode, rel);
    }
}
