//! Portable revision data and pure, deterministic hydration/dehydration.
//!
//! Inlined from ufo-types @ rev 5f8d018 (the last revision that included this
//! module). Storage, clocks, model-server calls and authorization belong to
//! adapters. Original source bytes are retained independently of the typed
//! semantic model. The wire dialect is versioned independently of the crate's
//! release version.

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ufo_types::{ElementId, ElementKind, Relation, SourceAnchor};

mod merge;
mod query;
pub use merge::{
    ChangeSet, Conflict, ConflictKind, ConflictSubject, MergeOutcome, RecordChange, merge_models,
};
pub use query::*;

/// The first portable contract. Unknown versions require an explicit migration.
pub const DIALECT: &str = "urn:b00t:dialect:ufo-types:revision:1.0.0";

/// Malformed or incomplete portable data never becomes a successful hydration.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RevisionError {
    #[error("invalid {kind}: {value:?}")]
    InvalidIdentity { kind: String, value: String },
    #[error("invalid model at {path}: {reason}")]
    InvalidModel { path: String, reason: String },
    #[error("unsupported portable dialect {0}")]
    UnsupportedDialect(String),
    #[error("missing artifact {0}")]
    MissingArtifact(String),
    #[error("digest mismatch for {0}")]
    DigestMismatch(String),
    #[error("adapter cannot preserve required fidelity: {0}")]
    FidelityLoss(String),
    #[error("invalid portable JSON: {0}")]
    Json(String),
}

fn valid_identity(kind: &str, value: &str) -> Result<(), RevisionError> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(RevisionError::InvalidIdentity {
            kind: kind.into(),
            value: value.into(),
        });
    }
    Ok(())
}

macro_rules! identity {
    ($name:ident) => {
        #[derive(
            Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
        )]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);
        impl $name {
            pub fn new(value: impl Into<String>) -> Result<Self, RevisionError> {
                Self::try_from(value.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl TryFrom<String> for $name {
            type Error = RevisionError;
            fn try_from(value: String) -> Result<Self, Self::Error> {
                valid_identity(stringify!($name), &value)?;
                Ok(Self(value))
            }
        }
        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

identity!(ProjectId);
identity!(RevisionId);
identity!(BranchId);
identity!(OperationId);
identity!(ActorId);

/// A canonical lowercase SHA-256 address; algorithms are explicit at the boundary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactDigest(String);

impl ArtifactDigest {
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("sha256:{:x}", Sha256::digest(bytes)))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArtifactDigest {
    type Error = RevisionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        let valid = value.strip_prefix("sha256:").is_some_and(|hex| {
            hex.len() == 64
                && hex
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        });
        if !valid {
            return Err(RevisionError::InvalidIdentity {
                kind: "ArtifactDigest".into(),
                value,
            });
        }
        Ok(Self(value))
    }
}

impl From<ArtifactDigest> for String {
    fn from(value: ArtifactDigest) -> Self {
        value.0
    }
}

/// Portable relative artifact names, independent of the host's path rules.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(try_from = "String", into = "String")]
pub struct ArtifactPath(String);

impl ArtifactPath {
    pub fn new(value: impl Into<String>) -> Result<Self, RevisionError> {
        Self::try_from(value.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for ArtifactPath {
    type Error = RevisionError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        valid_identity("ArtifactPath", &value)?;
        if value.contains(['\\', ':'])
            || value
                .split('/')
                .any(|s| s.is_empty() || s == "." || s == "..")
        {
            return Err(RevisionError::InvalidIdentity {
                kind: "ArtifactPath".into(),
                value,
            });
        }
        Ok(Self(value))
    }
}

impl From<ArtifactPath> for String {
    fn from(value: ArtifactPath) -> Self {
        value.0
    }
}

/// Absence of an expected head means an explicitly empty branch, not an unchecked write.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "revision",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ExpectedHead {
    Empty,
    Revision(RevisionId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TypeRef {
    Boolean,
    Integer,
    Natural,
    Real,
    String,
    Timestamp,
    Element(ElementId),
    List(Box<TypeRef>),
    External {
        library: String,
        qualified_name: String,
    },
}

/// Decimal and timestamp lexemes retain source fidelity instead of float rounding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(
    tag = "kind",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum TypedValue {
    Boolean(bool),
    Integer(i64),
    Natural(u64),
    Real(String),
    String(String),
    Timestamp(String),
    Reference(ElementId),
    List(Vec<TypedValue>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Multiplicity {
    pub lower: u64,
    pub upper: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TypedProperty {
    pub type_ref: TypeRef,
    pub multiplicity: Multiplicity,
    /// None is an unassigned model attribute, distinct from an assigned empty collection.
    pub values: Option<Vec<TypedValue>>,
}

/// Bind a preserved source coordinate to the exact bytes included in the bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SourceEvidence {
    pub artifact: ArtifactPath,
    pub anchor: SourceAnchor,
    pub source_revision: Option<RevisionId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelElement {
    pub id: ElementId,
    pub kind: ElementKind,
    pub name: String,
    pub properties: BTreeMap<String, TypedProperty>,
    pub anchors: Vec<SourceEvidence>,
    /// Adapter-specific data survives in an explicit envelope, never implicit domain fields.
    pub extensions: BTreeMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FactAuthority {
    Authored,
    Inferred,
    CompilerProven,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ModelRelation {
    pub id: ElementId,
    pub relation: Relation,
    pub authority: FactAuthority,
    pub anchors: Vec<SourceEvidence>,
    /// Inferred facts must identify the rule that produced them.
    pub rule: Option<String>,
    pub extensions: BTreeMap<String, serde_json::Value>,
}

/// String map keys must exactly equal each value's existing ElementId.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortableModel {
    pub elements: BTreeMap<String, ModelElement>,
    pub relations: BTreeMap<String, ModelRelation>,
}

fn invalid(path: impl Into<String>, reason: impl Into<String>) -> RevisionError {
    RevisionError::InvalidModel {
        path: path.into(),
        reason: reason.into(),
    }
}

fn endpoints(relation: &Relation) -> Vec<&ElementId> {
    relation.endpoints()
}

impl PortableModel {
    pub fn validate(&self) -> Result<(), RevisionError> {
        let mut owners = BTreeMap::new();
        for (key, element) in &self.elements {
            valid_identity("ElementId", key)?;
            if key != element.id.as_str() || self.relations.contains_key(key) {
                return Err(invalid(key, "duplicate or mismatched identity"));
            }
            for (name, property) in &element.properties {
                let path = format!("{key}/properties/{name}");
                valid_identity("PropertyName", name)?;
                if property
                    .multiplicity
                    .upper
                    .is_some_and(|upper| upper < property.multiplicity.lower)
                {
                    return Err(invalid(&path, "upper multiplicity is below lower"));
                }
                self.validate_type(&property.type_ref, &path)?;
                if let Some(values) = &property.values {
                    let count = values.len() as u64;
                    if count < property.multiplicity.lower
                        || property
                            .multiplicity
                            .upper
                            .is_some_and(|upper| count > upper)
                    {
                        return Err(invalid(&path, "assigned values violate multiplicity"));
                    }
                    for value in values {
                        self.validate_value(&property.type_ref, value, &path)?;
                    }
                }
            }
        }
        for (key, fact) in &self.relations {
            valid_identity("RelationId", key)?;
            if key != fact.id.as_str() {
                return Err(invalid(key, "mismatched identity"));
            }
            if fact.authority == FactAuthority::Inferred
                && fact.rule.as_ref().is_none_or(|r| r.trim().is_empty())
            {
                return Err(invalid(key, "inferred fact has no rule"));
            }
            if fact.authority == FactAuthority::CompilerProven && fact.anchors.is_empty() {
                return Err(invalid(key, "compiler-proven fact has no source evidence"));
            }
            for endpoint in endpoints(&fact.relation) {
                if !self.elements.contains_key(endpoint.as_str()) {
                    return Err(invalid(key, format!("missing endpoint {endpoint}")));
                }
            }
            if let Relation::FeatureMembership { owner, member } = &fact.relation {
                if owners.insert(member.as_str(), owner.as_str()).is_some() {
                    return Err(invalid(key, "member has multiple owning memberships"));
                }
            }
            if let Relation::Satisfy { requirement, .. } | Relation::Verify { requirement, .. } =
                &fact.relation
            {
                if !matches!(
                    self.elements[requirement.as_str()].kind,
                    ElementKind::RequirementDefinition | ElementKind::RequirementUsage
                ) {
                    return Err(invalid(key, "requirement endpoint is not a requirement"));
                }
            }
            if let Relation::Connection { ends } = &fact.relation {
                if ends.len() < 2 {
                    return Err(invalid(key, "connection needs at least two ends"));
                }
            }
        }
        for member in owners.keys() {
            let mut visited = std::collections::BTreeSet::new();
            let mut current = *member;
            while let Some(owner) = owners.get(current) {
                if !visited.insert(current) {
                    return Err(invalid(*member, "owning membership cycle"));
                }
                current = owner;
            }
        }
        Ok(())
    }

    fn validate_type(&self, ty: &TypeRef, path: &str) -> Result<(), RevisionError> {
        match ty {
            TypeRef::Element(id) if !self.elements.contains_key(id.as_str()) => {
                Err(invalid(path, "missing type reference"))
            }
            TypeRef::List(item) => self.validate_type(item, path),
            TypeRef::External {
                library,
                qualified_name,
            } => {
                valid_identity("Library", library)?;
                valid_identity("QualifiedName", qualified_name)
            }
            _ => Ok(()),
        }
    }

    fn validate_value(
        &self,
        ty: &TypeRef,
        value: &TypedValue,
        path: &str,
    ) -> Result<(), RevisionError> {
        let valid = match (ty, value) {
            (TypeRef::Boolean, TypedValue::Boolean(_))
            | (TypeRef::Integer, TypedValue::Integer(_))
            | (TypeRef::Natural, TypedValue::Natural(_))
            | (TypeRef::String, TypedValue::String(_)) => true,
            (TypeRef::Real, TypedValue::Real(v)) => v.parse::<bigdecimal::BigDecimal>().is_ok(),
            (TypeRef::Timestamp, TypedValue::Timestamp(v)) => {
                chrono::DateTime::parse_from_rfc3339(v).is_ok()
            }
            (TypeRef::Element(_) | TypeRef::External { .. }, TypedValue::Reference(id)) => {
                self.elements.contains_key(id.as_str())
            }
            (TypeRef::List(item), TypedValue::List(values)) => {
                for value in values {
                    self.validate_value(item, value, path)?;
                }
                true
            }
            _ => false,
        };
        if !valid {
            return Err(invalid(
                path,
                "value does not match declared type or has unresolved reference",
            ));
        }
        Ok(())
    }

    pub fn semantic_digest(&self) -> Result<ArtifactDigest, RevisionError> {
        self.validate()?;
        Ok(ArtifactDigest::of(&canonical_bytes(self)?))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FidelityKind {
    Unsupported,
    Dropped,
    Unresolved,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FidelityIssue {
    pub path: String,
    pub kind: FidelityKind,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct FidelityReport {
    pub issues: Vec<FidelityIssue>,
}

impl FidelityReport {
    pub fn require_lossless(&self) -> Result<(), RevisionError> {
        if let Some(issue) = self.issues.first() {
            return Err(RevisionError::FidelityLoss(format!(
                "{}: {}",
                issue.path, issue.reason
            )));
        }
        Ok(())
    }
}

/// Capabilities must come from a verified adapter probe, not assumed server branding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AdapterCapabilities {
    pub model_dialects: Vec<String>,
    pub element_kinds: Vec<ElementKind>,
    /// Canonical snake_case names of the existing Relation variants.
    pub relation_kinds: Vec<String>,
    pub preserves_extensions: bool,
    pub preserves_source_evidence: bool,
    /// Missing capability claims fail closed, including older probe records.
    #[serde(default)]
    pub preserves_properties: bool,
    #[serde(default)]
    pub preserves_fact_authority: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RevisionContext {
    pub project: ProjectId,
    pub revision: RevisionId,
    pub parent: Option<RevisionId>,
    pub model_dialect: String,
    pub library_revisions: BTreeMap<String, RevisionId>,
    pub source_revisions: BTreeMap<String, RevisionId>,
    pub toolchain: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct BundleManifest {
    pub dialect: String,
    pub context: RevisionContext,
    pub semantic_digest: ArtifactDigest,
    pub artifacts: BTreeMap<ArtifactPath, ArtifactDigest>,
    pub fidelity: FidelityReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortableBundle {
    pub manifest: BundleManifest,
    pub model: PortableModel,
    pub blobs: BTreeMap<ArtifactDigest, Vec<u8>>,
}

impl PortableBundle {
    pub fn dehydrate(
        model: PortableModel,
        context: RevisionContext,
        artifacts: BTreeMap<ArtifactPath, Vec<u8>>,
    ) -> Result<Self, RevisionError> {
        let mut blobs = BTreeMap::new();
        let mut artifact_index = BTreeMap::new();
        for (path, bytes) in artifacts {
            let digest = ArtifactDigest::of(&bytes);
            artifact_index.insert(path, digest.clone());
            blobs.insert(digest, bytes);
        }
        let bundle = Self {
            manifest: BundleManifest {
                dialect: DIALECT.into(),
                context,
                semantic_digest: model.semantic_digest()?,
                artifacts: artifact_index,
                fidelity: FidelityReport::default(),
            },
            model,
            blobs,
        };
        bundle.validate()?;
        Ok(bundle)
    }

    pub fn validate(&self) -> Result<(), RevisionError> {
        if self.manifest.dialect != DIALECT {
            return Err(RevisionError::UnsupportedDialect(
                self.manifest.dialect.clone(),
            ));
        }
        valid_identity("ModelDialect", &self.manifest.context.model_dialect)?;
        if self.manifest.context.parent.as_ref() == Some(&self.manifest.context.revision) {
            return Err(invalid(
                "context/parent",
                "revision cannot be its own parent",
            ));
        }
        self.manifest.fidelity.require_lossless()?;
        if self.model.semantic_digest()? != self.manifest.semantic_digest {
            return Err(RevisionError::DigestMismatch("semantic model".into()));
        }
        for (path, digest) in &self.manifest.artifacts {
            let bytes = self
                .blobs
                .get(digest)
                .ok_or_else(|| RevisionError::MissingArtifact(path.as_str().into()))?;
            if &ArtifactDigest::of(bytes) != digest {
                return Err(RevisionError::DigestMismatch(path.as_str().into()));
            }
        }
        for (digest, bytes) in &self.blobs {
            if &ArtifactDigest::of(bytes) != digest {
                return Err(RevisionError::DigestMismatch(digest.as_str().into()));
            }
            if !self.manifest.artifacts.values().any(|d| d == digest) {
                return Err(invalid("blobs", "unreferenced artifact"));
            }
        }
        for element in self.model.elements.values() {
            for property in element.properties.values() {
                check_library(&property.type_ref, &self.manifest.context.library_revisions)?;
            }
        }
        for evidence in self
            .model
            .elements
            .values()
            .flat_map(|e| &e.anchors)
            .chain(self.model.relations.values().flat_map(|r| &r.anchors))
        {
            if !self.manifest.artifacts.contains_key(&evidence.artifact) {
                return Err(RevisionError::MissingArtifact(
                    evidence.artifact.as_str().into(),
                ));
            }
            match &evidence.anchor {
                SourceAnchor::RustSpan { line, col, .. } if *line == 0 || *col == Some(0) => {
                    return Err(invalid("source evidence", "source spans are one-based"));
                }
                SourceAnchor::SysmlFile { line: Some(0), .. } => {
                    return Err(invalid("source evidence", "source spans are one-based"));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Validate before constructing a hydration candidate; no model-server side effects.
    pub fn hydrate(&self) -> Result<PortableModel, RevisionError> {
        self.validate()?;
        Ok(self.model.clone())
    }

    /// Describe precisely what an adapter cannot retain before allowing a model write.
    pub fn adapter_fidelity(&self, capabilities: &AdapterCapabilities) -> FidelityReport {
        let mut issues = Vec::new();
        if !capabilities
            .model_dialects
            .contains(&self.manifest.context.model_dialect)
        {
            issues.push(FidelityIssue {
                path: "context/model_dialect".into(),
                kind: FidelityKind::Unsupported,
                reason: "adapter has not verified this model dialect".into(),
            });
        }
        for (id, element) in &self.model.elements {
            let path = format!("elements/{id}");
            if !capabilities.preserves_properties && !element.properties.is_empty() {
                issues.push(FidelityIssue {
                    path: format!("{path}/properties"),
                    kind: FidelityKind::Dropped,
                    reason: "adapter has not verified typed property preservation".into(),
                });
            }
            if !capabilities.element_kinds.contains(&element.kind) {
                issues.push(FidelityIssue {
                    path: path.clone(),
                    kind: FidelityKind::Unsupported,
                    reason: format!("unsupported element kind {}", element.kind.kerml_name()),
                });
            }
            if !capabilities.preserves_extensions && !element.extensions.is_empty() {
                issues.push(FidelityIssue {
                    path: format!("{path}/extensions"),
                    kind: FidelityKind::Dropped,
                    reason: "adapter discards extension envelope".into(),
                });
            }
            if !capabilities.preserves_source_evidence && !element.anchors.is_empty() {
                issues.push(FidelityIssue {
                    path: format!("{path}/anchors"),
                    kind: FidelityKind::Dropped,
                    reason: "adapter discards source evidence".into(),
                });
            }
        }
        for (id, fact) in &self.model.relations {
            let path = format!("relations/{id}");
            if !capabilities.preserves_fact_authority {
                issues.push(FidelityIssue {
                    path: format!("{path}/authority"),
                    kind: FidelityKind::Dropped,
                    reason:
                        "adapter has not verified authored/inferred/compiler authority preservation"
                            .into(),
                });
            }
            let encoded = serde_json::to_value(&fact.relation)
                .expect("Relation has a finite string-only wire shape");
            let kind = encoded
                .as_object()
                .and_then(|o| o.keys().next())
                .expect("Relation is externally tagged");
            if !capabilities.relation_kinds.contains(kind) {
                issues.push(FidelityIssue {
                    path: path.clone(),
                    kind: FidelityKind::Unsupported,
                    reason: format!("unsupported relation kind {kind}"),
                });
            }
            if !capabilities.preserves_extensions && !fact.extensions.is_empty() {
                issues.push(FidelityIssue {
                    path: format!("{path}/extensions"),
                    kind: FidelityKind::Dropped,
                    reason: "adapter discards extension envelope".into(),
                });
            }
            if !capabilities.preserves_source_evidence && !fact.anchors.is_empty() {
                issues.push(FidelityIssue {
                    path: format!("{path}/anchors"),
                    kind: FidelityKind::Dropped,
                    reason: "adapter discards source evidence".into(),
                });
            }
        }
        FidelityReport { issues }
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, RevisionError> {
        self.validate()?;
        canonical_bytes(self)
    }

    /// Bind complete context and exact source/attachment bytes for intent/idempotency checks.
    pub fn bundle_digest(&self) -> Result<ArtifactDigest, RevisionError> {
        Ok(ArtifactDigest::of(&self.to_bytes()?))
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RevisionError> {
        let input: UniqueJson =
            serde_json::from_slice(bytes).map_err(|e| RevisionError::Json(e.to_string()))?;
        let bundle: Self = serde_json::from_value(input.0.clone())
            .map_err(|e| RevisionError::Json(e.to_string()))?;
        let encoded =
            serde_json::to_value(&bundle).map_err(|e| RevisionError::Json(e.to_string()))?;
        require_preserved_keys(&input.0, &encoded, "bundle")?;
        bundle.validate()?;
        Ok(bundle)
    }
}

fn require_preserved_keys(
    input: &serde_json::Value,
    encoded: &serde_json::Value,
    path: &str,
) -> Result<(), RevisionError> {
    match (input, encoded) {
        (serde_json::Value::Object(input), serde_json::Value::Object(encoded)) => {
            for (key, value) in input {
                let child = format!("{path}/{key}");
                let encoded = encoded
                    .get(key)
                    .ok_or_else(|| invalid(&child, "unknown field would be lost"))?;
                require_preserved_keys(value, encoded, &child)?;
            }
        }
        (serde_json::Value::Array(input), serde_json::Value::Array(encoded)) => {
            for (index, value) in input.iter().enumerate() {
                let encoded = encoded
                    .get(index)
                    .ok_or_else(|| invalid(path, "array value would be lost"))?;
                require_preserved_keys(value, encoded, &format!("{path}/{index}"))?;
            }
        }
        _ => {}
    }
    Ok(())
}

struct UniqueJson(serde_json::Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Self::Value, E> {
                let n = serde_json::Number::from_f64(v)
                    .ok_or_else(|| E::custom("non-finite JSON number"))?;
                Ok(UniqueJson(serde_json::Value::Number(n)))
            }
            fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut seq: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = seq.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(serde_json::Value::Array(values)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(serde::de::Error::custom(format!(
                            "duplicate JSON key {key:?}"
                        )));
                    }
                    let value = map.next_value::<UniqueJson>()?;
                    values.insert(key, value.0);
                }
                Ok(UniqueJson(serde_json::Value::Object(values)))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

fn check_library(
    ty: &TypeRef,
    libraries: &BTreeMap<String, RevisionId>,
) -> Result<(), RevisionError> {
    match ty {
        TypeRef::External { library, .. } if !libraries.contains_key(library) => Err(invalid(
            library,
            "external type library has no pinned revision",
        )),
        TypeRef::List(item) => check_library(item, libraries),
        _ => Ok(()),
    }
}

/// Sort every JSON object recursively, even when a consumer enables preserve_order.
pub fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>, RevisionError> {
    fn write(value: &serde_json::Value, out: &mut Vec<u8>) -> Result<(), serde_json::Error> {
        match value {
            serde_json::Value::Object(map) => {
                out.push(b'{');
                let mut entries: Vec<_> = map.iter().collect();
                entries.sort_by(|a, b| a.0.cmp(b.0));
                for (index, (key, value)) in entries.into_iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    serde_json::to_writer(&mut *out, key)?;
                    out.push(b':');
                    write(value, out)?;
                }
                out.push(b'}');
            }
            serde_json::Value::Array(values) => {
                out.push(b'[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        out.push(b',');
                    }
                    write(value, out)?;
                }
                out.push(b']');
            }
            _ => serde_json::to_writer(out, value)?,
        }
        Ok(())
    }
    let mut out = Vec::new();
    let value = serde_json::to_value(value).map_err(|e| RevisionError::Json(e.to_string()))?;
    write(&value, &mut out).map_err(|e| RevisionError::Json(e.to_string()))?;
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct IndexCheckpoint {
    pub project: ProjectId,
    pub revision: RevisionId,
    pub dialect: String,
    pub graph_digest: ArtifactDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SyncStatus {
    Pending,
    ModelCommitted { revision: RevisionId },
    Indexed { checkpoint: IndexCheckpoint },
    Conflict { paths: Vec<String> },
    Ambiguous { reason: String },
    Unavailable { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OperationReceipt {
    pub operation: OperationId,
    pub actor: ActorId,
    pub project: ProjectId,
    pub branch: BranchId,
    pub expected_head: ExpectedHead,
    pub proposal_digest: ArtifactDigest,
    pub status: SyncStatus,
}
