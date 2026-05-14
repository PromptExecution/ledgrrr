//! # holon-viz — Holonic Visualization Engine
//!
//! Provides a recursive holarchy model (`Holon`), deterministic Cytoscape.js
//! JSON emission (`CytoscapeGraph`), SysML-v2 and OWL2/Turtle text emitters,
//! and an append-only process controller with Blake3-authorized transitions.
//!
//! ## Design Notes
//!
//! - All IDs are `String` (typically hex-encoded Blake3 digests).
//! - `CytoscapeGraph` targets the Cytoscape.js JSON format consumed by the
//!   browser layer — no JS runtime is pulled in as a Rust dependency.
//! - `ImmutableActionLog` is sealed at the type level: the only write path is
//!   `append`, which computes and stores the Blake3 hash of each record.
//! - `ProcessController` authorizes transitions by hashing
//!   `(step_id, authorized_by, timestamp_ms)` — the resulting digest is
//!   embedded in the returned `TransitionReceipt` for audit.

pub mod controller;
pub mod cytoscape;
pub mod emitter;
pub mod holon;
pub mod log;
pub mod observer;
pub mod renderer;
pub mod gen;
pub mod type_graph;

pub use controller::{ProcessController, ProcessStep, TransitionReceipt};
pub use cytoscape::{CytoscapeEdge, CytoscapeGraph, CytoscapeNode};
pub use emitter::{Owl2Emitter, SysmlV2Emitter};
pub use holon::{Holon, HolonKind};
pub use log::{ActionKind, ActionRecord, ImmutableActionLog};
pub use observer::{VizObservation, VizObserver};
pub use renderer::HtmlRenderer;
pub use type_graph::{TypeNode, TypeRelationship, TypeRelationshipGraph, TypeRelationshipKind};

use thiserror::Error;

/// Crate-level error type.
#[derive(Debug, Error)]
pub enum HolonError {
    /// A referenced holon ID was not found in the graph.
    #[error("holon not found: {0}")]
    NotFound(String),

    /// An authorized-by value was empty.
    #[error("authorized_by must not be empty")]
    EmptyAuthorizer,

    /// A step ID was empty.
    #[error("step_id must not be empty")]
    EmptyStepId,

    /// Step already exists in the controller.
    #[error("step already registered: {0}")]
    DuplicateStep(String),

    /// I/O error (file read/write).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON serialization/deserialization failure.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// Render-time failure (e.g. template logic error).
    #[error("render error: {0}")]
    Render(String),
}
