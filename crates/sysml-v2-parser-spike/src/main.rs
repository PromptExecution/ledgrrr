//! Spike: does `sysml-v2-parser` (crates.io) round-trip against
//! `holon-viz`'s `SysmlV2Emitter` output, and does it hold up against real
//! OMG SysML-v2 sample corpora?
//!
//! This is a throwaway investigation binary, not production code. Run with:
//!   cargo run -p sysml-v2-parser-spike
//!
//! See docs/sysml-v2-parser-spike.md (in the ledgrrr repo root) for the
//! write-up of findings.

use holon_viz::{CytoscapeGraph, Holon, HolonKind, SysmlV2Emitter};
use sysml_v2_parser::ast::{PackageBodyElement, RootElement};
use sysml_v2_parser::{parse, parse_for_editor};
use std::collections::HashMap;

fn section(title: &str) {
    println!("\n=== {title} ===");
}

/// Small holarchy mirroring holon-viz's own demo (bin/demo.rs), kept
/// independent here since sample_tax_pipeline() there is a private fn.
fn sample_holons() -> Vec<Holon> {
    vec![
        Holon::root("pipeline", "Tax Ledger Pipeline", HolonKind::CapsuleGroup),
        Holon::child(
            "ingest",
            "Ingest PDFs",
            HolonKind::SysmlBlock,
            "pipeline",
        ),
        Holon::child(
            "classify",
            "Classify Transactions",
            HolonKind::SysmlBlock,
            "pipeline",
        ),
    ]
}

fn try_parse_strict(label: &str, src: &str) {
    match parse(src) {
        Ok(root) => {
            println!("[{label}] parse() OK — {} root elements", root.elements.len());
        }
        Err(e) => {
            println!("[{label}] parse() FAILED: {e}");
        }
    }
}

fn try_parse_editor(label: &str, src: &str) {
    let result = parse_for_editor(src);
    println!(
        "[{label}] parse_for_editor(): {} root elements, {} diagnostics, is_ok={}",
        result.root.elements.len(),
        result.errors.len(),
        result.is_ok()
    );
    for err in result.errors.iter().take(5) {
        println!(
            "    diag: line={:?} col={:?} msg={}",
            err.line, err.column, err.message
        );
    }

    // Summarize what the (possibly partial) AST actually contains, by kind.
    let mut counts: HashMap<&'static str, usize> = HashMap::new();
    for node in &result.root.elements {
        let kind = match &node.value {
            RootElement::Package(_) => "Package",
            RootElement::LibraryPackage(_) => "LibraryPackage",
            RootElement::Namespace(_) => "Namespace",
            RootElement::Import(_) => "Import",
            RootElement::Member(m) => match &m.value {
                PackageBodyElement::PartDef(_) => "Member::PartDef",
                PackageBodyElement::PartUsage(_) => "Member::PartUsage",
                PackageBodyElement::AttributeDef(_) => "Member::AttributeDef",
                PackageBodyElement::Error(_) => "Member::Error(ParseErrorNode)",
                _ => "Member::Other",
            },
        };
        *counts.entry(kind).or_insert(0) += 1;
    }
    for (k, v) in counts {
        println!("    ast element: {k} x{v}");
    }
}

fn main() {
    section("1. holon-viz SysmlV2Emitter output");
    let holons = sample_holons();
    let graph = CytoscapeGraph::from_holons(&holons);
    let emitted = SysmlV2Emitter::emit(&graph);
    println!("{emitted}");

    try_parse_strict("holon-viz emitted", &emitted);
    try_parse_editor("holon-viz emitted", &emitted);

    section("2. OMG sample: RootPackageTest.sysml");
    let root_pkg_test = include_str!("../samples/RootPackageTest.sysml");
    try_parse_strict("RootPackageTest.sysml", root_pkg_test);
    try_parse_editor("RootPackageTest.sysml", root_pkg_test);

    section("3. OMG sample: PartTest.sysml");
    let part_test = include_str!("../samples/PartTest.sysml");
    try_parse_strict("PartTest.sysml", part_test);
    try_parse_editor("PartTest.sysml", part_test);

    section("4. Don't Panic Batmobile teaching example");
    let batmobile = include_str!("../samples/DontPanic-Batmobile.sysml");
    try_parse_strict("DontPanic-Batmobile.sysml", batmobile);
    try_parse_editor("DontPanic-Batmobile.sysml", batmobile);

    section("5. Isolating holon-viz's bug: is it the '//'-comment-eats-'}' or the 'block def' keyword?");
    // holon-viz's emitter puts the node's closing brace on the SAME line as a
    // `//` line comment, so the comment swallows it — the block body is never
    // actually closed. Test with the trailing brace moved off the comment
    // line, keeping `block def` (not valid SysML-v2; SysML-v1 called it
    // Block, SysML-v2 calls it `part def`) to see whether that's *also*
    // rejected on its own.
    let fixed_braces = r#"package HolonModel {
    block def Tax_Ledger_Pipeline { // id: pipeline, kind: CapsuleGroup
    }
    block def Ingest_PDFs { // id: ingest, kind: SysmlBlock
    }
}
"#;
    try_parse_strict("brace-fixed, still 'block def'", fixed_braces);
    try_parse_editor("brace-fixed, still 'block def'", fixed_braces);

    let part_def_equivalent = r#"package HolonModel {
    part def Tax_Ledger_Pipeline { // id: pipeline, kind: CapsuleGroup
    }
    part def Ingest_PDFs { // id: ingest, kind: SysmlBlock
    }
}
"#;
    try_parse_strict("brace-fixed + 'part def'", part_def_equivalent);
    try_parse_editor("brace-fixed + 'part def'", part_def_equivalent);

    section("6. Deliberately malformed input (resilient-editor-mode check)");
    let malformed = r#"
package Broken {
    part def Foo {
        attribute x
        part def
    }
    part def Bar {
"#; // missing closing braces + a dangling "part def" with no identifier
    try_parse_strict("malformed", malformed);
    try_parse_editor("malformed", malformed);

    // Total garbage / binary-ish input and empty input — checking for panics,
    // not correctness of output.
    let garbage = "\u{0}\u{1}\u{FFFD} {{{ %%% not sysml at all )))";
    try_parse_strict("garbage bytes", garbage);
    try_parse_editor("garbage bytes", garbage);

    try_parse_strict("empty input", "");
    try_parse_editor("empty input", "");

    println!("\nNo panics across any of the above inputs — process reached the end normally.");
}
