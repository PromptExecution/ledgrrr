//! Durable local ownership of revision artifacts and operation intent.
//!
//! Authentication and credentials belong to the embedding host. An `ActorId`
//! passed here must come from that host's authenticated principal, never from an
//! untrusted request's actor field. This library does not contact providers or
//! publish graphs and cannot fence a provider that accepts unconditional writes.
#![doc = include_str!("../../../book/src/revision-io.md")]
#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
pub use ufo_types::revision::{
    ActorId, ArtifactDigest, BranchId, ExpectedHead, IndexCheckpoint, OperationId,
    OperationReceipt, PortableBundle, ProjectId, RevisionId, SyncStatus,
};

/// Configured provider handles, containing no credentials.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectBinding {
    pub provider: String,
    pub remote_project: String,
    pub model_dialect: String,
}

/// Each permission is explicit: possession of a hash never grants read access.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grant {
    pub read: bool,
    pub propose: bool,
    pub administer: bool,
}

impl Grant {
    pub const OWNER: Self = Self {
        read: true,
        propose: true,
        administer: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intake {
    pub project: ProjectId,
    pub branch: BranchId,
    pub operation: OperationId,
    pub expected_head: ExpectedHead,
    pub binding: ProjectBinding,
}

/// The host's worker authorization remains required in addition to this token.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchToken {
    pub project: ProjectId,
    pub branch: BranchId,
    pub operation: OperationId,
    pub fence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StoredOperation {
    pub receipt: OperationReceipt,
    pub generation: u64,
    pub raw_envelope: ArtifactDigest,
    pub canonical_envelope: ArtifactDigest,
    pub dispatch: Option<DispatchToken>,
    pub dispatch_evidence: Option<String>,
    pub actual_revision: Option<RevisionId>,
    pub indexing_work: Option<ArtifactDigest>,
    pub commit_evidence: Option<CommitEvidence>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepared: Option<PreparedCandidate>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub send_authorized: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub observed_existing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conflict_evidence: Option<ArtifactDigest>,
}

fn is_false(v: &bool) -> bool {
    !*v
}

/// Immutable operational preparation; intake receipt identity stays unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreparedCandidate {
    pub envelope: ArtifactDigest,
    pub candidate_digest: ArtifactDigest,
    pub parent: ExpectedHead,
    pub projection_digest: ArtifactDigest,
    pub identity_digest: ArtifactDigest,
}

/// Evidence independently obtained by an authenticated provider adapter.
/// The store checks binding and identity; it cannot prove network observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitEvidence {
    pub token: DispatchToken,
    pub binding: ProjectBinding,
    pub expected_head: ExpectedHead,
    pub proposal_digest: ArtifactDigest,
    pub actual_revision: RevisionId,
    pub observation: String,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("project permission denied")]
    Denied,
    #[error("record not found")]
    NotFound,
    #[error("immutable identity conflict")]
    IdentityConflict,
    #[error("invalid input: {0}")]
    Invalid(String),
    #[error("unsupported or unrelated database schema")]
    Schema,
    #[error("corrupt persisted record: {0}")]
    Corrupt(String),
    #[error("stale generation or dispatch fence")]
    Stale,
    #[error("branch dispatch is unresolved")]
    BranchBlocked,
    #[error("invalid receipt transition")]
    Transition,
    #[error(transparent)]
    Sql(#[from] rusqlite::Error),
    #[error(transparent)]
    Revision(#[from] ufo_types::revision::RevisionError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

mod store;
pub use store::{IndexPublisher, Store};
