# `ufo-types` / `Requirement` fragmentation across the b00tyverse — scoping

Status: diagnosis complete, no code changed. Surfaced 2026-09-23 while
scoping kr0ki's Flexo baseline adapter (M3 of
`kr0ki/docs/HANDOFF-2026-09-19-reqif-flexo.md`) — building more ReqIF
export logic in kr0ki without first understanding this fragmentation
risked deepening it, so that work paused here instead.

## 1. Framing — why this doc exists

Three independent representations of "a requirement" exist across the
b00tyverse today, and none of them is designated canonical over the
others in writing anywhere:

1. **`PromptExecution/ufo-types`** (standalone repo, published/git-pinned)
   — `ufo_types::mbse::requirements::Requirement` + `RequirementGraph`.
   This is what `kr0ki` depends on (`git` rev pin) for its whole
   requirements-viewpoints/rules-system surface (`RequirementGraph::view`,
   the five viewpoints, `RelationAuthority::{Asserted,Inferred,Proposed}`
   + `promote_relation()`, evidence/provenance, baseline identity). It has
   a `reqif` module (`parse_and_lower`/`bundle_to_requirement_graph`) that
   imports ReqIF XML into this type via the `reqrs` crate — import only,
   no export/writer yet.

2. **`ledgrrr/crates/ufo-types`** (local workspace member, this repo) —
   same crate *name*, a real shared ancestor (`a873b40`, "UFO ontology
   stereotypes, Satisfies trait, and ISO standard types for the
   tax-lawyer platform"), but **independently evolved since** and now
   missing `mbse::requirements` and `reqif` entirely. Pinned to
   `sysml-v2-parser = "=0.54.0"` (kr0ki's copy: `"0.55"`). Last touched
   `c106a5e`. It is not a submodule, not a git subtree, not synced by any
   tooling found in this repo (`.gitmodules` doesn't exist; no
   `check-drift`-style CI step covers this) — it has simply drifted.

3. **`arc-kit-au::Requirement`** (`crates/arc-kit-au/src/node.rs:334`) —
   a third, unrelated shape: `requirement_id, title, rationale, source,
   status, related_decisions: Vec<NodeId>, imported_at`. This is what
   `crates/reqif-mcp-spike` actually converts `reqif-opa-mcp`'s parsed
   `RequirementRecord`s into (`requirement_record_to_node`,
   `reqif-mcp-spike/src/lib.rs:228`) — bypassing *both* `ufo-types`
   copies, even though `ledger-core`/`ledgerr-mcp` already depend on
   this repo's own `crates/ufo-types` for other things.

The result: kr0ki's `ufo_types::mbse::requirements::Requirement` (rich —
baseline identity, provenance, evidence, relation authority/promotion,
five typed viewpoints) and ledgrrr's `arc_kit_au::Requirement` (thin — a
flat record keyed to `NodeId`/`related_decisions`, fitting ledgrrr's own
evidence-chain/decision-ledger shape) cannot exchange a requirement
without a hand-written, lossy translation, and no such translation
exists today either.

## 2. Why this matters now, concretely

kr0ki's Flexo baseline adapter (M3) needs a ReqIF **export** path
(`RequirementGraph` → ReqIF XML) that doesn't exist anywhere yet.
Building it directly in kr0ki (using `reqrs` there too, mirroring the
import side) is architecturally sound *for kr0ki alone*, but it would be
the second independent ReqIF-adjacent Rust implementation in this
ecosystem (this repo's `reqif-mcp-spike` is the first, via MCP+Python)
with no shared contract between them — precisely the anti-pattern this
project's own precedent already warns against. kr0ki's own
`docs/HANDOFF-2026-09-19-reqif-flexo.md` (from the *other* repo) states
the principle plainly: *"`ufo-types` owns shared semantics. [...] Move it
upstream [...] before another consumer depends on this local path."*
`arc-kit-au::Requirement` is exactly "another consumer['s] local path"
that already exists, predating that principle being written down.

## 3. Recommended path (not started — needs a decision, not code, first)

1. **Retire `ledgrrr/crates/ufo-types` as an independent implementation.**
   Depend on the published `PromptExecution/ufo-types` crate (git rev,
   same pattern kr0ki already uses) instead of this repo's own drifted
   copy. This repo's `sysml-derive`/`holon-viz`/`ledger-core`/`ledgerr-mcp`
   call sites need auditing for anything that used a field/module only
   the local fork had — the `mbse::requirements`/`reqif` modules are pure
   additions from the published crate's side, so this is likely additive
   for most callers, but `sysml-v2-parser` version drift (0.54.0 → 0.55)
   needs its own compatibility check first.
2. **Retype `arc-kit-au::Requirement` onto `ufo_types::mbse::
   requirements::Requirement`**, or make it wrap/reference one (TBD which
   — `arc-kit-au`'s own `NodeId`/`related_decisions`/evidence-chain
   machinery may still need a thin ledgrrr-local envelope around the
   shared type, the same way kr0ki's own `EvidenceRef`/`Provenance`
   already carry model identity without owning the semantic contract).
   `reqif-mcp-spike`'s `requirement_record_to_node` becomes the one
   conversion point that changes.
3. **Only then** does it make sense to ask whether kr0ki's forthcoming
   ReqIF *export* path and this repo's `reqif-mcp-spike` *import* path
   should become one shared adapter crate, or whether they legitimately
   stay separate (kr0ki: direct Rust `reqrs` dependency, no sidecar;
   ledgrrr: MCP-wrapped `reqif-opa-mcp`, decided 2026-08-22 for reasons
   specific to this repo's governance/evidence-chain model) — that
   question only has a clean answer once both sides speak the same
   `Requirement` type.

## 3a. Hard requirement: `ledgrrr://` attribution (approval-blocking)

**Added 2026-09-23, user-mandated — the unification path in §3 is not
approved without this.** Any unified `Requirement` (and by extension
`Decision`/`Cost`/every other `arc-kit-au::node` type) must be
attributable back to ledgrrr via a `ledgrrr://` URI acting as a
**meta-dataframe interface** — a locator into ledgrrr's own graph/
evidence-chain representation, not a file path or HTTP endpoint. This is
a precondition for the recommendation in §3 (kr0ki, or any other
consumer, adopting the shared `ufo_types::mbse::requirements::Requirement`
type), not an independent nice-to-have layered on top of it.

**No schema change is needed to carry this** — `ufo_types::mbse::
requirements::Provenance.source_uri: String` and `EvidenceRef.uri:
Option<String>` are already free-form URI slots (kr0ki's `reqif_import`
already populates an analogous `urn:sha256:` locator when no better
source URI exists, so provenance values in this ecosystem already flow
through a plain string URI field — the gap is a *convention*, not a
missing field). What's missing is the scheme's own definition and a
mandate to actually use it for anything ledgrrr-sourced.

**Proposed scheme** (needs this repo's own sign-off, not decided here):
`arc-kit-au::NodeId` already stringifies as `"{prefix}:{content_hash}"`
(`NodeId::new`, `crates/arc-kit-au/src/node.rs:28`) — e.g. `req:a1b2c3…`
for a `Requirement`. The natural, zero-new-machinery mapping is
`ledgrrr://<NodeId>` (e.g. `ledgrrr://req:a1b2c3…`), so any node this
repo already content-hashes is trivially also a resolvable
`ledgrrr://` URI with no new identity scheme invented. What resolving
that URI actually *does* (a local MCP tool call, a `ledgerr-mcp` HTTP
route, purely an opaque-but-stable identifier with no live resolver yet)
is this repo's decision, not kr0ki's or this doc's to make — flagging it
as the concrete open question a reviewer of this PR needs to answer
before §3's retyping work starts.

## 4. Explicit non-recommendation

This doc does **not** recommend ledgrrr adopt kr0ki's `reqrs`-direct
approach over its own MCP-wrapped `reqif-opa-mcp` approach (§5 Q6 in
`docs/systems-modeling-registry-rescope.md` already reasoned through that
choice for this repo's specific constraints, and nothing here
contradicts that reasoning). The fragmentation this doc is about is the
**target type** (`Requirement`'s shape), not the **parsing mechanism**
(direct Rust dependency vs. MCP-wrapped sidecar) — those are separable
decisions, and only the first one is a live footgun as written today.

## 5. Open questions (for whoever picks this up)

- Does `PromptExecution/ufo-types` need a version bump/release tag before
  this repo depends on it externally, or is a `rev`-pinned git dependency
  (kr0ki's own pattern) acceptable here too?
- `arc-kit-au`'s `#[derive(SysmlBlock)]` retrofit
  ([`ledgrrr#193`](https://github.com/PromptExecution/ledgrrr/pull/193),
  draft per `docs/systems-modeling-registry-rescope.md`'s own status
  line) touches the same struct this doc wants retyped — check whether
  that PR should land first, after, or get folded into this work.
- Is there a reason `ledgrrr/crates/ufo-types` was allowed to diverge
  this far without anyone noticing — should a `check-drift`-style CI step
  (already used elsewhere in this repo per `ledgrrr#194`) cover crate
  parity against the published `ufo-types` repo too?
- **§3a's resolver semantics**: what does dereferencing a `ledgrrr://`
  URI actually do at runtime, if anything, in v1? Does it need a resolver
  at all before this scheme can be considered "defined," or is an
  opaque-but-stable identifier (no live resolution) sufficient for the
  attribution requirement to be satisfied?
