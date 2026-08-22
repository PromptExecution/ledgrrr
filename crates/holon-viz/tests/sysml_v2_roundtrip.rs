//! Round-trips `SysmlV2Emitter::emit()`'s output through the real
//! `sysml-v2-parser` crate (wired in via `ufo_types::sysml`).
//!
//! `docs/sysml-v2-parser-spike.md` found `SysmlV2Emitter`'s output didn't
//! parse as SysML v2 at all: a `//` comment swallowed the closing `}`, and
//! it emitted SysML v1's `block def` instead of SysML v2's `part def`. Both
//! are fixed in `src/emitter.rs`. This test is the concrete "did we close
//! the round-trip" signal the spike asked for: it fails loudly if either
//! regresses, instead of only being checked by eyeballing the output text.

use holon_viz::{CytoscapeGraph, Holon, HolonKind, SysmlV2Emitter};
use ufo_types::sysml::validate_sysml_v2;

#[test]
fn emitted_output_is_valid_sysml_v2_for_a_single_node() {
    let h = Holon::root("alpha-id", "Alpha", HolonKind::SysmlBlock);
    let g = CytoscapeGraph::from_holons(&[h]);
    let emitted = SysmlV2Emitter::emit(&g);

    let result = validate_sysml_v2(&emitted);
    assert!(
        result.disposition.is_satisfied(),
        "SysmlV2Emitter output failed to parse as SysML v2: {:?}\n---\n{emitted}",
        result.disposition
    );
}

#[test]
fn emitted_output_is_valid_sysml_v2_for_a_holarchy_with_containment_edges() {
    let holons = vec![
        Holon::root("pipeline", "Tax Ledger Pipeline", HolonKind::CapsuleGroup),
        Holon::child("ingest", "Ingest PDFs", HolonKind::SysmlBlock, "pipeline"),
        Holon::child(
            "classify",
            "Classify Transactions",
            HolonKind::SysmlBlock,
            "pipeline",
        ),
    ];
    let g = CytoscapeGraph::from_holons(&holons);
    let emitted = SysmlV2Emitter::emit(&g);

    let result = validate_sysml_v2(&emitted);
    assert!(
        result.disposition.is_satisfied(),
        "SysmlV2Emitter output failed to parse as SysML v2: {:?}\n---\n{emitted}",
        result.disposition
    );
}

#[test]
fn empty_graph_emits_valid_sysml_v2() {
    let g = CytoscapeGraph::from_holons(&[]);
    let emitted = SysmlV2Emitter::emit(&g);
    let result = validate_sysml_v2(&emitted);
    assert!(result.disposition.is_satisfied(), "{:?}", result.disposition);
}
