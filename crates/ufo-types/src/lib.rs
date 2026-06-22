//! UFO ontology stereotypes, Satisfies trait, and ISO standard types.
//!
//! Provides the ontological foundation for the tax-lawyer platform.
//! All domain concepts are grounded in UFO (Unified Foundational Ontology)
//! stereotypes (Guizzardi 2005) with ISO-standard identifiers.

pub mod iso;
pub mod satisfies;
pub mod ufo;

pub use iso::{BankAccount, Currency, FinancialInstrument, Isin, Lei};
pub use satisfies::{Constraint, Disposition, NodeId, SatisfiesResult, Satisfies};
pub use ufo::{EndurantStereotype, MomentStereotype, PerdurantStereotype, UfoCategory};
