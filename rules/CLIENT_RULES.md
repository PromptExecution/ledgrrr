# Client Rule Overlays

How to plug operator- or client-specific classification rules (vendor names,
account patterns, client-owned heuristics) into `RuleRegistry` **without**
committing them to this repo.

## Why this exists

The tracked rules in this directory (see `ONTOLOGY.md`) are deliberately
generic + jurisdiction-level (US federal, AU). They must stay client-agnostic
so this repo is reusable across any operator's books. Real client vendor
lists (ML/compute providers, hosting, app-store fees, hardware suppliers,
whatever a given client's spend actually looks like) belong to that client,
not to `ledgrrr` — and guessing at them here would just be hallucinated data
masquerading as a real classification rule.

## Where to put client rules

Drop `.rhai` files in:

```
rules/client/
```

This directory is gitignored (`rules/client/` in `.gitignore`) — anything you
put there stays local to your machine and is never tracked or pushed.

`RuleRegistry::load_from_dir(rules_dir)` scans `rules_dir` for `.rhai` files
as before, and additionally scans `<rules_dir>/client/` if it exists. A
missing `client/` directory is the normal, unconfigured state — no error, no
fallback rules are affected. There is no separate API to call: dropping a
file in `rules/client/` and re-running (or letting the file watcher reload)
is the entire mechanism.

## Rule file contract

Client rules must follow the exact same `fn classify(tx)` contract as every
other rule in this directory — see `ONTOLOGY.md`'s Notes section and read an
existing rule (e.g. `classify_schedule_c.rhai` or `classify_foreign_income.rhai`)
as a worked example. Summary:

- Entry point: `fn classify(tx)`, where `tx` is a map with
  `tx_id`, `account_id`, `date`, `amount`, `description`.
- `amount` is a **string** decimal — use `parse_float()` only for threshold
  comparisons, never store or return money as a float.
- Return a map: `#{ category, confidence, review, reason }`.
- Return `category: "Unclassified"`, `confidence: 0.0` when the rule's
  signal doesn't match, so the waterfall continues to the next rule.
- Guard every `tx[...]` field access with `tx.contains("field")` first —
  missing keys are not an error in Rhai, but relying on that without a guard
  produces confusing downstream behavior.

## Fallback behavior

`classify_fallback.rhai` (tracked, in this directory) is the catch-all: it
unconditionally matches with `category: "Unclassified"`, `confidence: 0.0`,
`review: true`. Any transaction that no rule — tracked or client overlay —
positively classifies ends up `category: "Unclassified"`. Client rules do
not need their own fallback; they only need to return `Unclassified` when
they don't apply, per the contract above.

Caveat: `classify_waterfall` selects candidate rules via
`select_rules_semantic` (lexical-similarity ranking), which does **not**
guarantee `classify_fallback.rhai` is the literal last rule evaluated the
way `select_rules_deterministic` does (that keyword-match path always
appends fallback rules last). When multiple rules return `Unclassified` for
the same transaction, the `review`/`reason` fields on the final outcome come
from whichever conforming rule was scored last, not necessarily
`classify_fallback.rhai` itself. The `category: "Unclassified"` contract is
reliable; treat `review`/`reason` on an unclassified outcome as informational,
not load-bearing.

## Selection order

`RuleRegistry::classify_waterfall` selects candidate rules by lexical
similarity to the transaction (`select_rules_semantic`, falling back to
keyword matching), not by directory. A client rule with a strong vendor-name
match will be tried ahead of a generic rule with a weak match, and vice
versa — there is no need to manually order `rules/client/` relative to the
tracked rules.
