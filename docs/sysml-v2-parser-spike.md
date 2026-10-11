# Spike: `sysml-v2-parser` round-trip against `holon-viz`'s `SysmlV2Emitter`

Status: spike complete. This doc is the only artifact ported to `main` (PR
#187 was closed as superseded); the companion crate
(`crates/sysml-v2-parser-spike`, a throwaway investigation binary +
integration tests) was left behind as disposable per its own scope. The
`holon-viz` emitter bugs this spike found (see Finding 1 below) were fixed
independently, on `main`, in PR #183 (`c106a5e`) before this doc itself
landed — `crates/holon-viz/src/emitter.rs` already emits `part def` with the
comment on its own line.

Context: `docs/sysml-v2-tooling-survey.md` (branch `docs/sysml-v2-tooling-survey`,
PR #180) flagged `sysml-v2-parser` as the highest-priority Tier-0 candidate to
close the SysML-v2 round-trip (parse → model → `holon-viz` → text/OWL2)
without writing a parser in-house. This spike actually runs it against real
text, per the project's verify-before-writing-datums rule, rather than trusting
the crate's own README claims.

## Crate health

- **crates.io**: `sysml-v2-parser` v0.54.0, published 2026-08-07. 56 versions
  published since the crate's creation on 2026-04-09 — i.e. roughly one
  release every 2-3 days for ~4.5 months. That's a high release cadence for a
  pre-1.0 (0.x) crate: either very active development, or a lot of API/behavior
  churn to absorb if we depend on it. MIT licensed, `serde` feature flag
  available.
- **GitHub** (`elan8/sysml-v2-parser`): created 2026-03-11, last push
  2026-08-08 (two weeks before this spike) — actively maintained, not
  abandoned. Only 4 stars, 2 forks — very small adoption footprint. 21 open
  issues, and a meaningful fraction of them are exactly the kind of thing this
  spike also hit independently: real parsing gaps for legal SysML-v2
  constructs (`package` nested inside `part def` body fails to parse, `ref`
  declarations not dispatched in port bodies, anonymous `constraint { }`
  usage fails, multiplicity silently discarded at 9+ call sites, etc.). This
  is a young, immature, single/small-team-maintained parser, not a mature
  reference implementation.
- **API surface**: exports `parse(&str) -> Result<RootNamespace, ParseError>`
  (strict, fails fast) and `parse_for_editor(&str) -> ParseResult` /
  `parser::parse_with_diagnostics(&str) -> ParseResult` (resilient: partial
  AST + `Vec<ParseError>`, `is_ok()` helper, never fails). There's also an
  `emit` module (`emit_sysml`, `emit_sysml_with_options`, `opacity_report`)
  for AST → text, which we did not exercise here (out of scope: this spike is
  about parsing `holon-viz`'s emitted text, not re-emitting through this
  crate).

**Verdict on health alone: usable for prototyping, not yet a dependency to
build load-bearing tooling on.** Track upstream issue resolution before
committing further.

## What we actually ran

`crates/sysml-v2-parser-spike/src/main.rs` feeds six inputs through both
`parse()` and `parse_for_editor()` and inspects the resulting AST /
diagnostics (not just "did it compile"):

1. `holon-viz`'s own `SysmlV2Emitter::emit()` output, from a small hand-built
   `Holon`/`CytoscapeGraph` (same shape as `crates/holon-viz/src/bin/demo.rs`'s
   `sample_tax_pipeline`, reconstructed here since that helper is private to
   the demo binary).
2. OMG `SysML-v2-Release` sample `sysml/src/examples/Simple Tests/RootPackageTest.sysml`
   (13 lines — the smallest file in the corpus: nested packages, a `part def`,
   a `private import`, and a `subsets` usage).
3. OMG sample `PartTest.sysml` (more constructs: ports, redefinition, aliasing,
   nested `action`/`state`, individuals).
4. The "Don't Panic" Batmobile teaching example
   (`MBSE4U/dont-panic-batmobile`, 293 lines — full vehicle/engine/wheel model
   with variation points and constraints).
5. Deliberately malformed input (unclosed braces, a dangling `part def` with
   no identifier).
6. Garbage bytes (`\0`, `\u{FFFD}`, stray punctuation) and empty input.

Ran with `cargo run -p sysml-v2-parser-spike`; pinned the load-bearing
findings as `cargo test -p sysml-v2-parser-spike` assertions in
`tests/roundtrip.rs` (3 tests, all green).

## Findings

### 1. `holon-viz`'s emitted SysML-v2 text does not parse — and it's two independent bugs, not one

`SysmlV2Emitter::emit()` currently produces:

```
package HolonModel {
    block def Tax_Ledger_Pipeline { // id: pipeline, kind: CapsuleGroup }
    block def Ingest_PDFs { // id: ingest, kind: SysmlBlock }
    ...
}
```

Feeding this straight into `parse()` fails with `expected '}' ... missing
closing '}'`. Isolating the two things wrong with it (see
`crates/sysml-v2-parser-spike/src/main.rs`, section 5):

- **Bug A — the closing `}` is inside a `//` line comment.** The emitter
  writes the node's closing brace on the *same line* as the trailing
  `// id: ..., kind: ...` comment. Since `//` comments run to end of line,
  that `}` is commented out — the block body is never syntactically closed at
  all. This is a real bug in `crates/holon-viz/src/emitter.rs`'s
  `SysmlV2Emitter`, independent of anything about the parser crate. Moving the
  comment before the newline (or dropping it) is enough to fix it, confirmed
  by feeding the brace-corrected text through the parser: it then fails for a
  *different* reason (Bug B below), so the brace bug alone was masking Bug B.
- **Bug B — `block def` is not SysML v2 syntax.** Once the brace bug is
  worked around, the parser rejects `block def` outright:
  `` `block` is not a SysML keyword; check for a typo or unsupported
  construct, or remove it `` (from `parse_for_editor`'s diagnostic). SysML v1
  had a `Block` stereotype; SysML v2 renamed the equivalent concept to
  `part def`. Confirmed by substituting `part def` for `block def` in the
  otherwise-identical text: it then parses with zero errors. `holon-viz`'s
  emitter is emitting SysML v1 terminology through a "v2" emitter — this is a
  real correctness bug in `holon-viz`, not a parser limitation.

**This means "closing the round-trip" is not just wiring the two crates
together — `holon-viz`'s emitter needs at least two fixes first**
(brace-vs-comment placement, and `block def` → `part def`) before its output
is even syntactically valid SysML v2, let alone semantically round-trippable.
This spike itself only investigates (fixing was out of scope for it), but
both fixes described above landed on `main` in PR #183 (`c106a5e`), ahead of
this doc — `crates/holon-viz/src/emitter.rs` now emits `part def` with the
comment on its own line, closing the bracket correctly.

### 2. The parser handles real, spec-legal SysML v2 correctly on simple input

The smallest official OMG sample (`RootPackageTest.sysml` — 3 packages, an
import, a `part def`, a `subsets` usage) parses with **zero diagnostics** via
both `parse()` and `parse_for_editor()`, and produces the AST shape you'd
expect (4 root elements: 1 `Import` + 3 `Package`). This is real evidence the
crate isn't just accepting anything — it's doing genuine grammar-level
validation and getting the basics right.

### 3. More complex real-world SysML v2 hits genuine parser gaps

Both `PartTest.sysml` (OMG's own port/redefinition/alias/action stress test)
and the Batmobile example (variation points, constraint blocks, item flows)
produced multiple diagnostics — `derived` rejected in a part-definition body,
`package` rejected nested inside a `part def` body, `alias` rejected inside a
port-definition body, comma-separated sequence assignment
(`:>> x := "a", "b"`) misparsed, `item` rejected inside an occurrence body.
These line up almost one-to-one with the crate's own open GitHub issues
(#116 `package` nested in `part def` fails; #111 anonymous `constraint { }`
fails; #106 multiplicity silently discarded, etc.) — independent confirmation
the issue tracker accurately reflects real, reproducible gaps rather than
theoretical ones.

### 4. Resilient editor mode genuinely degrades gracefully — no panics, anywhere

`parse_for_editor()` (and the internally-equivalent
`parser::parse_with_diagnostics()`) never panicked or aborted across any
input tried: well-formed OMG samples, `holon-viz`'s broken output, hand-built
malformed SysML (unclosed braces, dangling incomplete declarations), raw
garbage bytes (`\0`, `\u{FFFD}`, stray punctuation), and empty input. Every
case returned a `ParseResult { root, errors }` with a partial (possibly empty)
AST plus structured diagnostics carrying line/column/message and often a
concrete suggested fix (e.g. `` Replace `derived` with a valid part definition
body member or remove it ``). This is exactly the "resilient editor mode"
selling point the tooling survey flagged as valuable, and it held up under
adversarial input, not just the happy path.

## Recommendation: **adopt-with-caveats**

- The parser itself is competently built — real grammar coverage on
  spec-legal input, genuinely graceful degradation on bad input, and
  actionable diagnostics. It is worth continuing to build against.
- It is **not** yet mature enough to be a silent, load-bearing dependency:
  0.x versioning, ~5 months old, tiny adoption (4 GitHub stars), 21 open
  issues including real parsing gaps for legal SysML v2 constructs. Expect to
  file upstream issues and possibly patch around gaps for any construct
  beyond basic packages/parts/imports.
- **Closing the actual round-trip required fixing `holon-viz`'s
  `SysmlV2Emitter` first** — it did not emit syntactically valid SysML v2
  (the `block def` / SysML-v1-terminology bug, plus the
  comment-swallows-brace bug). **Done:** both fixes landed on `main` in PR
  #183 (`c106a5e`).
- Remaining next steps if `sysml-v2-parser` adoption continues: (1) pin it to
  an exact version (not a caret range) given its release cadence, and (2)
  budget for upstream issue triage/patches if we lean on constructs beyond
  basic package/part/import (see Finding 3).

## Artifacts

The investigation crate below was **not** ported to `main` — it was
throwaway by design, and its one load-bearing regression test asserted the
pre-fix broken behavior, which is now stale since the fixes landed. It
remains on the original spike branch (`spike/sysml-v2-parser-roundtrip`,
closed PR #187) for reference if this investigation needs re-running:

- `crates/sysml-v2-parser-spike/src/main.rs` — the investigation binary,
  runnable via `cargo run -p sysml-v2-parser-spike`.
- `crates/sysml-v2-parser-spike/tests/roundtrip.rs` — 3 pinned regression
  tests (`cargo test -p sysml-v2-parser-spike`), one of which
  (`holon_viz_emitter_output_does_not_parse_yet`) now fails given the fixes
  above — that failure is the "did we close the round-trip" signal the
  spike was designed to produce.
- `crates/sysml-v2-parser-spike/samples/` — the fetched OMG
  (`RootPackageTest.sysml`, `PartTest.sysml`) and Batmobile
  (`DontPanic-Batmobile.sysml`) sample files, vendored so the spike doesn't
  depend on network access to re-run.
