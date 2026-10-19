//! Shared revision discovery wire data. No evaluator, clock, storage or grants.
//!
//! Structural validation cannot establish accepted ancestry, authorization or
//! graph completeness. The owner proves those before returning a response.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ArtifactDigest, BranchId, IndexCheckpoint, ProjectId, RevisionError, RevisionId};

/// Wire ceilings; services may impose smaller resource limits.
pub const MAX_REVISION_QUERY_BYTES: usize = 65_536;
pub const MAX_REVISION_QUERY_DEADLINE_MS: u64 = 60_000;
pub const MAX_REVISION_QUERY_ROWS: usize = 10_000;
pub const MAX_REVISION_QUERY_VARIABLES: usize = 256;
pub const MAX_REVISION_QUERY_RESULT_BYTES: usize = 4_194_304;
pub const MAX_REVISION_QUERY_MODEL_DIALECT_BYTES: usize = 1_024;

fn invalid(path: &str, reason: &str) -> RevisionError {
    RevisionError::InvalidModel {
        path: format!("query/{path}"),
        reason: reason.into(),
    }
}

fn checkpoint(value: &IndexCheckpoint, project: &ProjectId) -> Result<(), RevisionError> {
    if &value.project != project {
        return Err(invalid("checkpoint/project", "project mismatch"));
    }
    super::valid_identity("model dialect", &value.dialect)?;
    if value.dialect.len() > MAX_REVISION_QUERY_MODEL_DIALECT_BYTES {
        return Err(invalid(
            "checkpoint/dialect",
            "model dialect byte limit exceeded",
        ));
    }
    Ok(())
}

// Deserialize through the same semantic check used for Rust-created values.
// The local Wire struct avoids a second independently maintained field list.
macro_rules! validated_struct {
    ($(#[$meta:meta])* pub struct $name:ident { $(pub $field:ident: $ty:ty,)* }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct $name { $(pub $field: $ty,)* }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                #[derive(Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Wire { $($field: $ty,)* }
                let input = super::UniqueJson::deserialize(deserializer)?;
                let wire: Wire = serde_json::from_value(input.0)
                    .map_err(serde::de::Error::custom)?;
                let value = Self { $($field: wire.$field,)* };
                value.validate().map_err(serde::de::Error::custom)?;
                Ok(value)
            }
        }
    };
}

/// Minimum means accepted ancestry, never UUID or lexical ordering. If
/// allow_older is true, the owner may return an explicitly stale fallback below
/// the minimum. Exact always names one revision and never falls back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevisionQuerySelector {
    Exact {
        revision: RevisionId,
    },
    Minimum {
        checkpoint: IndexCheckpoint,
        allow_older: bool,
    },
    Current {
        allow_older: bool,
    },
}

validated_struct! {
    /// Relative deadline includes waiting for a checkpoint and evaluation.
    /// Authentication and server resource ceilings never come from this value.
    pub struct RevisionQueryRequest {
        pub project: ProjectId,
        pub branch: BranchId,
        pub selector: RevisionQuerySelector,
        pub query: String,
        pub deadline_ms: u64,
    }
}

impl RevisionQueryRequest {
    pub fn validate(&self) -> Result<(), RevisionError> {
        if self.query.trim().is_empty() || self.query.len() > MAX_REVISION_QUERY_BYTES {
            return Err(invalid("text", "empty query or query byte limit exceeded"));
        }
        if self.deadline_ms == 0 || self.deadline_ms > MAX_REVISION_QUERY_DEADLINE_MS {
            return Err(invalid("deadline_ms", "deadline outside wire bounds"));
        }
        if let RevisionQuerySelector::Minimum { checkpoint: cp, .. } = &self.selector {
            checkpoint(cp, &self.project)?;
        }
        Ok(())
    }
}

validated_struct! {
    /// A claimed immutable complete graph. The owner must independently verify
    /// its accepted input and actual artifact bytes before publishing it.
    pub struct RevisionGraphDescriptor {
        pub checkpoint: IndexCheckpoint,
        pub projection_schema: String,
        pub accepted_candidate_digest: ArtifactDigest,
        pub artifact_digest: ArtifactDigest,
        pub quad_count: u64,
    }
}

impl RevisionGraphDescriptor {
    pub fn validate(&self) -> Result<(), RevisionError> {
        checkpoint(&self.checkpoint, &self.checkpoint.project)?;
        super::valid_identity("projection_schema", &self.projection_schema)?;
        if self.artifact_digest != self.checkpoint.graph_digest {
            return Err(invalid(
                "graph/artifact_digest",
                "graph byte digest mismatch",
            ));
        }
        // Even an empty model carries accepted-revision metadata triples.
        if self.quad_count == 0 {
            return Err(invalid("graph/quad_count", "revision metadata is required"));
        }
        Ok(())
    }
}

/// Fresh means indexed_revision equals the independently observed branch model
/// revision sampled for this answer. An exact historical answer can be Stale
/// while fully satisfying its requested revision. Head changes remain possible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryFreshness {
    Fresh,
    Stale,
}

/// Literal lexemes are retained even when their datatype's value is ill-formed
/// (which is legal RDF). Blank-node labels are opaque and scoped to this result;
/// they must not be treated as persistent model identities or raw RDF syntax.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RdfTerm {
    Iri { value: String },
    BlankNode { label: String },
    Literal { value: String, datatype: String },
    LanguageLiteral { value: String, language: String },
}

impl RdfTerm {
    pub fn validate(&self) -> Result<(), RevisionError> {
        let iri = |value: &str| {
            oxiri::Iri::parse(value)
                .map(|_| ())
                .map_err(|_| invalid("term/iri", "invalid absolute IRI"))
        };
        match self {
            Self::Iri { value } => iri(value),
            Self::BlankNode { label } => super::valid_identity("blank node label", label),
            Self::Literal { datatype, .. } => {
                iri(datatype)?;
                if datatype == "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString" {
                    return Err(invalid(
                        "term/language",
                        "langString requires a language tag",
                    ));
                }
                Ok(())
            }
            Self::LanguageLiteral { language, .. } => {
                oxilangtag::LanguageTag::parse(language.as_str())
                    .map(|_| ())
                    .map_err(|_| invalid("term/language", "invalid BCP47 language tag"))
            }
        }
    }
}

/// Unbound variables are omitted from a row, distinct from an empty literal.
/// Row order, duplicate solutions and projected variable order are preserved.
/// Column labels are opaque wire strings. The adapter must obtain actual
/// variable names and result kind from its parsed query/evaluator, and must not
/// interpolate these labels into SPARQL text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevisionQueryResults {
    Select {
        variables: Vec<String>,
        rows: Vec<BTreeMap<String, RdfTerm>>,
    },
    Ask {
        value: bool,
    },
}

impl RevisionQueryResults {
    pub fn validate(&self) -> Result<(), RevisionError> {
        if let Self::Select { variables, rows } = self {
            if variables.len() > MAX_REVISION_QUERY_VARIABLES
                || rows.len() > MAX_REVISION_QUERY_ROWS
            {
                return Err(invalid("results", "row or variable limit exceeded"));
            }
            let names: BTreeSet<_> = variables.iter().collect();
            if names.len() != variables.len() {
                return Err(invalid("results/variables", "duplicate projected variable"));
            }
            for name in variables {
                super::valid_identity("projected variable", name)?;
            }
            for row in rows {
                for (name, term) in row {
                    if !names.contains(name) {
                        return Err(invalid(
                            "results/rows",
                            "binding not in projected variables",
                        ));
                    }
                    term.validate()?;
                }
            }
        }
        let bytes = serde_json::to_vec(self).map_err(|e| RevisionError::Json(e.to_string()))?;
        if bytes.len() > MAX_REVISION_QUERY_RESULT_BYTES {
            return Err(invalid("results", "result byte limit exceeded"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryUnavailableReason {
    MissingRevision,
    MissingCheckpoint,
    MissingArtifact,
    CorruptArtifact,
    UnknownLineage,
    ExternalRevision,
    UnsupportedProjection,
    InvalidQuery,
    UnsupportedQuery,
    DeadlineExceeded,
    CapacityExceeded,
    EvaluationFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum RevisionQueryOutcome {
    Completed {
        model_revision: RevisionId,
        indexed_revision: RevisionId,
        graph: RevisionGraphDescriptor,
        freshness: QueryFreshness,
        results: RevisionQueryResults,
    },
    Pending {
        model_revision: Option<RevisionId>,
        requested_revision: Option<RevisionId>,
        available_graph: Option<RevisionGraphDescriptor>,
    },
    Unavailable {
        model_revision: Option<RevisionId>,
        requested_revision: Option<RevisionId>,
        reason: QueryUnavailableReason,
    },
}

validated_struct! {
    /// Owner-supplied identities bind every outcome, including absence/failure.
    pub struct RevisionQueryResponse {
        pub project: ProjectId,
        pub branch: BranchId,
        pub outcome: RevisionQueryOutcome,
    }
}

impl RevisionQueryResponse {
    pub fn validate(&self) -> Result<(), RevisionError> {
        match &self.outcome {
            RevisionQueryOutcome::Completed {
                model_revision,
                indexed_revision,
                graph,
                freshness,
                results,
            } => {
                graph.validate()?;
                checkpoint(&graph.checkpoint, &self.project)?;
                if indexed_revision != &graph.checkpoint.revision {
                    return Err(invalid("indexed_revision", "checkpoint revision mismatch"));
                }
                if (*freshness == QueryFreshness::Fresh) != (model_revision == indexed_revision) {
                    return Err(invalid(
                        "freshness",
                        "freshness contradicts revision identity",
                    ));
                }
                results.validate()?;
            }
            RevisionQueryOutcome::Pending {
                available_graph: Some(graph),
                ..
            } => {
                graph.validate()?;
                checkpoint(&graph.checkpoint, &self.project)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Check request/response consistency. Minimum ancestry and permission to
    /// use an older checkpoint require owner evidence and cannot be proven here.
    pub fn validate_for(&self, request: &RevisionQueryRequest) -> Result<(), RevisionError> {
        request.validate()?;
        self.validate()?;
        if self.project != request.project || self.branch != request.branch {
            return Err(invalid("response", "request project or branch mismatch"));
        }
        let requested = match &request.selector {
            RevisionQuerySelector::Exact { revision } => Some(revision),
            RevisionQuerySelector::Minimum { checkpoint, .. } => Some(&checkpoint.revision),
            RevisionQuerySelector::Current { .. } => None,
        };
        match &self.outcome {
            RevisionQueryOutcome::Completed {
                indexed_revision,
                graph,
                freshness,
                ..
            } => match &request.selector {
                RevisionQuerySelector::Exact { revision } if revision != indexed_revision => {
                    return Err(invalid("response", "exact revision cannot fall back"));
                }
                RevisionQuerySelector::Minimum {
                    checkpoint: minimum,
                    ..
                } if &minimum.revision == indexed_revision && minimum != &graph.checkpoint => {
                    return Err(invalid("response", "minimum checkpoint identity mismatch"));
                }
                RevisionQuerySelector::Current { allow_older: false }
                    if *freshness != QueryFreshness::Fresh =>
                {
                    return Err(invalid("response", "current revision cannot fall back"));
                }
                _ => {}
            },
            RevisionQueryOutcome::Pending {
                requested_revision, ..
            }
            | RevisionQueryOutcome::Unavailable {
                requested_revision, ..
            } => {
                if requested.is_some() && requested != requested_revision.as_ref() {
                    return Err(invalid("response", "requested revision mismatch"));
                }
            }
        }
        Ok(())
    }
}
