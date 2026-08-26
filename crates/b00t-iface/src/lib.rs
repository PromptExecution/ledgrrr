//! b00t interface library — solver-verifiable process lifecycle for the b00t ecosystem.
//!
//! # Architecture
//!
//! ```text
//! core/                 ← abstract syntax: solver-verifiable primitives
//!   surface.rs          ← ProcessSurface trait + DatumWatcher impl
//!   governance.rs       ← GovernancePolicy, AgentRole, constraints
//!   promise.rs          ← LifecyclePromise, PromiseChain (event-driven)
//!   machine.rs          ← SurfaceMachine (state machine over surface states)
//! exec/                 ← executive subsystem: promise-event-driven harness
//!   harness.rs          ← SurfaceHarness: wraps surface + governance + machine
//! gated/                ← cfg-gated surfaces for upstreaming (feature = "b00t")
//!   opencode_provider.rs  ← opencode LLM provider as a b00t surface
//!   autoresearch.rs       ← karpathy/autoresearch loop as a b00t surface
//! ```
//!
//! # Feature gates
//! - `b00t`: enables cfg-gated types designed for upstreaming to b00t repo
//! - `autoresearch`: adds reqwest for remote eval dispatch

pub mod core;
pub mod docling;
pub mod exec;
pub mod metric;
pub mod sarif;
pub mod viz;

#[cfg(feature = "b00t")]
pub mod ralph;

#[cfg(feature = "b00t")]
pub mod gated;

#[cfg(feature = "autoresearch")]
pub mod llm;

#[cfg(feature = "b00t")]
pub mod handshake;

pub use core::*;
