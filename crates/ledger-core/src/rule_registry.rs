//! Rule Registry — multi-rule orchestration and semantic rule selection.
//!
//! ## Purpose
//! This module defines the interface for discovering, selecting, and applying Rhai
//! classification rules from a directory-based registry. It also defines the Rust-side
//! mirror types for the `reqif-opa-mcp` Python sidecar's JSON output.
//!
//! ## Status
//! - `RuleRegistry::load_from_dir` — implemented; builds lexical-similarity index eagerly
//! - `RuleRegistry::select_rules_deterministic` — implemented keyword fallback
//! - `RuleRegistry::select_rules_semantic` — implemented (Jaccard/lexical); used by waterfall
//! - `RuleRegistry::classify_waterfall` — implemented; routes through semantic selector
//! - `SemanticRuleSelector` — implemented with lexical similarity; upgrade path to vector
//!   embeddings (`fastembed-rs` / `candle` / ONNX) is wired at the trait boundary
//!
//! ## External Dependency
//! The Python sidecar at <https://github.com/PromptExecution/reqif-opa-mcp> produces
//! `RequirementCandidate` JSON objects that are deserialized into `ReqIfCandidate` here.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::classify::{
    ClassificationEngine, ClassificationError, ClassificationOutcome, SampleTransaction,
};

// ============================================================================
// Internal helpers
// ============================================================================

/// Check whether a rule filename (stem only) matches a given keyword pattern.
fn filename_contains(path: &Path, keyword: &str) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.to_ascii_lowercase().contains(keyword))
        .unwrap_or(false)
}

fn is_transaction_rule(path: &Path) -> bool {
    let Ok(src) = std::fs::read_to_string(path) else {
        return false;
    };

    src.contains("fn classify(")
}

fn semantic_candidate_id(source_kind: &str, source_ref: &str, text: &str) -> String {
    let canonical = format!(
        "semantic_candidate|{}|{}|{}",
        source_kind.trim(),
        source_ref.trim(),
        text.trim()
    );
    blake3::hash(canonical.as_bytes()).to_hex().to_string()
}

fn semantic_tokens(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter_map(|token| {
            let token = token.trim().to_ascii_lowercase();
            (token.len() >= 3).then_some(token)
        })
        .collect()
}

fn lexical_similarity(query: &BTreeSet<String>, candidate: &BTreeSet<String>) -> f64 {
    if query.is_empty() || candidate.is_empty() {
        return 0.0;
    }
    let intersection = query.intersection(candidate).count() as f64;
    let union = query.union(candidate).count() as f64;
    intersection / union
}

// ============================================================================
// MIRROR TYPES: reqif-opa-mcp JSON output shapes
// ============================================================================

/// Mirrors `reqif-opa-mcp`'s `RequirementCandidate` JSON output.
///
/// Populated by calling the Python sidecar and deserializing its NDJSON output.
/// The Python pipeline produces these from a `DocumentGraph` after running through
/// the OPA gate. Each candidate represents a deterministically-derived requirement
/// from a source document.
///
/// # Sidecar pipeline
/// ```text
/// source PDF
///   → extract_docling_document
///   → DocumentGraph
///   → RequirementCandidate  ← serialized here
///   → OPA gate
///   → emit_reqif_xml
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReqIfCandidate {
    /// Stable key, e.g. `"REQ-001"` or a SHA-derived slug.
    pub key: String,
    /// Requirement text extracted from the source document.
    pub text: String,
    /// Section identifier within the source document (e.g., `"3.2.1"`).
    pub section: String,
    /// Human-readable rationale for why this was identified as a requirement.
    pub rationale: String,
    /// Source of the confidence score: `"rule"`, `"llm"`, `"heuristic"`, etc.
    pub confidence_source: String,
    /// Confidence in [0.0, 1.0].
    pub confidence: f64,
}

/// A document chunk with text and semantic anchoring.
///
/// Maps to `reqif-opa-mcp`'s `DocumentNode` — a canonical graph node that carries
/// extracted text, its parent in the document tree, a semantic identifier, and
/// positional anchors into the source PDF.
///
/// `DocumentChunk` objects are produced by the Python sidecar during the
/// `extract_docling_document` → `DocumentGraph` phase and streamed to Rust via
/// NDJSON over a subprocess pipe.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DocumentChunk {
    /// Unique node identifier within the document graph.
    pub node_id: String,
    /// Extracted text content of this chunk.
    pub text: String,
    /// Parent node ID in the document tree (`None` for root chunks).
    pub parent_id: Option<String>,
    /// Semantic identifier: section number, heading slug, etc.
    pub semantic_id: String,
    /// Page anchors `[page_number, offset_chars]` into the source PDF.
    pub anchors: Vec<[u32; 2]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SemanticCandidate {
    pub id: String,
    pub rule_path: PathBuf,
    pub source_kind: String,
    pub source_ref: String,
    pub text: String,
}

#[derive(Debug, Clone)]
struct SemanticIndexEntry {
    candidate: SemanticCandidate,
    tokens: BTreeSet<String>,
}

// ============================================================================
// ERRORS
// ============================================================================

/// Errors arising from rule registry operations.
#[derive(Debug, thiserror::Error)]
pub enum RuleRegistryError {
    #[error("failed to read rules directory: {0}")]
    Io(#[from] std::io::Error),

    #[error("no rules found in directory: {0}")]
    NoRules(PathBuf),

    #[error("classification error in waterfall: {0}")]
    Classification(#[from] ClassificationError),
}

// ============================================================================
// TRAIT: Semantic rule selection
// ============================================================================

/// Selects applicable Rhai rule files for a given transaction based on
/// lexical similarity to rule file content and `ReqIfCandidate` metadata.
///
/// # Current implementation
/// `RuleRegistry` implements this trait using Jaccard similarity over tokenised
/// rule text (filename + content / ReqIfCandidate fields). The index is built
/// eagerly by `load_from_dir` so `classify_waterfall` can call
/// `select_rules_semantic` immediately without a separate build step.
///
/// # Future: vector embeddings
/// The intended upgrade path is local embedding inference (`fastembed-rs`,
/// `candle`, or an ONNX sidecar) to replace Jaccard with cosine similarity in
/// a shared embedding space. `build_embedding_index` and `select_rules_semantic`
/// are kept as a trait so the production implementation can be swapped without
/// changing call sites.
pub trait SemanticRuleSelector {
    /// Select the `top_k` most relevant rules for a transaction using
    /// lexical-similarity scoring. Falls back to `select_rules_deterministic`
    /// when the index is empty or `top_k == 0`.
    ///
    /// # Prerequisites
    /// - Index must be built via `build_embedding_index` (called automatically
    ///   by `load_from_dir`).
    fn select_rules_semantic(&self, tx: &SampleTransaction, top_k: usize) -> Vec<PathBuf>;

    /// Build or rebuild the lexical-similarity index from loaded rule files and
    /// any paired `ReqIfCandidate` sidecars. Called automatically by
    /// `load_from_dir`; re-call after hot-reloading rules from disk.
    fn build_embedding_index(&mut self) -> Result<(), RuleRegistryError>;
}

// ============================================================================
// STRUCT: RuleRegistry
// ============================================================================

/// Registry of Rhai rule files with their associated `ReqIfCandidate` metadata.
///
/// Rules are loaded from a `rules/` directory at startup. Each `.rhai` file
/// represents one classification rule. Optionally, a paired `.reqif.json`
/// sidecar (produced by the Python `reqif-opa-mcp` pipeline) associates
/// `ReqIfCandidate` objects with each rule file.
///
/// # Production pipeline (waterfall model)
/// 1. `load_from_dir` — discover all `.rhai` files
/// 2. `select_rules_deterministic` — keyword-match to narrow the candidate set
/// 3. `classify_waterfall` — run rules in order; first non-`Unclassified` wins
///
/// # Planned: Semantic pipeline
/// Once embedding infrastructure is available, `SemanticRuleSelector` will replace
/// step 2 with vector similarity over `ReqIfCandidate` embeddings.
pub struct RuleRegistry {
    /// Paths to discovered `.rhai` rule files, sorted alphabetically.
    rule_paths: Vec<PathBuf>,
    /// Optional `ReqIfCandidate` objects associated with each rule, indexed
    /// parallel to `rule_paths`. `None` if no sidecar JSON was found.
    candidates: Vec<Option<ReqIfCandidate>>,
    /// Local deterministic lexical index used until model embeddings are wired.
    semantic_index: Vec<SemanticIndexEntry>,
}

impl RuleRegistry {
    /// Load all `.rhai` files from a rules directory.
    ///
    /// Scans `rules_dir` for transaction rules ending in `.rhai` and sorts them
    /// alphabetically. A transaction rule must expose `fn classify(tx)`.
    /// Document-shape rules such as `classify_document_shape.rhai` are excluded
    /// because they expose a different entry point.
    /// Optionally loads a paired `<rule_name>.reqif.json` sidecar for each rule.
    ///
    /// Returns `RuleRegistryError::NoRules` if the directory contains no `.rhai` files.
    pub fn load_from_dir(rules_dir: &Path) -> Result<Self, RuleRegistryError> {
        let mut rule_paths: Vec<PathBuf> = std::fs::read_dir(rules_dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("rhai")
                    && is_transaction_rule(&path)
                {
                    Some(path)
                } else {
                    None
                }
            })
            .collect();

        if rule_paths.is_empty() {
            return Err(RuleRegistryError::NoRules(rules_dir.to_path_buf()));
        }

        rule_paths.sort();

        // Load optional .reqif.json sidecars in parallel with rule_paths order
        let candidates: Vec<Option<ReqIfCandidate>> = rule_paths
            .iter()
            .map(|p| {
                let sidecar = p.with_extension("reqif.json");
                if sidecar.exists() {
                    std::fs::read_to_string(&sidecar)
                        .ok()
                        .and_then(|s| serde_json::from_str::<ReqIfCandidate>(&s).ok())
                } else {
                    None
                }
            })
            .collect();

        let mut registry = Self {
            rule_paths,
            candidates,
            semantic_index: Vec::new(),
        };
        // Build the lexical-similarity index eagerly so classify_waterfall can use
        // select_rules_semantic immediately. Errors are non-fatal: if a rule file
        // cannot be read the index will be partial but the deterministic fallback
        // inside select_rules_semantic covers the gap.
        let _ = registry.build_embedding_index();
        Ok(registry)
    }

    /// Select rules applicable to a transaction by keyword match (deterministic fallback).
    ///
    /// Filters `rule_paths` by checking whether the rule filename contains keywords that
    /// match fields in `tx`. The keyword mapping is:
    /// - `account_id` contains "hsbc" → include `*foreign*` rules
    /// - `description` contains "btc", "eth", or "crypto" → include `*crypto*` rules
    /// - `description` contains "rent" or "rental" → include `*rental*` rules
    /// - `description` contains "contractor", "freelance", or "self-employ" → include `*self_employ*` rules
    /// - `*fallback*` rules are always appended last
    ///
    /// Returns a unique, ordered list: matched rules first, fallback rules last.
    /// If no keywords matched any non-fallback rule, all non-fallback rules are included
    /// so the waterfall always has candidates.
    pub fn select_rules_deterministic(&self, tx: &SampleTransaction) -> Vec<PathBuf> {
        let account_id_lower = tx.account_id.to_ascii_lowercase();
        let desc_lower = tx.description.to_ascii_lowercase();

        // Determine which rule-type keywords are relevant for this transaction
        let mut wanted_patterns: Vec<&str> = Vec::new();

        if account_id_lower.contains("hsbc")
            || desc_lower.contains("foreign")
            || desc_lower.contains("eur")
            || desc_lower.contains("germany")
            || desc_lower.contains(" de ")
        {
            wanted_patterns.push("foreign");
        }
        if desc_lower.contains("btc") || desc_lower.contains("eth") || desc_lower.contains("crypto")
        {
            wanted_patterns.push("crypto");
        }
        if desc_lower.contains("rent") || desc_lower.contains("rental") {
            wanted_patterns.push("schedule_e");
        }
        if desc_lower.contains("contractor")
            || desc_lower.contains("freelance")
            || desc_lower.contains("self-employ")
            || desc_lower.contains("self_employ")
            || desc_lower.contains("invoice")
            || desc_lower.contains("client")
            || desc_lower.contains("consulting")
            || desc_lower.contains("1099")
        {
            wanted_patterns.push("self_employ");
            wanted_patterns.push("schedule_c");
        }

        let mut matched: Vec<PathBuf> = Vec::new();
        let mut fallbacks: Vec<PathBuf> = Vec::new();

        for path in &self.rule_paths {
            let is_fallback = filename_contains(path, "fallback");

            if is_fallback {
                fallbacks.push(path.clone());
                continue;
            }

            if wanted_patterns.is_empty() {
                // No keyword matched — include all non-fallback rules
                matched.push(path.clone());
            } else if wanted_patterns.iter().any(|p| filename_contains(path, p)) {
                matched.push(path.clone());
            }
        }

        // If keyword patterns were specified but nothing matched, fall through to all non-fallback rules
        if !wanted_patterns.is_empty() && matched.is_empty() {
            for path in &self.rule_paths {
                if !filename_contains(path, "fallback") {
                    matched.push(path.clone());
                }
            }
        }

        // Deduplicate while preserving order
        let mut seen = std::collections::HashSet::new();
        let mut result: Vec<PathBuf> = Vec::new();
        for path in matched.into_iter().chain(fallbacks) {
            if seen.insert(path.clone()) {
                result.push(path);
            }
        }

        result
    }

    /// Apply all rules in order, returning the first non-`Unclassified` result.
    ///
    /// This is the production multi-rule pipeline (waterfall model). When the
    /// lexical-similarity index is populated (built by `load_from_dir`), rule
    /// selection uses `select_rules_semantic` which scores candidates by Jaccard
    /// similarity over tokenised rule text and falls back to the keyword-match
    /// path when the index is empty. Execution stops as soon as one rule returns
    /// a `category` other than `"Unclassified"`.
    ///
    /// If all rules return `"Unclassified"`, the final unclassified outcome is
    /// returned so fallback reason/review fields are preserved. Rule execution
    /// errors abort the waterfall because silently skipping a broken financial
    /// rule would hide audit-relevant failures.
    pub fn classify_waterfall(
        &self,
        engine: &mut ClassificationEngine,
        tx: &SampleTransaction,
    ) -> Result<ClassificationOutcome, ClassificationError> {
        // Use semantic selection (lexical-similarity index) when available;
        // select_rules_semantic falls back to deterministic when the index is empty.
        let selected = self.select_rules_semantic(tx, self.rule_paths.len());

        let mut last_unclassified = None;

        for rule_path in selected {
            let outcome = engine.run_rule_from_file(&rule_path, tx)?;
            if outcome.category != "Unclassified" {
                return Ok(outcome);
            }

            last_unclassified = Some(outcome);
        }

        Ok(last_unclassified.unwrap_or_else(|| ClassificationOutcome {
            category: "Unclassified".to_string(),
            confidence: 0.0,
            needs_review: true,
            reason: "no rule produced a classification".to_string(),
        }))
    }

    /// Return the number of rules loaded in this registry.
    pub fn rule_count(&self) -> usize {
        self.rule_paths.len()
    }

    /// Return the rule paths in registry order.
    pub fn rule_paths(&self) -> &[PathBuf] {
        &self.rule_paths
    }

    /// Return the number of loaded ReqIF sidecar candidates.
    pub fn candidate_count(&self) -> usize {
        self.candidates
            .iter()
            .filter(|candidate| candidate.is_some())
            .count()
    }

    /// Return stable semantic candidate identifiers in index order.
    pub fn semantic_candidate_ids(&self) -> Vec<String> {
        self.semantic_index
            .iter()
            .map(|entry| entry.candidate.id.clone())
            .collect()
    }

    /// Return semantic candidates in index order.
    pub fn semantic_candidates(&self) -> Vec<SemanticCandidate> {
        self.semantic_index
            .iter()
            .map(|entry| entry.candidate.clone())
            .collect()
    }
}

impl SemanticRuleSelector for RuleRegistry {
    fn select_rules_semantic(&self, tx: &SampleTransaction, top_k: usize) -> Vec<PathBuf> {
        if top_k == 0 || self.semantic_index.is_empty() {
            return self.select_rules_deterministic(tx);
        }

        let query = semantic_tokens(&format!("{} {}", tx.account_id, tx.description));
        let mut scored = self
            .semantic_index
            .iter()
            .map(|entry| {
                (
                    lexical_similarity(&query, &entry.tokens),
                    entry.candidate.id.as_str(),
                    entry.candidate.rule_path.clone(),
                )
            })
            .collect::<Vec<_>>();
        scored.sort_by(|a, b| {
            b.0.total_cmp(&a.0)
                .then_with(|| a.1.cmp(b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        let mut selected = Vec::new();
        let mut seen = std::collections::HashSet::new();
        const MIN_LEXICAL_SIMILARITY: f64 = 0.05;
        for (score, _id, path) in scored {
            if score < MIN_LEXICAL_SIMILARITY {
                continue;
            }
            if seen.insert(path.clone()) {
                selected.push(path);
            }
            if selected.len() >= top_k {
                break;
            }
        }

        if selected.is_empty() {
            self.select_rules_deterministic(tx)
        } else {
            selected
        }
    }

    fn build_embedding_index(&mut self) -> Result<(), RuleRegistryError> {
        let mut entries = Vec::new();
        for (rule_path, candidate) in self.rule_paths.iter().zip(self.candidates.iter()) {
            let source_ref = rule_path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unknown_rule")
                .to_string();
            let text = if let Some(candidate) = candidate {
                format!(
                    "{} {} {} {}",
                    candidate.key, candidate.section, candidate.text, candidate.rationale
                )
            } else {
                std::fs::read_to_string(rule_path)?
            };
            let id = semantic_candidate_id("rule", &source_ref, &text);
            entries.push(SemanticIndexEntry {
                tokens: semantic_tokens(&format!("{source_ref} {text}")),
                candidate: SemanticCandidate {
                    id,
                    rule_path: rule_path.clone(),
                    source_kind: "rule".to_string(),
                    source_ref,
                    text,
                },
            });
        }
        entries.sort_by(|a, b| {
            a.candidate
                .id
                .cmp(&b.candidate.id)
                .then_with(|| a.candidate.rule_path.cmp(&b.candidate.rule_path))
        });
        self.semantic_index = entries;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_rule(dir: &std::path::Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).unwrap();
        path
    }

    fn sample_tx(account_id: &str, description: &str) -> SampleTransaction {
        SampleTransaction {
            tx_id: "test-tx".to_string(),
            account_id: account_id.to_string(),
            date: "2024-06-01".to_string(),
            amount: "100.00".to_string(),
            description: description.to_string(),
        }
    }

    #[test]
    fn semantic_candidate_ids_are_stable() {
        let first = semantic_candidate_id("rule", "classify_schedule_c.rhai", "client invoice");
        let second = semantic_candidate_id("rule", "classify_schedule_c.rhai", "client invoice");
        let different = semantic_candidate_id("rule", "classify_schedule_e.rhai", "client invoice");

        assert_eq!(first, second);
        assert_ne!(first, different);
        assert_eq!(first.len(), 64);
    }

    #[test]
    fn load_from_dir_builds_semantic_index() {
        let dir = TempDir::new().unwrap();
        make_rule(
            dir.path(),
            "classify_schedule_c.rhai",
            r#"fn classify(tx) { #{category: "Unclassified", confidence: 0.0, review: false, reason: ""} }"#,
        );
        let registry = RuleRegistry::load_from_dir(dir.path()).unwrap();
        assert_eq!(registry.rule_count(), 1);
        assert!(
            !registry.semantic_candidates().is_empty(),
            "semantic index must be built eagerly by load_from_dir"
        );
    }

    #[test]
    fn select_rules_semantic_returns_all_rules_for_unrelated_tx() {
        let dir = TempDir::new().unwrap();
        make_rule(
            dir.path(),
            "classify_schedule_c.rhai",
            r#"fn classify(tx) { #{category: "Unclassified", confidence: 0.0, review: false, reason: ""} }"#,
        );
        make_rule(
            dir.path(),
            "classify_schedule_e.rhai",
            r#"fn classify(tx) { #{category: "Unclassified", confidence: 0.0, review: false, reason: ""} }"#,
        );
        let registry = RuleRegistry::load_from_dir(dir.path()).unwrap();
        let tx = sample_tx("unknown", "purchase at store");
        let selected = registry.select_rules_semantic(&tx, 10);
        assert_eq!(
            selected.len(),
            2,
            "should return all rules when no strong match"
        );
    }

    #[test]
    fn classify_waterfall_uses_semantic_path() {
        let dir = TempDir::new().unwrap();
        // Rule that matches "invoice" in the description
        make_rule(
            dir.path(),
            "classify_schedule_c.rhai",
            r#"fn classify(tx) {
                if tx.description.contains("invoice") {
                    #{category: "ScheduleC", confidence: 0.9, review: false, reason: "invoice found"}
                } else {
                    #{category: "Unclassified", confidence: 0.0, review: false, reason: ""}
                }
            }"#,
        );
        make_rule(
            dir.path(),
            "classify_fallback.rhai",
            r#"fn classify(tx) {
                #{category: "Other", confidence: 0.5, review: false, reason: "fallback"}
            }"#,
        );

        let registry = RuleRegistry::load_from_dir(dir.path()).unwrap();
        let mut engine = ClassificationEngine::default();
        let tx = sample_tx("ACME", "client invoice Q2");

        let outcome = registry.classify_waterfall(&mut engine, &tx).unwrap();
        assert_eq!(outcome.category, "ScheduleC");
    }

    #[test]
    fn lexical_similarity_scores_intersection_over_union() {
        let a: BTreeSet<String> = ["foo", "bar", "baz"].iter().map(|s| s.to_string()).collect();
        let b: BTreeSet<String> = ["foo", "bar", "qux"].iter().map(|s| s.to_string()).collect();
        let sim = lexical_similarity(&a, &b);
        // intersection={foo,bar}=2, union={foo,bar,baz,qux}=4 → 0.5
        assert!((sim - 0.5).abs() < 1e-9);
    }

    #[test]
    fn lexical_similarity_empty_sets_return_zero() {
        let empty = BTreeSet::new();
        let nonempty: BTreeSet<String> = ["foo"].iter().map(|s| s.to_string()).collect();
        assert_eq!(lexical_similarity(&empty, &nonempty), 0.0);
        assert_eq!(lexical_similarity(&nonempty, &empty), 0.0);
        assert_eq!(lexical_similarity(&empty, &empty), 0.0);
    }
}
