//! Integration tests pinning down the spike's key findings so regressions in
//! either `sysml-v2-parser` (crate bump) or `holon-viz`'s emitter are caught.
//!
//! See docs/sysml-v2-parser-spike.md for the narrative write-up.

use holon_viz::{CytoscapeGraph, Holon, HolonKind, SysmlV2Emitter};
use sysml_v2_parser::{parse, parse_for_editor};

/// holon-viz's `SysmlV2Emitter` output is NOT valid SysML v2 today, for two
/// independent reasons: (1) it emits `block def` (SysML v1 "Block"
/// terminology; SysML v2 uses `part def`), and (2) it places the closing `}`
/// of each node's body on the same line as a `//` comment, which swallows
/// the brace as commented-out text and leaves the body syntactically
/// unclosed. This test documents the current (broken) state — flip it when
/// either bug is fixed upstream in holon-viz.
#[test]
fn holon_viz_emitter_output_does_not_parse_yet() {
    let holons = vec![Holon::root("pipeline", "Tax Ledger Pipeline", HolonKind::CapsuleGroup)];
    let graph = CytoscapeGraph::from_holons(&holons);
    let emitted = SysmlV2Emitter::emit(&graph);

    assert!(
        parse(&emitted).is_err(),
        "holon-viz emitter output unexpectedly became valid SysML v2 — \
         if this now passes, the brace/keyword bugs described in \
         docs/sysml-v2-parser-spike.md were fixed; update this test."
    );
}

/// The canonical OMG minimal example (package + part def + private import)
/// parses cleanly with zero diagnostics — confirms the crate handles basic
/// SysML v2 textual syntax correctly, not just that it compiles.
#[test]
fn omg_root_package_test_parses_cleanly() {
    let src = include_str!("../samples/RootPackageTest.sysml");
    let result = parse_for_editor(src);
    assert!(result.is_ok(), "expected zero diagnostics, got: {:?}", result.errors);
    assert_eq!(result.root.elements.len(), 4);
}

/// `parse_for_editor` must never panic and must always return a (possibly
/// empty/partial) AST, even for syntactically hopeless input. This is the
/// "resilient editor mode" the crate advertises.
#[test]
fn parse_for_editor_never_panics_on_bad_input() {
    let cases = [
        "",
        "\u{0}\u{1}\u{FFFD} {{{ %%% not sysml at all )))",
        "package Broken {\n    part def Foo {\n        part def\n",
    ];
    for src in cases {
        let result = parse_for_editor(src);
        // Reaching this line at all (no panic/abort) is the assertion.
        let _ = (result.root.elements.len(), result.errors.len());
    }
}
