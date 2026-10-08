//! Deterministic native assertions backed by digest-addressed owner envelopes.
//! Property values, source evidence and authority are retained evidence, not
//! native evaluations. Action/state behavior and unresolved derivation refuse emission.
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use ufo_types::revision::{
    canonical_bytes, ArtifactDigest, FidelityIssue, FidelityKind, FidelityReport, PortableBundle,
};
use ufo_types::{ElementKind, Relation};

pub const MARKER_PREFIX: &str = "urn:ledgrrr:revision:1:";
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationMarker {
    pub project: String,
    pub branch: String,
    pub operation: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvelopeReference {
    pub marker: OperationMarker,
    pub envelope: ArtifactDigest,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    #[serde(rename = "@id")]
    pub id: String,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeElement {
    #[serde(rename = "@id")]
    pub id: String,
    #[serde(rename = "@type")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_name: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alias_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_definition: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verification_case_definition: Option<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub verified_requirement: Vec<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub satisfied_requirement: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub satisfying_feature: Option<Reference>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_feature: Option<Reference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub target_feature: Vec<Reference>,
    #[serde(flatten)]
    pub fields: BTreeMap<String, Value>,
}
impl NativeElement {
    fn new(id: String, kind: &str, name: Option<String>, aliases: Vec<String>) -> Self {
        Self {
            id,
            kind: kind.into(),
            declared_name: name,
            alias_ids: aliases,
            requirement_definition: None,
            verification_case_definition: None,
            verified_requirement: vec![],
            satisfied_requirement: None,
            satisfying_feature: None,
            source_feature: None,
            target_feature: vec![],
            fields: BTreeMap::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Projection {
    pub elements: Vec<NativeElement>,
    pub envelope_digest: ArtifactDigest,
    pub projection_digest: ArtifactDigest,
    pub identity_digest: ArtifactDigest,
    pub identity_map: BTreeMap<String, String>,
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("native fidelity failure: {0:?}")]
    Fidelity(FidelityReport),
    #[error("invalid native protocol: {0}")]
    Protocol(String),
    #[error(transparent)]
    Revision(#[from] ufo_types::revision::RevisionError),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
fn issue(path: impl Into<String>, reason: impl Into<String>) -> FidelityIssue {
    FidelityIssue {
        path: path.into(),
        kind: FidelityKind::Unsupported,
        reason: reason.into(),
    }
}
fn loss(path: &str, reason: &str) -> Error {
    Error::Fidelity(FidelityReport {
        issues: vec![issue(path, reason)],
    })
}

/// Explicit capability classifications; new upstream variants fail closed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Capability {
    pub canonical: String,
    pub native_supported: bool,
    pub scope: String,
}
pub fn capability_table() -> Vec<Capability> {
    let mut result: Vec<_> = ElementKind::ALL
        .iter()
        .map(|k| Capability {
            canonical: k.kerml_name().into(),
            native_supported: supported_element(*k),
            scope: if supported_element(*k) {
                "identity/name; owner-envelope properties/evidence"
            } else {
                "unverified required native semantics"
            }
            .into(),
        })
        .collect();
    for (kind, supported, scope) in [
        (
            "feature_membership",
            true,
            "singleton membershipOwningNamespace/memberElement",
        ),
        (
            "specialization",
            false,
            "generic specialization is not FeatureTyping",
        ),
        ("feature_typing", true, "one compatible usage-to-definition typing; real native singleton/array definition fields"),
        ("subsetting", false, "unverified"),
        ("redefinition", false, "unverified"),
        ("connection", false, "library/end semantics unverified"),
        ("succession", false, "behavior unverified"),
        (
            "allocation",
            true,
            "singleton sourceFeature/array targetFeature",
        ),
        (
            "satisfy",
            true,
            "RequirementUsage and native Feature singleton",
        ),
        (
            "verify",
            true,
            "VerificationCaseUsage verifiedRequirement array",
        ),
        ("refine", false, "unverified"),
        ("dependency", true, "client/supplier arrays"),
        (
            "domain",
            false,
            "resolved pinned derivation library unavailable",
        ),
    ] {
        result.push(Capability {
            canonical: kind.into(),
            native_supported: supported,
            scope: scope.into(),
        });
    }
    for value in [
        "boolean",
        "integer",
        "natural",
        "real",
        "string",
        "timestamp",
        "element",
        "external",
        "reference",
        "list",
    ] {
        result.push(Capability {
            canonical: format!("value/{value}"),
            native_supported: false,
            scope: "exact typed owner-envelope evidence; no native value evaluation".into(),
        });
    }
    result
}
fn supported_element(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::Package
            | ElementKind::PartDefinition
            | ElementKind::PartUsage
            | ElementKind::RequirementDefinition
            | ElementKind::RequirementUsage
            | ElementKind::VerificationCaseDefinition
            | ElementKind::VerificationCaseUsage
    )
}
fn native_feature(kind: ElementKind) -> bool {
    matches!(
        kind,
        ElementKind::PartUsage | ElementKind::RequirementUsage | ElementKind::VerificationCaseUsage
    )
}

/// Length-framed namespace and opaque identity prevent concatenation collisions.
pub fn provider_uuid(namespace: &str, opaque: &str) -> String {
    let mut h = blake3::Hasher::new();
    h.update(b"ledgrrr/native/uuid/1");
    for component in [namespace, opaque] {
        h.update(&(component.len() as u64).to_be_bytes());
        h.update(component.as_bytes());
    }
    let mut b = *h.finalize().as_bytes();
    b[6] = (b[6] & 15) | 0x50;
    b[8] = (b[8] & 63) | 0x80;
    let hex = b[..16]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}
pub fn emit(bundle: &PortableBundle, marker: &OperationMarker) -> Result<Projection> {
    bundle.validate()?;
    if marker.project != bundle.manifest.context.project.as_str() {
        return Err(loss(
            "marker/project",
            "operation marker does not bind the bundle's owner project",
        ));
    }
    for value in [&marker.project, &marker.branch, &marker.operation] {
        if value.trim().is_empty() || value.chars().any(char::is_control) {
            return Err(Error::Protocol("invalid operation marker".into()));
        }
    }
    let mut issues = vec![];
    if bundle.manifest.context.model_dialect != "SysML-v2" {
        issues.push(issue("context/model_dialect", "only SysML-v2 verified"));
    }
    for (id, e) in &bundle.model.elements {
        if !supported_element(e.kind) {
            issues.push(issue(
                format!("elements/{id}"),
                format!("native {} behavior unavailable", e.kind.kerml_name()),
            ));
        }
        if e.extensions.contains_key("native_required") {
            issues.push(issue(
                format!("elements/{id}/native_required"),
                "native property/behavior claims unavailable",
            ));
        }
    }
    let mut typed_features = BTreeSet::new();
    for (id, r) in &bundle.model.relations {
        let allowed = match &r.relation {
            Relation::Verify { requirement, by } => {
                bundle.model.elements[requirement.as_str()].kind == ElementKind::RequirementUsage
                    && bundle.model.elements[by.as_str()].kind == ElementKind::VerificationCaseUsage
            }
            Relation::Satisfy {
                requirement,
                subject,
            } => {
                bundle.model.elements[requirement.as_str()].kind == ElementKind::RequirementUsage
                    && native_feature(bundle.model.elements[subject.as_str()].kind)
            }
            Relation::Allocation { source, target } => {
                native_feature(bundle.model.elements[source.as_str()].kind)
                    && native_feature(bundle.model.elements[target.as_str()].kind)
            }
            Relation::FeatureMembership { owner, member } => {
                bundle.model.elements[owner.as_str()].kind == ElementKind::Package
                    && native_feature(bundle.model.elements[member.as_str()].kind)
            }
            Relation::FeatureTyping { feature, type_ } => {
                let compatible = bundle.model.elements[feature.as_str()].kind.definition_of()
                    == Some(bundle.model.elements[type_.as_str()].kind);
                compatible && typed_features.insert(feature.as_str())
            }
            Relation::Specialization { .. } => false,
            Relation::Dependency { .. } => true,
            _ => false,
        };
        if !allowed {
            issues.push(issue(format!("relations/{id}"),"unsupported native relation or endpoint kinds; derivation requires resolved pinned library"));
        }
    }
    if !issues.is_empty() {
        return Err(Error::Fidelity(FidelityReport { issues }));
    }
    let envelope_digest = bundle.bundle_digest()?;
    let reference = EnvelopeReference {
        marker: marker.clone(),
        envelope: envelope_digest.clone(),
    };
    let alias = format!("{MARKER_PREFIX}{}", serde_json::to_string(&reference)?);
    let namespace = serde_json::to_string(&(marker.project.as_str(), marker.branch.as_str()))?;
    let mut identity_map = BTreeMap::new();
    let mut rows = BTreeMap::new();
    for (id, e) in &bundle.model.elements {
        let uuid = provider_uuid(&namespace, &format!("element:{id}"));
        identity_map.insert(id.clone(), uuid.clone());
        rows.insert(
            id.clone(),
            NativeElement::new(
                uuid,
                e.kind.kerml_name(),
                Some(e.name.clone()),
                vec![
                    alias.clone(),
                    format!("urn:ledgrrr:original:1:{}", serde_json::to_string(id)?),
                ],
            ),
        );
    }
    let native_ref = |id: &ufo_types::ElementId| Reference {
        id: identity_map[id.as_str()].clone(),
    };
    let mut relation_rows = vec![];
    for (id, r) in &bundle.model.relations {
        let uuid = provider_uuid(&namespace, &format!("relation:{id}"));
        let aliases = vec![
            alias.clone(),
            format!("urn:ledgrrr:original:1:{}", serde_json::to_string(id)?),
        ];
        let mut row = NativeElement::new(uuid.clone(), r.relation.kerml_name(), None, aliases);
        match &r.relation {
            Relation::Verify { requirement, by } => {
                rows.get_mut(by.as_str())
                    .ok_or_else(|| loss(id, "missing verification"))?
                    .verified_requirement
                    .push(native_ref(requirement));
                continue;
            }
            Relation::Satisfy {
                requirement,
                subject,
            } => {
                row.kind = "SatisfyRequirementUsage".into();
                row.satisfied_requirement = Some(native_ref(requirement));
                row.satisfying_feature = Some(native_ref(subject));
            }
            Relation::Allocation { source, target } => {
                row.kind = "AllocationUsage".into();
                row.source_feature = Some(native_ref(source));
                row.target_feature = vec![native_ref(target)];
            }
            Relation::Dependency { client, supplier } => {
                row.fields
                    .insert("client".into(), json!([native_ref(client)]));
                row.fields
                    .insert("supplier".into(), json!([native_ref(supplier)]));
            }
            Relation::FeatureMembership { owner, member } => {
                row.fields
                    .insert("membershipOwningNamespace".into(), json!(native_ref(owner)));
                row.fields
                    .insert("memberElement".into(), json!(native_ref(member)));
            }
            Relation::FeatureTyping { feature, type_ } => {
                row.fields
                    .insert("typedFeature".into(), json!(native_ref(feature)));
                row.fields.insert("type".into(), json!(native_ref(type_)));
                let usage = rows
                    .get_mut(feature.as_str())
                    .ok_or_else(|| loss(id, "missing typed usage"))?;
                match bundle.model.elements[feature.as_str()].kind {
                    ElementKind::RequirementUsage => {
                        usage.requirement_definition = Some(native_ref(type_))
                    }
                    ElementKind::VerificationCaseUsage => {
                        usage.verification_case_definition = Some(native_ref(type_))
                    }
                    ElementKind::PartUsage => {
                        usage
                            .fields
                            .insert("partDefinition".into(), json!([native_ref(type_)]));
                    }
                    _ => return Err(loss(id, "unsupported native typed usage")),
                }
            }
            _ => return Err(loss(id, "unsupported relation")),
        }
        relation_rows.push(row);
    }
    // Verify has no standalone native row; preserve and bind its canonical identity separately.
    for id in bundle.model.relations.keys() {
        identity_map.insert(
            id.clone(),
            provider_uuid(&namespace, &format!("relation:{id}")),
        );
    }
    let mut elements: Vec<_> = rows.into_values().chain(relation_rows).collect();
    let anchor = NativeElement::new(
        provider_uuid(&namespace, "reserved:envelope-anchor"),
        "Package",
        Some("ledgrrr_revision_envelope_v1".into()),
        vec![alias],
    );
    elements.push(anchor);
    elements.sort_by(|a, b| a.id.cmp(&b.id));
    for e in &mut elements {
        e.verified_requirement.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let mut seen = BTreeSet::new();
    if elements.iter().any(|e| !seen.insert(e.id.clone())) {
        return Err(Error::Protocol("provider identity collision".into()));
    }
    Ok(Projection {
        projection_digest: ArtifactDigest::of(&canonical_bytes(&elements)?),
        identity_digest: ArtifactDigest::of(&canonical_bytes(&identity_map)?),
        envelope_digest,
        identity_map,
        elements,
    })
}

pub fn marker_from_elements(elements: &[NativeElement]) -> Result<EnvelopeReference> {
    let anchor = elements
        .iter()
        .find(|e| {
            e.kind == "Package"
                && e.declared_name.as_deref() == Some("ledgrrr_revision_envelope_v1")
        })
        .ok_or_else(|| loss("anchor", "missing native envelope anchor"))?;
    let aliases: Vec<_> = anchor
        .alias_ids
        .iter()
        .filter_map(|a| a.strip_prefix(MARKER_PREFIX))
        .collect();
    if aliases.len() != 1 {
        return Err(loss("anchor", "missing or duplicate envelope marker"));
    }
    Ok(serde_json::from_str(aliases[0])?)
}

/// Compare complete independently fetched native content with deterministic assertions.
pub fn hydrate(
    elements: &[NativeElement],
    envelope_bytes: &[u8],
    marker: &OperationMarker,
) -> Result<PortableBundle> {
    let bundle = PortableBundle::from_bytes(envelope_bytes)?;
    let expected = emit(&bundle, marker)?;
    if ArtifactDigest::of(envelope_bytes) != expected.envelope_digest {
        return Err(loss(
            "envelope",
            "envelope bytes do not match their canonical native digest",
        ));
    }
    let reference = marker_from_elements(elements)?;
    if reference.marker != *marker || reference.envelope != expected.envelope_digest {
        return Err(loss("anchor", "operation/envelope mismatch"));
    }
    let mut actual = elements.to_vec();
    let mut ids = BTreeSet::new();
    for row in &mut actual {
        if !ids.insert(row.id.clone()) {
            return Err(loss(&row.id, "duplicate native identity"));
        }
        let mut aliases = BTreeSet::new();
        if row.alias_ids.iter().any(|a| !aliases.insert(a.clone())) {
            return Err(loss(&row.id, "duplicate native alias"));
        }
        normalize_defaults(row)?;
        row.verified_requirement.sort_by(|a, b| a.id.cmp(&b.id));
    }
    actual.sort_by(|a, b| a.id.cmp(&b.id));
    if actual != expected.elements {
        let expected_rows: BTreeMap<_, _> = expected
            .elements
            .iter()
            .map(|e| (e.id.as_str(), e))
            .collect();
        let actual_rows: BTreeMap<_, _> = actual.iter().map(|e| (e.id.as_str(), e)).collect();
        let mut issues = Vec::new();
        for (id, row) in &expected_rows {
            let Some(observed) = actual_rows.get(id) else {
                issues.push(FidelityIssue {
                    path: format!("native/{id}"),
                    kind: FidelityKind::Dropped,
                    reason: "missing managed native identity".into(),
                });
                continue;
            };
            if row != observed {
                let expected_value = serde_json::to_value(row)?;
                let observed_value = serde_json::to_value(observed)?;
                if let (Some(expected), Some(observed)) =
                    (expected_value.as_object(), observed_value.as_object())
                {
                    let keys: BTreeSet<_> = expected.keys().chain(observed.keys()).collect();
                    for key in keys {
                        if expected.get(key) != observed.get(key) {
                            let kind = if !observed.contains_key(key) {
                                FidelityKind::Dropped
                            } else if !expected.contains_key(key) {
                                FidelityKind::Unsupported
                            } else {
                                FidelityKind::Unresolved
                            };
                            issues.push(FidelityIssue {
                                path: format!("native/{id}/{key}"),
                                kind,
                                reason: "native field differs from the accepted owner projection"
                                    .into(),
                            });
                        }
                    }
                }
            }
        }
        for id in actual_rows.keys() {
            if !expected_rows.contains_key(id) {
                issues.push(FidelityIssue {
                    path: format!("native/{id}"),
                    kind: FidelityKind::Unresolved,
                    reason: "unmanaged native identity has no owner envelope representation".into(),
                });
            }
        }
        return Err(Error::Fidelity(FidelityReport { issues }));
    }
    Ok(bundle)
}
fn normalize_defaults(row: &mut NativeElement) -> Result<()> {
    // Only documented generated defaults/metadata are ignored. Unknown fields,
    // even empty ones, remain content and prevent an equivalence assertion.
    // Explicit fields from official reference 0af711b / OMG 20250201 schema.
    let empty_fields: &[&str] = match row.kind.as_str() {
        "Package" => &[
            "declaredShortName",
            "documentation",
            "elementId",
            "filterCondition",
            "importedMembership",
            "isImpliedIncluded",
            "isLibraryElement",
            "member",
            "membership",
            "name",
            "ownedAnnotation",
            "ownedElement",
            "ownedImport",
            "ownedMember",
            "ownedMembership",
            "ownedRelationship",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "qualifiedName",
            "shortName",
            "textualRepresentation",
        ],
        "PartDefinition" => &[
            "declaredShortName",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "documentation",
            "elementId",
            "endFeature",
            "feature",
            "featureMembership",
            "importedMembership",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isConjugated",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isSufficient",
            "isVariation",
            "member",
            "membership",
            "multiplicity",
            "name",
            "output",
            "ownedAction",
            "ownedAllocation",
            "ownedAnalysisCase",
            "ownedAnnotation",
            "ownedAttribute",
            "ownedCalculation",
            "ownedCase",
            "ownedConcern",
            "ownedConjugator",
            "ownedConnection",
            "ownedConstraint",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedEnumeration",
            "ownedFeature",
            "ownedFeatureMembership",
            "ownedFlow",
            "ownedImport",
            "ownedInterface",
            "ownedIntersecting",
            "ownedItem",
            "ownedMember",
            "ownedMembership",
            "ownedMetadata",
            "ownedOccurrence",
            "ownedPart",
            "ownedPort",
            "ownedReference",
            "ownedRelationship",
            "ownedRendering",
            "ownedRequirement",
            "ownedSpecialization",
            "ownedState",
            "ownedSubclassification",
            "ownedTransition",
            "ownedUnioning",
            "ownedUsage",
            "ownedUseCase",
            "ownedVerificationCase",
            "ownedView",
            "ownedViewpoint",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "qualifiedName",
            "shortName",
            "textualRepresentation",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "PartUsage" => &[
            "chainingFeature",
            "crossFeature",
            "declaredShortName",
            "definition",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "direction",
            "documentation",
            "elementId",
            "endFeature",
            "endOwningType",
            "feature",
            "featureMembership",
            "featureTarget",
            "featuringType",
            "importedMembership",
            "individualDefinition",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isComposite",
            "isConjugated",
            "isConstant",
            "isDerived",
            "isEnd",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isOrdered",
            "isPortion",
            "isReference",
            "isSufficient",
            "isUnique",
            "isVariable",
            "isVariation",
            "itemDefinition",
            "mayTimeVary",
            "member",
            "membership",
            "multiplicity",
            "name",
            "nestedAction",
            "nestedAllocation",
            "nestedAnalysisCase",
            "nestedAttribute",
            "nestedCalculation",
            "nestedCase",
            "nestedConcern",
            "nestedConnection",
            "nestedConstraint",
            "nestedEnumeration",
            "nestedFlow",
            "nestedInterface",
            "nestedItem",
            "nestedMetadata",
            "nestedOccurrence",
            "nestedPart",
            "nestedPort",
            "nestedReference",
            "nestedRendering",
            "nestedRequirement",
            "nestedState",
            "nestedTransition",
            "nestedUsage",
            "nestedUseCase",
            "nestedVerificationCase",
            "nestedView",
            "nestedViewpoint",
            "occurrenceDefinition",
            "output",
            "ownedAnnotation",
            "ownedConjugator",
            "ownedCrossSubsetting",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedFeature",
            "ownedFeatureChaining",
            "ownedFeatureInverting",
            "ownedFeatureMembership",
            "ownedImport",
            "ownedIntersecting",
            "ownedMember",
            "ownedMembership",
            "ownedRedefinition",
            "ownedReferenceSubsetting",
            "ownedRelationship",
            "ownedSpecialization",
            "ownedSubsetting",
            "ownedTypeFeaturing",
            "ownedTyping",
            "ownedUnioning",
            "owner",
            "owningDefinition",
            "owningFeatureMembership",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "owningType",
            "owningUsage",
            "partDefinition",
            "portionKind",
            "qualifiedName",
            "shortName",
            "textualRepresentation",
            "type",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "RequirementDefinition" => &[
            "actorParameter",
            "assumedConstraint",
            "declaredShortName",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "documentation",
            "elementId",
            "endFeature",
            "expression",
            "feature",
            "featureMembership",
            "framedConcern",
            "importedMembership",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isConjugated",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isModelLevelEvaluable",
            "isSufficient",
            "isVariation",
            "member",
            "membership",
            "multiplicity",
            "name",
            "output",
            "ownedAction",
            "ownedAllocation",
            "ownedAnalysisCase",
            "ownedAnnotation",
            "ownedAttribute",
            "ownedCalculation",
            "ownedCase",
            "ownedConcern",
            "ownedConjugator",
            "ownedConnection",
            "ownedConstraint",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedEnumeration",
            "ownedFeature",
            "ownedFeatureMembership",
            "ownedFlow",
            "ownedImport",
            "ownedInterface",
            "ownedIntersecting",
            "ownedItem",
            "ownedMember",
            "ownedMembership",
            "ownedMetadata",
            "ownedOccurrence",
            "ownedPart",
            "ownedPort",
            "ownedReference",
            "ownedRelationship",
            "ownedRendering",
            "ownedRequirement",
            "ownedSpecialization",
            "ownedState",
            "ownedSubclassification",
            "ownedTransition",
            "ownedUnioning",
            "ownedUsage",
            "ownedUseCase",
            "ownedVerificationCase",
            "ownedView",
            "ownedViewpoint",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "parameter",
            "qualifiedName",
            "reqId",
            "requiredConstraint",
            "result",
            "shortName",
            "stakeholderParameter",
            "step",
            "subjectParameter",
            "text",
            "textualRepresentation",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "RequirementUsage" => &[
            "actorParameter",
            "assumedConstraint",
            "behavior",
            "chainingFeature",
            "constraintDefinition",
            "crossFeature",
            "declaredShortName",
            "definition",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "direction",
            "documentation",
            "elementId",
            "endFeature",
            "endOwningType",
            "feature",
            "featureMembership",
            "featureTarget",
            "featuringType",
            "framedConcern",
            "function",
            "importedMembership",
            "individualDefinition",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isComposite",
            "isConjugated",
            "isConstant",
            "isDerived",
            "isEnd",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isModelLevelEvaluable",
            "isOrdered",
            "isPortion",
            "isReference",
            "isSufficient",
            "isUnique",
            "isVariable",
            "isVariation",
            "mayTimeVary",
            "member",
            "membership",
            "multiplicity",
            "name",
            "nestedAction",
            "nestedAllocation",
            "nestedAnalysisCase",
            "nestedAttribute",
            "nestedCalculation",
            "nestedCase",
            "nestedConcern",
            "nestedConnection",
            "nestedConstraint",
            "nestedEnumeration",
            "nestedFlow",
            "nestedInterface",
            "nestedItem",
            "nestedMetadata",
            "nestedOccurrence",
            "nestedPart",
            "nestedPort",
            "nestedReference",
            "nestedRendering",
            "nestedRequirement",
            "nestedState",
            "nestedTransition",
            "nestedUsage",
            "nestedUseCase",
            "nestedVerificationCase",
            "nestedView",
            "nestedViewpoint",
            "occurrenceDefinition",
            "output",
            "ownedAnnotation",
            "ownedConjugator",
            "ownedCrossSubsetting",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedFeature",
            "ownedFeatureChaining",
            "ownedFeatureInverting",
            "ownedFeatureMembership",
            "ownedImport",
            "ownedIntersecting",
            "ownedMember",
            "ownedMembership",
            "ownedRedefinition",
            "ownedReferenceSubsetting",
            "ownedRelationship",
            "ownedSpecialization",
            "ownedSubsetting",
            "ownedTypeFeaturing",
            "ownedTyping",
            "ownedUnioning",
            "owner",
            "owningDefinition",
            "owningFeatureMembership",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "owningType",
            "owningUsage",
            "parameter",
            "portionKind",
            "predicate",
            "qualifiedName",
            "reqId",
            "requiredConstraint",
            "result",
            "shortName",
            "stakeholderParameter",
            "subjectParameter",
            "text",
            "textualRepresentation",
            "type",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "VerificationCaseDefinition" => &[
            "action",
            "actorParameter",
            "calculation",
            "declaredShortName",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "documentation",
            "elementId",
            "endFeature",
            "expression",
            "feature",
            "featureMembership",
            "importedMembership",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isConjugated",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isModelLevelEvaluable",
            "isSufficient",
            "isVariation",
            "member",
            "membership",
            "multiplicity",
            "name",
            "objectiveRequirement",
            "output",
            "ownedAction",
            "ownedAllocation",
            "ownedAnalysisCase",
            "ownedAnnotation",
            "ownedAttribute",
            "ownedCalculation",
            "ownedCase",
            "ownedConcern",
            "ownedConjugator",
            "ownedConnection",
            "ownedConstraint",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedEnumeration",
            "ownedFeature",
            "ownedFeatureMembership",
            "ownedFlow",
            "ownedImport",
            "ownedInterface",
            "ownedIntersecting",
            "ownedItem",
            "ownedMember",
            "ownedMembership",
            "ownedMetadata",
            "ownedOccurrence",
            "ownedPart",
            "ownedPort",
            "ownedReference",
            "ownedRelationship",
            "ownedRendering",
            "ownedRequirement",
            "ownedSpecialization",
            "ownedState",
            "ownedSubclassification",
            "ownedTransition",
            "ownedUnioning",
            "ownedUsage",
            "ownedUseCase",
            "ownedVerificationCase",
            "ownedView",
            "ownedViewpoint",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "parameter",
            "qualifiedName",
            "result",
            "shortName",
            "step",
            "subjectParameter",
            "textualRepresentation",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "VerificationCaseUsage" => &[
            "actionDefinition",
            "actorParameter",
            "behavior",
            "calculationDefinition",
            "caseDefinition",
            "chainingFeature",
            "crossFeature",
            "declaredShortName",
            "definition",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "direction",
            "documentation",
            "elementId",
            "endFeature",
            "endOwningType",
            "feature",
            "featureMembership",
            "featureTarget",
            "featuringType",
            "function",
            "importedMembership",
            "individualDefinition",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isComposite",
            "isConjugated",
            "isConstant",
            "isDerived",
            "isEnd",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isModelLevelEvaluable",
            "isOrdered",
            "isPortion",
            "isReference",
            "isSufficient",
            "isUnique",
            "isVariable",
            "isVariation",
            "mayTimeVary",
            "member",
            "membership",
            "multiplicity",
            "name",
            "nestedAction",
            "nestedAllocation",
            "nestedAnalysisCase",
            "nestedAttribute",
            "nestedCalculation",
            "nestedCase",
            "nestedConcern",
            "nestedConnection",
            "nestedConstraint",
            "nestedEnumeration",
            "nestedFlow",
            "nestedInterface",
            "nestedItem",
            "nestedMetadata",
            "nestedOccurrence",
            "nestedPart",
            "nestedPort",
            "nestedReference",
            "nestedRendering",
            "nestedRequirement",
            "nestedState",
            "nestedTransition",
            "nestedUsage",
            "nestedUseCase",
            "nestedVerificationCase",
            "nestedView",
            "nestedViewpoint",
            "objectiveRequirement",
            "occurrenceDefinition",
            "output",
            "ownedAnnotation",
            "ownedConjugator",
            "ownedCrossSubsetting",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedFeature",
            "ownedFeatureChaining",
            "ownedFeatureInverting",
            "ownedFeatureMembership",
            "ownedImport",
            "ownedIntersecting",
            "ownedMember",
            "ownedMembership",
            "ownedRedefinition",
            "ownedReferenceSubsetting",
            "ownedRelationship",
            "ownedSpecialization",
            "ownedSubsetting",
            "ownedTypeFeaturing",
            "ownedTyping",
            "ownedUnioning",
            "owner",
            "owningDefinition",
            "owningFeatureMembership",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "owningType",
            "owningUsage",
            "parameter",
            "portionKind",
            "qualifiedName",
            "result",
            "shortName",
            "subjectParameter",
            "textualRepresentation",
            "type",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "SatisfyRequirementUsage" => &[
            "actorParameter",
            "assertedConstraint",
            "assumedConstraint",
            "behavior",
            "chainingFeature",
            "constraintDefinition",
            "crossFeature",
            "declaredShortName",
            "definition",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "direction",
            "documentation",
            "elementId",
            "endFeature",
            "endOwningType",
            "feature",
            "featureMembership",
            "featureTarget",
            "featuringType",
            "framedConcern",
            "function",
            "importedMembership",
            "individualDefinition",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isComposite",
            "isConjugated",
            "isConstant",
            "isDerived",
            "isEnd",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isModelLevelEvaluable",
            "isNegated",
            "isOrdered",
            "isPortion",
            "isReference",
            "isSufficient",
            "isUnique",
            "isVariable",
            "isVariation",
            "mayTimeVary",
            "member",
            "membership",
            "multiplicity",
            "name",
            "nestedAction",
            "nestedAllocation",
            "nestedAnalysisCase",
            "nestedAttribute",
            "nestedCalculation",
            "nestedCase",
            "nestedConcern",
            "nestedConnection",
            "nestedConstraint",
            "nestedEnumeration",
            "nestedFlow",
            "nestedInterface",
            "nestedItem",
            "nestedMetadata",
            "nestedOccurrence",
            "nestedPart",
            "nestedPort",
            "nestedReference",
            "nestedRendering",
            "nestedRequirement",
            "nestedState",
            "nestedTransition",
            "nestedUsage",
            "nestedUseCase",
            "nestedVerificationCase",
            "nestedView",
            "nestedViewpoint",
            "occurrenceDefinition",
            "output",
            "ownedAnnotation",
            "ownedConjugator",
            "ownedCrossSubsetting",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedFeature",
            "ownedFeatureChaining",
            "ownedFeatureInverting",
            "ownedFeatureMembership",
            "ownedImport",
            "ownedIntersecting",
            "ownedMember",
            "ownedMembership",
            "ownedRedefinition",
            "ownedReferenceSubsetting",
            "ownedRelationship",
            "ownedSpecialization",
            "ownedSubsetting",
            "ownedTypeFeaturing",
            "ownedTyping",
            "ownedUnioning",
            "owner",
            "owningDefinition",
            "owningFeatureMembership",
            "owningMembership",
            "owningNamespace",
            "owningRelationship",
            "owningType",
            "owningUsage",
            "parameter",
            "portionKind",
            "predicate",
            "qualifiedName",
            "reqId",
            "requiredConstraint",
            "result",
            "shortName",
            "stakeholderParameter",
            "subjectParameter",
            "text",
            "textualRepresentation",
            "type",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "AllocationUsage" => &[
            "allocationDefinition",
            "association",
            "chainingFeature",
            "connectionDefinition",
            "connectorEnd",
            "crossFeature",
            "declaredShortName",
            "defaultFeaturingType",
            "definition",
            "differencingType",
            "directedFeature",
            "directedUsage",
            "direction",
            "documentation",
            "elementId",
            "endFeature",
            "endOwningType",
            "feature",
            "featureMembership",
            "featureTarget",
            "featuringType",
            "importedMembership",
            "individualDefinition",
            "inheritedFeature",
            "inheritedMembership",
            "input",
            "intersectingType",
            "isAbstract",
            "isComposite",
            "isConjugated",
            "isConstant",
            "isDerived",
            "isEnd",
            "isImplied",
            "isImpliedIncluded",
            "isIndividual",
            "isLibraryElement",
            "isOrdered",
            "isPortion",
            "isReference",
            "isSufficient",
            "isUnique",
            "isVariable",
            "isVariation",
            "itemDefinition",
            "mayTimeVary",
            "member",
            "membership",
            "multiplicity",
            "name",
            "nestedAction",
            "nestedAllocation",
            "nestedAnalysisCase",
            "nestedAttribute",
            "nestedCalculation",
            "nestedCase",
            "nestedConcern",
            "nestedConnection",
            "nestedConstraint",
            "nestedEnumeration",
            "nestedFlow",
            "nestedInterface",
            "nestedItem",
            "nestedMetadata",
            "nestedOccurrence",
            "nestedPart",
            "nestedPort",
            "nestedReference",
            "nestedRendering",
            "nestedRequirement",
            "nestedState",
            "nestedTransition",
            "nestedUsage",
            "nestedUseCase",
            "nestedVerificationCase",
            "nestedView",
            "nestedViewpoint",
            "occurrenceDefinition",
            "output",
            "ownedAnnotation",
            "ownedConjugator",
            "ownedCrossSubsetting",
            "ownedDifferencing",
            "ownedDisjoining",
            "ownedElement",
            "ownedEndFeature",
            "ownedFeature",
            "ownedFeatureChaining",
            "ownedFeatureInverting",
            "ownedFeatureMembership",
            "ownedImport",
            "ownedIntersecting",
            "ownedMember",
            "ownedMembership",
            "ownedRedefinition",
            "ownedReferenceSubsetting",
            "ownedRelatedElement",
            "ownedRelationship",
            "ownedSpecialization",
            "ownedSubsetting",
            "ownedTypeFeaturing",
            "ownedTyping",
            "ownedUnioning",
            "owner",
            "owningDefinition",
            "owningFeatureMembership",
            "owningMembership",
            "owningNamespace",
            "owningRelatedElement",
            "owningRelationship",
            "owningType",
            "owningUsage",
            "partDefinition",
            "portionKind",
            "qualifiedName",
            "relatedElement",
            "relatedFeature",
            "shortName",
            "source",
            "target",
            "textualRepresentation",
            "type",
            "unioningType",
            "usage",
            "variant",
            "variantMembership",
        ],
        "Dependency" => &[
            "client",
            "declaredShortName",
            "documentation",
            "elementId",
            "isImplied",
            "isImpliedIncluded",
            "isLibraryElement",
            "name",
            "ownedAnnotation",
            "ownedElement",
            "ownedRelatedElement",
            "ownedRelationship",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelatedElement",
            "owningRelationship",
            "qualifiedName",
            "relatedElement",
            "shortName",
            "source",
            "supplier",
            "target",
            "textualRepresentation",
        ],
        "FeatureMembership" => &[
            "declaredShortName",
            "documentation",
            "elementId",
            "isImplied",
            "isImpliedIncluded",
            "isLibraryElement",
            "memberElement",
            "memberElementId",
            "memberName",
            "memberShortName",
            "membershipOwningNamespace",
            "name",
            "ownedAnnotation",
            "ownedElement",
            "ownedMemberElement",
            "ownedMemberElementId",
            "ownedMemberFeature",
            "ownedMemberName",
            "ownedMemberShortName",
            "ownedRelatedElement",
            "ownedRelationship",
            "owner",
            "owningMembership",
            "owningNamespace",
            "owningRelatedElement",
            "owningRelationship",
            "owningType",
            "qualifiedName",
            "relatedElement",
            "shortName",
            "source",
            "target",
            "textualRepresentation",
            "visibility",
        ],
        "FeatureTyping" => &[
            "declaredShortName",
            "documentation",
            "elementId",
            "general",
            "isImplied",
            "isImpliedIncluded",
            "isLibraryElement",
            "name",
            "ownedAnnotation",
            "ownedElement",
            "ownedRelatedElement",
            "ownedRelationship",
            "owner",
            "owningFeature",
            "owningMembership",
            "owningNamespace",
            "owningRelatedElement",
            "owningRelationship",
            "owningType",
            "qualifiedName",
            "relatedElement",
            "shortName",
            "source",
            "specific",
            "target",
            "textualRepresentation",
            "type",
            "typedFeature",
        ],
        _ => &[],
    };
    let mut remove = vec![];
    for (key, value) in &row.fields {
        let empty = value.is_null() || value.as_array().is_some_and(Vec::is_empty);
        // elementId is the server's duplicate identity, not an opaque source ID.
        if (empty_fields.contains(&key.as_str()) && empty)
            || (key == "elementId" && value.as_str() == Some(&row.id))
        {
            remove.push(key.clone());
        }
    }
    for key in remove {
        row.fields.remove(&key);
    }
    Ok(())
}

/// Reject duplicate JSON keys recursively before decoding DTO maps.
pub fn decode_json(bytes: &[u8]) -> Result<Value> {
    Ok(serde_json::from_slice::<UniqueJson>(bytes)?.0)
}
struct UniqueJson(Value);
impl<'de> Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("JSON without duplicate keys")
            }
            fn visit_bool<E: serde::de::Error>(
                self,
                v: bool,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, v: i64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, v: u64) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, v: f64) -> std::result::Result<Self::Value, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| UniqueJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("invalid number"))
            }
            fn visit_str<E: serde::de::Error>(
                self,
                v: &str,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_string<E: serde::de::Error>(
                self,
                v: String,
            ) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(v.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Self::Value, E> {
                Ok(UniqueJson(Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut v = vec![];
                while let Some(x) = a.next_element::<UniqueJson>()? {
                    v.push(x.0);
                }
                Ok(UniqueJson(Value::Array(v)))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut m = serde_json::Map::new();
                while let Some(k) = a.next_key::<String>()? {
                    if m.contains_key(&k) {
                        return Err(serde::de::Error::custom(format!("duplicate JSON key {k}")));
                    }
                    let v = a.next_value::<UniqueJson>()?;
                    m.insert(k, v.0);
                }
                Ok(UniqueJson(Value::Object(m)))
            }
        }
        d.deserialize_any(Visitor)
    }
}
