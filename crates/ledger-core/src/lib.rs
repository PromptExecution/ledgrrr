pub mod attest;
pub mod calendar;
pub mod classify;
pub mod constraints;
pub mod document;
pub mod document_shape;
pub mod filename;
pub mod fs_meta;
pub mod graph;
pub mod ingest;
pub mod iso;
pub mod iso_objects;
pub mod journal;
pub mod layout;
pub mod ledger_ops;
pub mod legal;
pub mod manifest;
pub mod observability;
pub mod ontology;
pub mod pipeline;
pub mod proposal;
pub mod render;
pub mod rule_registry;
pub mod slint_viz;
pub mod tags;
pub mod validation;
pub mod verify;
pub mod visualize;
pub mod watcher;
pub mod workbook;
pub mod workflow;

pub use graph::{create_pipeline_edges, create_pipeline_nodes, EdgeData, NodeData};
pub use layout::{iso_project, ForceLayout};
pub use render::GraphRenderer;
pub use slint_viz::SlintGraphView;

#[cfg(test)]
mod integration_tests;
