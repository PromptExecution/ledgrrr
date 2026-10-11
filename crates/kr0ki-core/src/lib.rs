//! kr0ki-core: maps SysML-v2 process elements to a lazily::StateChart.
//!
//! # Mapping
//! - `ElementKind::ActionUsage`     → atomic leaf state
//! - `ElementKind::ActionDefinition`→ compound root state (wraps its usages)
//! - `Relation::Succession`         → transition on event `"next"`
//!
//! The resulting `ProcessChart` can be driven by calling `send("next", &{})`.

#![forbid(unsafe_code)]

pub mod chart;
pub mod error;

pub use chart::ProcessChart;
pub use error::Kr0kiError;
