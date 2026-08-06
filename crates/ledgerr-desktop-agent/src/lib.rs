//! `ledgerr-desktop-agent` — the Claude Desktop MCPB controller crate.
//!
//! Implements the `ledgrrr_*` tool surface from PRD-11 §3.1. This crate is
//! deliberately thin: it inspects local state and delegates real work
//! (diagram rendering, deterministic simulation) to typed modules here, but
//! it is not itself the privileged installer — see [`install_plan`] and
//! PRD-11 §7/§10.

pub mod contract;
pub mod install_plan;
pub mod office_artifact;
pub mod playbook;
pub mod render;
pub mod runtime_client;
pub mod service_control;
pub mod settings_server;
pub mod simulate;
pub mod state;
pub mod status;
