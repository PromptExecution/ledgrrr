//! Proves the `<rules_dir>/client/` overlay extension point (issue #113):
//! a `.rhai` rule file dropped into an untracked `client/` subdirectory is
//! discovered by `RuleRegistry::load_from_dir` alongside the tracked rules,
//! and participates in the classification waterfall exactly like a tracked
//! rule. See `rules/CLIENT_RULES.md` for the operator-facing contract this
//! test exercises.
//!
//! No app4dog-specific (or any other real client's) vendor names, rules, or
//! logic are used here — the fixture below is a synthetic stand-in for
//! "some operator's untracked rule file", not a guess at real client data.

use std::fs;
use std::path::{Path, PathBuf};

use ledger_core::classify::{ClassificationEngine, SampleTransaction};
use ledger_core::rule_registry::RuleRegistry;
use serde::Deserialize;

/// Mirrors the JSON fixture shape. `SampleTransaction` itself does not
/// derive `Deserialize`, so this local shim maps fixture fields onto it —
/// consistent with the mirror-type pattern already used for `ReqIfCandidate`
/// in `rule_registry.rs`.
#[derive(Debug, Deserialize)]
struct FixtureTransaction {
    tx_id: String,
    account_id: String,
    date: String,
    amount: String,
    description: String,
}

impl From<FixtureTransaction> for SampleTransaction {
    fn from(f: FixtureTransaction) -> Self {
        SampleTransaction {
            tx_id: f.tx_id,
            account_id: f.account_id,
            date: f.date,
            amount: f.amount,
            description: f.description,
        }
    }
}

fn fixture_tx() -> SampleTransaction {
    let manifest =
        std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set by cargo test");
    let path = Path::new(&manifest).join("tests/fixtures/client_rule_overlay_tx.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read fixture {}: {e}", path.display()));
    let fixture: FixtureTransaction = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("failed to parse fixture {}: {e}", path.display()));
    fixture.into()
}

const CLIENT_RULE_BODY: &str = r#"
// Synthetic client-overlay rule used only by the extension-point test.
// Real client rule files (never committed to this repo) follow the same
// fn classify(tx) contract documented in rules/CLIENT_RULES.md.
fn classify(tx) {
    let description = "";
    if tx.contains("description") {
        description = tx["description"];
    }

    if description.contains("Acme Test Vendor") {
        return #{
            category:   "ClientVendorExpense",
            confidence: 0.95,
            review:     false,
            reason:     "matched synthetic client overlay vendor rule"
        };
    }

    #{
        category:   "Unclassified",
        confidence: 0.0,
        review:     false,
        reason:     "no client overlay signal"
    }
}
"#;

const FALLBACK_RULE_BODY: &str = r#"
fn classify(tx) {
    #{
        category:   "Unclassified",
        confidence: 0.0,
        review:     true,
        reason:     "no rule matched"
    }
}
"#;

/// Builds a temp rules directory laid out like the real `rules/` tree:
/// a tracked-style fallback rule at the top level, plus an untracked
/// `client/` subdirectory holding the overlay rule.
fn build_overlay_rules_dir() -> tempfile::TempDir {
    let dir = tempfile::TempDir::new().expect("create temp rules dir");

    fs::write(dir.path().join("classify_fallback.rhai"), FALLBACK_RULE_BODY)
        .expect("write fallback rule");

    let client_dir = dir.path().join("client");
    fs::create_dir(&client_dir).expect("create client overlay dir");
    fs::write(
        client_dir.join("classify_client_vendor.rhai"),
        CLIENT_RULE_BODY,
    )
    .expect("write client overlay rule");

    dir
}

#[test]
fn load_from_dir_discovers_client_overlay_rule() {
    let dir = build_overlay_rules_dir();
    let registry = RuleRegistry::load_from_dir(dir.path()).expect("rules directory loads");

    assert_eq!(
        registry.rule_count(),
        2,
        "expected the tracked fallback rule plus the client overlay rule"
    );

    let has_overlay_rule = registry.rule_paths().iter().any(|p: &PathBuf| {
        p.parent().and_then(|parent| parent.file_name()) == Some(std::ffi::OsStr::new("client"))
            && p.file_name() == Some(std::ffi::OsStr::new("classify_client_vendor.rhai"))
    });
    assert!(
        has_overlay_rule,
        "client overlay rule must be discovered from <rules_dir>/client/"
    );
}

#[test]
fn client_overlay_rule_classifies_matching_transaction() {
    let dir = build_overlay_rules_dir();
    let registry = RuleRegistry::load_from_dir(dir.path()).expect("rules directory loads");
    let mut engine = ClassificationEngine::default();

    let tx = fixture_tx();
    let outcome = registry
        .classify_waterfall(&mut engine, &tx)
        .expect("waterfall classifies");

    assert_eq!(outcome.category, "ClientVendorExpense");
    assert!(outcome.confidence > 0.0);
}

#[test]
fn unmatched_transaction_still_falls_through_to_unclassified_with_overlay_present() {
    // Confirms the client overlay doesn't break the existing fallback
    // contract: a transaction no rule (tracked or overlay) matches still
    // ends up `Unclassified`, per `classify_fallback.rhai`'s catch-all.
    // Note: `classify_waterfall`'s semantic rule selection does not
    // guarantee `classify_fallback.rhai` is the literal last rule
    // evaluated (unlike `select_rules_deterministic`, which always appends
    // fallback rules last) — whichever conforming rule scores last in the
    // selection wins the `review`/`reason` fields, so this test asserts
    // only the `category` contract, not `needs_review`.
    let dir = build_overlay_rules_dir();
    let registry = RuleRegistry::load_from_dir(dir.path()).expect("rules directory loads");
    let mut engine = ClassificationEngine::default();

    let tx = SampleTransaction {
        tx_id: "tx-no-match".to_string(),
        account_id: "TESTBANK--CLIENT-CHK--2024-05".to_string(),
        date: "2024-05-11".to_string(),
        amount: "10.00".to_string(),
        description: "Completely unrelated mystery charge".to_string(),
    };

    let outcome = registry
        .classify_waterfall(&mut engine, &tx)
        .expect("waterfall reaches fallback");

    assert_eq!(outcome.category, "Unclassified");
}

#[test]
fn missing_client_overlay_directory_is_not_an_error() {
    // No client/ subdirectory at all — the normal, unconfigured state.
    let dir = tempfile::TempDir::new().expect("create temp rules dir");
    fs::write(dir.path().join("classify_fallback.rhai"), FALLBACK_RULE_BODY)
        .expect("write fallback rule");

    let registry = RuleRegistry::load_from_dir(dir.path())
        .expect("rules directory loads without a client overlay");
    assert_eq!(registry.rule_count(), 1);
}
