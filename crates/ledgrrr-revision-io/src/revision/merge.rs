//! Deterministic, typed three-way model merges. No remote mutation or implicit conflict resolution.

use std::collections::{BTreeMap, BTreeSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{ArtifactDigest, ModelElement, ModelRelation, PortableModel, RevisionError, invalid};

/// A record replacement preserves both the precondition and its proposed value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecordChange<T> {
    pub before: Option<T>,
    pub after: Option<T>,
}

/// Pure model changes; an owner must separately bind actor, project, operation and expected head.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    pub base_digest: ArtifactDigest,
    pub proposed_digest: ArtifactDigest,
    pub elements: BTreeMap<String, RecordChange<ModelElement>>,
    pub relations: BTreeMap<String, RecordChange<ModelRelation>>,
}

impl ChangeSet {
    /// Decode without duplicate keys or silently ignored nested fields. Applying to a validated
    /// base remains mandatory; decoding alone cannot prove a proposed model is well formed.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, RevisionError> {
        let input: super::UniqueJson =
            serde_json::from_slice(bytes).map_err(|e| RevisionError::Json(e.to_string()))?;
        let changes: Self = serde_json::from_value(input.0.clone())
            .map_err(|e| RevisionError::Json(e.to_string()))?;
        let encoded =
            serde_json::to_value(&changes).map_err(|e| RevisionError::Json(e.to_string()))?;
        super::require_preserved_keys(&input.0, &encoded, "changes")?;
        Ok(changes)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, RevisionError> {
        super::canonical_bytes(self)
    }

    pub fn between(base: &PortableModel, proposed: &PortableModel) -> Result<Self, RevisionError> {
        Ok(Self {
            base_digest: base.semantic_digest()?,
            proposed_digest: proposed.semantic_digest()?,
            elements: differences(&base.elements, &proposed.elements),
            relations: differences(&base.relations, &proposed.relations),
        })
    }

    /// Apply only to the exact validated base. Reject edited preconditions and false digest claims.
    pub fn apply(&self, base: &PortableModel) -> Result<PortableModel, RevisionError> {
        if base.semantic_digest()? != self.base_digest {
            return Err(invalid("/base_digest", "change set base does not match"));
        }
        let mut model = base.clone();
        apply_records(&mut model.elements, &self.elements, "/elements")?;
        apply_records(&mut model.relations, &self.relations, "/relations")?;
        if model.semantic_digest()? != self.proposed_digest {
            return Err(invalid(
                "/proposed_digest",
                "change set result digest does not match",
            ));
        }
        Ok(model)
    }
}

fn keys<T>(a: &BTreeMap<String, T>, b: &BTreeMap<String, T>) -> BTreeSet<String> {
    a.keys().chain(b.keys()).cloned().collect()
}

fn differences<T: Clone + PartialEq>(
    base: &BTreeMap<String, T>,
    proposed: &BTreeMap<String, T>,
) -> BTreeMap<String, RecordChange<T>> {
    keys(base, proposed)
        .into_iter()
        .filter_map(|key| {
            let before = base.get(&key);
            let after = proposed.get(&key);
            (before != after).then(|| {
                (
                    key,
                    RecordChange {
                        before: before.cloned(),
                        after: after.cloned(),
                    },
                )
            })
        })
        .collect()
}

fn apply_records<T: Clone + PartialEq>(
    target: &mut BTreeMap<String, T>,
    changes: &BTreeMap<String, RecordChange<T>>,
    prefix: &str,
) -> Result<(), RevisionError> {
    for (key, change) in changes {
        if target.get(key) != change.before.as_ref() || change.before == change.after {
            return Err(invalid(
                path(prefix, key),
                "record precondition does not match or edit is redundant",
            ));
        }
        match &change.after {
            Some(value) => {
                target.insert(key.clone(), value.clone());
            }
            None => {
                target.remove(key);
            }
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    SameField,
    DeleteModify,
    ConcurrentCreation,
    SemanticViolation,
}

/// Snapshots stay typed even when a conflict concerns one field inside a record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ConflictSubject {
    Element {
        base: Option<ModelElement>,
        ours: Option<ModelElement>,
        theirs: Option<ModelElement>,
    },
    Relation {
        base: Option<ModelRelation>,
        ours: Option<ModelRelation>,
        theirs: Option<ModelRelation>,
    },
    Semantic {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Conflict {
    /// RFC 6901 JSON pointer; conflicting values are in `subject`.
    pub path: String,
    pub kind: ConflictKind,
    pub subject: ConflictSubject,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum MergeOutcome {
    /// `changes` applies to theirs (the newly observed head), after revalidation.
    Merged {
        model: PortableModel,
        changes: ChangeSet,
    },
    /// No partial candidate escapes through this result.
    Conflicted { conflicts: Vec<Conflict> },
}

fn path(prefix: &str, key: &str) -> String {
    format!("{prefix}/{}", key.replace('~', "~0").replace('/', "~1"))
}

struct MergeState {
    conflicts: Vec<Conflict>,
}

impl MergeState {
    fn value<T: Clone + PartialEq>(
        &mut self,
        base: &T,
        ours: &T,
        theirs: &T,
        path: &str,
        subject: &ConflictSubject,
    ) -> T {
        if ours == theirs || theirs == base {
            return ours.clone();
        }
        if ours == base {
            return theirs.clone();
        }
        self.conflicts.push(Conflict {
            path: path.into(),
            kind: ConflictKind::SameField,
            subject: subject.clone(),
        });
        ours.clone() // Private scratch only: conflicted results never return a model.
    }

    fn fields<T: Clone + PartialEq>(
        &mut self,
        base: &BTreeMap<String, T>,
        ours: &BTreeMap<String, T>,
        theirs: &BTreeMap<String, T>,
        prefix: &str,
        subject: &ConflictSubject,
    ) -> BTreeMap<String, T> {
        keys(base, ours)
            .union(&keys(base, theirs))
            .map(|key| {
                let value = self.value(
                    &base.get(key),
                    &ours.get(key),
                    &theirs.get(key),
                    &path(prefix, key),
                    subject,
                );
                (key.clone(), value.cloned())
            })
            .filter_map(|(key, value)| value.map(|v| (key, v)))
            .collect()
    }
}

fn merge_records<T: Clone + PartialEq>(
    base: &BTreeMap<String, T>,
    ours: &BTreeMap<String, T>,
    theirs: &BTreeMap<String, T>,
    prefix: &str,
    state: &mut MergeState,
    subject: impl Fn(Option<T>, Option<T>, Option<T>) -> ConflictSubject,
    merge: impl Fn(&T, &T, &T, &str, &ConflictSubject, &mut MergeState) -> T,
) -> BTreeMap<String, T> {
    let all: BTreeSet<_> = keys(base, ours)
        .union(&keys(base, theirs))
        .cloned()
        .collect();
    let mut result = BTreeMap::new();
    for key in all {
        let (b, o, t) = (base.get(&key), ours.get(&key), theirs.get(&key));
        let pointer = path(prefix, &key);
        let chosen = if o == t || t == b {
            o.cloned()
        } else if o == b {
            t.cloned()
        } else {
            let evidence = subject(b.cloned(), o.cloned(), t.cloned());
            match (b, o, t) {
                (Some(b), Some(o), Some(t)) => Some(merge(b, o, t, &pointer, &evidence, state)),
                _ => {
                    state.conflicts.push(Conflict {
                        path: pointer,
                        kind: if b.is_none() {
                            ConflictKind::ConcurrentCreation
                        } else {
                            ConflictKind::DeleteModify
                        },
                        subject: evidence,
                    });
                    None
                }
            }
        };
        if let Some(value) = chosen {
            result.insert(key, value);
        }
    }
    result
}

/// Merge independent fields and independent property/extension keys. A property, source-evidence
/// vector or relation payload is atomic: concurrent edits to it require explicit resolution.
/// All three inputs must be valid. The entire merged candidate is validated again before return.
pub fn merge_models(
    base: &PortableModel,
    ours: &PortableModel,
    theirs: &PortableModel,
) -> Result<MergeOutcome, RevisionError> {
    base.validate()?;
    ours.validate()?;
    theirs.validate()?;
    let mut state = MergeState {
        conflicts: Vec::new(),
    };
    let elements = merge_records(
        &base.elements,
        &ours.elements,
        &theirs.elements,
        "/elements",
        &mut state,
        |base, ours, theirs| ConflictSubject::Element { base, ours, theirs },
        |b, o, t, p, evidence, s| ModelElement {
            id: b.id.clone(),
            kind: s.value(&b.kind, &o.kind, &t.kind, &path(p, "kind"), evidence),
            name: s.value(&b.name, &o.name, &t.name, &path(p, "name"), evidence),
            properties: s.fields(
                &b.properties,
                &o.properties,
                &t.properties,
                &path(p, "properties"),
                evidence,
            ),
            anchors: s.value(
                &b.anchors,
                &o.anchors,
                &t.anchors,
                &path(p, "anchors"),
                evidence,
            ),
            extensions: s.fields(
                &b.extensions,
                &o.extensions,
                &t.extensions,
                &path(p, "extensions"),
                evidence,
            ),
        },
    );
    let relations = merge_records(
        &base.relations,
        &ours.relations,
        &theirs.relations,
        "/relations",
        &mut state,
        |base, ours, theirs| ConflictSubject::Relation { base, ours, theirs },
        |b, o, t, p, evidence, s| ModelRelation {
            id: b.id.clone(),
            relation: s.value(
                &b.relation,
                &o.relation,
                &t.relation,
                &path(p, "relation"),
                evidence,
            ),
            authority: s.value(
                &b.authority,
                &o.authority,
                &t.authority,
                &path(p, "authority"),
                evidence,
            ),
            anchors: s.value(
                &b.anchors,
                &o.anchors,
                &t.anchors,
                &path(p, "anchors"),
                evidence,
            ),
            rule: s.value(&b.rule, &o.rule, &t.rule, &path(p, "rule"), evidence),
            extensions: s.fields(
                &b.extensions,
                &o.extensions,
                &t.extensions,
                &path(p, "extensions"),
                evidence,
            ),
        },
    );
    let model = PortableModel {
        elements,
        relations,
    };
    if state.conflicts.is_empty() {
        if let Err(error) = model.validate() {
            state.conflicts.push(Conflict {
                path: "/model".into(),
                kind: ConflictKind::SemanticViolation,
                subject: ConflictSubject::Semantic {
                    reason: error.to_string(),
                },
            });
        }
    }
    if state.conflicts.is_empty() {
        Ok(MergeOutcome::Merged {
            changes: ChangeSet::between(theirs, &model)?,
            model,
        })
    } else {
        state.conflicts.sort_by(|a, b| a.path.cmp(&b.path));
        Ok(MergeOutcome::Conflicted {
            conflicts: state.conflicts,
        })
    }
}
