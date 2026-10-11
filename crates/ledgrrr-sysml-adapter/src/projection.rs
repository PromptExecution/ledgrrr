//! Versioned lossless accepted-model index. This projection is evidence, not a
//! claim that envelope-only facts were retained natively by a provider.
//!
//! Every canonical field is represented by a typed structural tree: `field:*`
//! predicates encode exact UTF-8 keys, arrays have indexed `item` nodes, and
//! explicit null/empty nodes preserve absence and ordering. Semantic predicates
//! on elements, relations, properties and evidence support ordinary traversal.
use oxigraph::{
    io::{RdfFormat, RdfParser},
    model::{Literal, NamedNode, Quad, Term},
    store::Store,
};
use serde_json::Value;
use std::collections::BTreeSet;
use ufo_types::revision::{
    ArtifactDigest, IndexCheckpoint, PortableBundle, ProjectId, RevisionGraphDescriptor, RevisionId,
};

pub const VOCAB: &str = "urn:ledgrrr:revision:1:";
pub const PROJECTION_SCHEMA: &str = "urn:ledgrrr:revision-projection:1";
#[derive(Debug, Clone)]
pub struct ProjectionLimits {
    pub max_bytes: usize,
    pub max_quads: usize,
    pub max_depth: usize,
}
impl Default for ProjectionLimits {
    fn default() -> Self {
        Self {
            max_bytes: 64 * 1024 * 1024,
            max_quads: 1_000_000,
            max_depth: 128,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum ProjectionError {
    #[error("invalid canonical bundle: {0}")]
    Invalid(String),
    #[error("projection resource limit: {0}")]
    Limit(&'static str),
    #[error("invalid graph artifact: {0}")]
    Graph(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedGraph {
    pub descriptor: RevisionGraphDescriptor,
    pub nquads: Vec<u8>,
    pub graph_iri: String,
}
fn encoded(value: &str) -> String {
    value
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub fn graph_iri(project: &ProjectId, revision: &RevisionId) -> String {
    format!(
        "{VOCAB}graph:{}:{}",
        encoded(project.as_str()),
        encoded(revision.as_str())
    )
}
pub fn field_iri(field: &str) -> String {
    format!("{VOCAB}field:{}", encoded(field))
}
fn node(value: impl Into<String>) -> NamedNode {
    NamedNode::new_unchecked(value)
}
struct Builder<'a> {
    graph: NamedNode,
    lines: BTreeSet<String>,
    bytes: usize,
    limits: &'a ProjectionLimits,
}
impl Builder<'_> {
    fn put(
        &mut self,
        s: &NamedNode,
        p: impl Into<String>,
        o: impl Into<Term>,
    ) -> Result<(), ProjectionError> {
        let line = format!(
            "{} .\n",
            Quad::new(s.clone(), node(p), o, self.graph.clone())
        );
        if !self.lines.contains(&line) {
            if self.lines.len() >= self.limits.max_quads {
                return Err(ProjectionError::Limit("quads"));
            }
            if self.bytes.saturating_add(line.len()) > self.limits.max_bytes {
                return Err(ProjectionError::Limit("bytes"));
            }
            self.bytes += line.len();
            self.lines.insert(line);
        }
        Ok(())
    }
    fn text(&mut self, s: &NamedNode, p: &str, value: &str) -> Result<(), ProjectionError> {
        self.put(s, format!("{VOCAB}{p}"), Literal::new_simple_literal(value))
    }
    fn link(&mut self, s: &NamedNode, p: &str, o: &NamedNode) -> Result<(), ProjectionError> {
        self.put(s, format!("{VOCAB}{p}"), o.clone())
    }
    fn tree(&mut self, s: &NamedNode, value: &Value, depth: usize) -> Result<(), ProjectionError> {
        if depth > self.limits.max_depth {
            return Err(ProjectionError::Limit("depth"));
        }
        match value {
            Value::Object(map) => {
                self.text(s, "node_kind", "object")?;
                for (k, v) in map {
                    let child = node(format!("{}:field:{}", s.as_str(), encoded(k)));
                    self.put(s, field_iri(k), child.clone())?;
                    self.text(&child, "key", k)?;
                    self.tree(&child, v, depth + 1)?;
                }
            }
            Value::Array(values) => {
                self.text(s, "node_kind", "array")?;
                for (i, v) in values.iter().enumerate() {
                    let child = node(format!("{}:item:{i}", s.as_str()));
                    self.link(s, "item", &child)?;
                    self.put(&child, format!("{VOCAB}index"), Literal::from(i as u64))?;
                    self.tree(&child, v, depth + 1)?;
                }
            }
            Value::Null => self.text(s, "node_kind", "null")?,
            Value::Bool(v) => {
                self.text(s, "node_kind", "boolean")?;
                self.put(s, format!("{VOCAB}value"), Literal::from(*v))?;
            }
            Value::Number(v) => {
                self.text(s, "node_kind", "number")?;
                self.text(s, "value", &v.to_string())?;
            }
            Value::String(v) => {
                self.text(s, "node_kind", "string")?;
                self.text(s, "value", v)?;
            }
        }
        Ok(())
    }
    fn type_ref(
        &mut self,
        n: &NamedNode,
        t: &ufo_types::revision::TypeRef,
        element: &impl Fn(&str) -> NamedNode,
    ) -> Result<(), ProjectionError> {
        use ufo_types::revision::TypeRef;
        match t {
            TypeRef::Element(id) => {
                self.text(n, "type_kind", "element")?;
                self.link(n, "type_element", &element(id.as_str()))?;
            }
            TypeRef::External {
                library,
                qualified_name,
            } => {
                self.text(n, "type_kind", "external")?;
                self.text(n, "library", library)?;
                self.text(n, "qualified_name", qualified_name)?;
            }
            TypeRef::List(inner) => {
                self.text(n, "type_kind", "list")?;
                let child = node(format!("{}:inner", n.as_str()));
                self.link(n, "inner_type", &child)?;
                self.type_ref(&child, inner, element)?;
            }
            TypeRef::Boolean => self.text(n, "type_kind", "boolean")?,
            TypeRef::Integer => self.text(n, "type_kind", "integer")?,
            TypeRef::Natural => self.text(n, "type_kind", "natural")?,
            TypeRef::Real => self.text(n, "type_kind", "real")?,
            TypeRef::String => self.text(n, "type_kind", "string")?,
            TypeRef::Timestamp => self.text(n, "type_kind", "timestamp")?,
        }
        Ok(())
    }
    fn typed_value(
        &mut self,
        n: &NamedNode,
        v: &ufo_types::revision::TypedValue,
        element: &impl Fn(&str) -> NamedNode,
    ) -> Result<(), ProjectionError> {
        use ufo_types::revision::TypedValue;
        match v {
            TypedValue::Boolean(v) => {
                self.text(n, "value_kind", "boolean")?;
                self.put(n, format!("{VOCAB}scalar"), Literal::from(*v))?;
            }
            TypedValue::Integer(v) => {
                self.text(n, "value_kind", "integer")?;
                self.put(n, format!("{VOCAB}scalar"), Literal::from(*v))?;
            }
            TypedValue::Natural(v) => {
                self.text(n, "value_kind", "natural")?;
                self.put(n, format!("{VOCAB}scalar"), Literal::from(*v))?;
            }
            TypedValue::Real(v) => {
                self.text(n, "value_kind", "real")?;
                self.text(n, "lexeme", v)?;
            }
            TypedValue::Timestamp(v) => {
                self.text(n, "value_kind", "timestamp")?;
                self.text(n, "lexeme", v)?;
            }
            TypedValue::String(v) => {
                self.text(n, "value_kind", "string")?;
                self.text(n, "scalar", v)?;
            }
            TypedValue::Reference(id) => {
                self.text(n, "value_kind", "reference")?;
                self.link(n, "referenced_element", &element(id.as_str()))?;
            }
            TypedValue::List(values) => {
                self.text(n, "value_kind", "list")?;
                for (i, v) in values.iter().enumerate() {
                    let child = node(format!("{}:value:{i}", n.as_str()));
                    self.link(n, "ordered_value", &child)?;
                    self.put(&child, format!("{VOCAB}index"), Literal::from(i as u64))?;
                    self.typed_value(&child, v, element)?;
                }
            }
        }
        Ok(())
    }
    fn evidence(
        &mut self,
        owner: &NamedNode,
        anchors: &[ufo_types::revision::SourceEvidence],
        bundle: &PortableBundle,
    ) -> Result<(), ProjectionError> {
        for (i, e) in anchors.iter().enumerate() {
            let n = node(format!("{}:evidence:{i}", owner.as_str()));
            self.link(owner, "evidence", &n)?;
            self.tree(
                &n,
                &serde_json::to_value(e).map_err(|e| ProjectionError::Invalid(e.to_string()))?,
                0,
            )?;
            self.text(&n, "artifact_path", e.artifact.as_str())?;
            let digest = &bundle.manifest.artifacts[&e.artifact];
            self.text(&n, "artifact_digest", digest.as_str())?;
            if let Some(r) = &e.source_revision {
                self.text(&n, "source_revision", r.as_str())?;
            }
            let anchor = serde_json::to_value(&e.anchor)
                .map_err(|e| ProjectionError::Invalid(e.to_string()))?;
            let (k, v) = anchor
                .as_object()
                .ok_or_else(|| ProjectionError::Invalid("anchor shape".into()))?
                .iter()
                .next()
                .ok_or_else(|| ProjectionError::Invalid("empty anchor".into()))?;
            self.text(&n, "anchor_kind", k)?;
            if let Some(text) = v.as_str() {
                self.text(&n, "coordinate", text)?;
            } else if let Some(fields) = v.as_object() {
                for (k, v) in fields {
                    if let Some(text) = v.as_str() {
                        self.text(&n, k, text)?;
                    } else if let Some(number) = v.as_u64() {
                        self.put(&n, format!("{VOCAB}{k}"), Literal::from(number))?;
                    }
                }
            }
        }
        Ok(())
    }
}
pub fn project_bundle(
    bundle: &PortableBundle,
    actual_revision: &RevisionId,
) -> Result<ProjectedGraph, ProjectionError> {
    project_bundle_with_limits(bundle, actual_revision, &ProjectionLimits::default())
}
pub fn project_bundle_with_limits(
    bundle: &PortableBundle,
    actual_revision: &RevisionId,
    limits: &ProjectionLimits,
) -> Result<ProjectedGraph, ProjectionError> {
    if limits.max_bytes == 0 || limits.max_quads == 0 || limits.max_depth == 0 {
        return Err(ProjectionError::Limit("zero cap"));
    }
    // Bound recursive typed values/extensions before canonical validation/serialization.
    for e in bundle.model.elements.values() {
        for p in e.properties.values() {
            check_type_depth(&p.type_ref, 0, limits.max_depth)?;
            if let Some(values) = &p.values {
                for v in values {
                    check_value_depth(v, 0, limits.max_depth)?;
                }
            }
        }
        for v in e.extensions.values() {
            check_json_depth(v, 0, limits.max_depth)?;
        }
    }
    for r in bundle.model.relations.values() {
        for v in r.extensions.values() {
            check_json_depth(v, 0, limits.max_depth)?;
        }
    }
    bundle
        .validate()
        .map_err(|e| ProjectionError::Invalid(e.to_string()))?;
    let graph_iri = graph_iri(&bundle.manifest.context.project, actual_revision);
    let g = node(&graph_iri);
    let mut b = Builder {
        graph: g.clone(),
        lines: BTreeSet::new(),
        bytes: 0,
        limits,
    };
    b.text(&g, "projection_schema", PROJECTION_SCHEMA)?;
    b.text(&g, "project", bundle.manifest.context.project.as_str())?;
    b.text(&g, "revision", actual_revision.as_str())?;
    let accepted = ArtifactDigest::of(
        &bundle
            .to_bytes()
            .map_err(|e| ProjectionError::Invalid(e.to_string()))?,
    );
    b.text(&g, "accepted_candidate_digest", accepted.as_str())?;
    let manifest = node(format!("{graph_iri}:manifest"));
    b.link(&g, "manifest", &manifest)?;
    b.tree(
        &manifest,
        &serde_json::to_value(&bundle.manifest)
            .map_err(|e| ProjectionError::Invalid(e.to_string()))?,
        0,
    )?;
    for (path, digest) in &bundle.manifest.artifacts {
        let artifact = node(format!("{graph_iri}:artifact:{}", encoded(path.as_str())));
        b.link(&g, "artifact", &artifact)?;
        b.text(&artifact, "artifact_path", path.as_str())?;
        b.text(&artifact, "artifact_digest", digest.as_str())?;
    }
    let element = |id: &str| node(format!("{graph_iri}:element:{}", encoded(id)));
    for e in bundle.model.elements.values() {
        let n = element(e.id.as_str());
        b.link(&g, "element", &n)?;
        b.tree(
            &n,
            &serde_json::to_value(e).map_err(|e| ProjectionError::Invalid(e.to_string()))?,
            0,
        )?;
        b.text(&n, "id", e.id.as_str())?;
        b.text(&n, "name", &e.name)?;
        b.text(
            &n,
            "element_kind",
            serde_json::to_value(e.kind)
                .map_err(|e| ProjectionError::Invalid(e.to_string()))?
                .as_str()
                .ok_or_else(|| ProjectionError::Invalid("element kind".into()))?,
        )?;
        b.evidence(&n, &e.anchors, bundle)?;
        for (k, p) in &e.properties {
            let prop = node(format!("{}:property:{}", n.as_str(), encoded(k)));
            b.link(&n, "property", &prop)?;
            b.text(&prop, "name", k)?;
            b.tree(
                &prop,
                &serde_json::to_value(p).map_err(|e| ProjectionError::Invalid(e.to_string()))?,
                0,
            )?;
            b.put(
                &prop,
                format!("{VOCAB}assigned"),
                Literal::from(p.values.is_some()),
            )?;
            let type_node = node(format!("{}:type", prop.as_str()));
            b.link(&prop, "property_type", &type_node)?;
            b.type_ref(&type_node, &p.type_ref, &element)?;
            if let Some(values) = &p.values {
                for (i, v) in values.iter().enumerate() {
                    let value_node = node(format!("{}:value:{i}", prop.as_str()));
                    b.link(&prop, "ordered_value", &value_node)?;
                    b.put(
                        &value_node,
                        format!("{VOCAB}index"),
                        Literal::from(i as u64),
                    )?;
                    b.typed_value(&value_node, v, &element)?;
                }
            }
        }
    }
    for r in bundle.model.relations.values() {
        let n = node(format!("{graph_iri}:relation:{}", encoded(r.id.as_str())));
        b.link(&g, "relation", &n)?;
        b.tree(
            &n,
            &serde_json::to_value(r).map_err(|e| ProjectionError::Invalid(e.to_string()))?,
            0,
        )?;
        b.text(&n, "id", r.id.as_str())?;
        let rel = serde_json::to_value(&r.relation)
            .map_err(|e| ProjectionError::Invalid(e.to_string()))?;
        let (kind, roles) = rel
            .as_object()
            .ok_or_else(|| ProjectionError::Invalid("relation shape".into()))?
            .iter()
            .next()
            .ok_or_else(|| ProjectionError::Invalid("empty relation".into()))?;
        b.text(&n, "relation_kind", kind)?;
        for (role, value) in roles
            .as_object()
            .ok_or_else(|| ProjectionError::Invalid("relation roles".into()))?
        {
            if kind == "domain" && role == "kind" {
                b.text(&n, "domain_kind", value.as_str().unwrap_or_default())?;
            } else if let Some(id) = value.as_str() {
                b.link(&n, role, &element(id))?;
            } else if let Some(ends) = value.as_array() {
                for (i, id) in ends.iter().enumerate() {
                    let endpoint = node(format!("{}:end:{i}", n.as_str()));
                    b.link(&n, "end", &endpoint)?;
                    b.put(&endpoint, format!("{VOCAB}index"), Literal::from(i as u64))?;
                    b.link(
                        &endpoint,
                        "element",
                        &element(
                            id.as_str().ok_or_else(|| {
                                ProjectionError::Invalid("relation endpoint".into())
                            })?,
                        ),
                    )?;
                }
            }
        }
        let authority = serde_json::to_value(&r.authority)
            .map_err(|e| ProjectionError::Invalid(e.to_string()))?;
        b.text(
            &n,
            "authority",
            authority
                .as_str()
                .ok_or_else(|| ProjectionError::Invalid("authority".into()))?,
        )?;
        if let Some(rule) = &r.rule {
            b.text(&n, "rule", rule)?;
            b.text(&n, "input_revision", actual_revision.as_str())?;
        }
        b.evidence(&n, &r.anchors, bundle)?;
    }
    let quad_count = b.lines.len() as u64;
    let nquads = b.lines.into_iter().collect::<String>().into_bytes();
    let digest = ArtifactDigest::of(&nquads);
    let descriptor = RevisionGraphDescriptor {
        checkpoint: IndexCheckpoint {
            project: bundle.manifest.context.project.clone(),
            revision: actual_revision.clone(),
            dialect: bundle.manifest.context.model_dialect.clone(),
            graph_digest: digest.clone(),
        },
        projection_schema: PROJECTION_SCHEMA.into(),
        accepted_candidate_digest: accepted,
        artifact_digest: digest,
        quad_count,
    };
    validate_graph(&descriptor, &nquads)?;
    Ok(ProjectedGraph {
        descriptor,
        nquads,
        graph_iri,
    })
}
fn check_json_depth(v: &Value, d: usize, max: usize) -> Result<(), ProjectionError> {
    if d > max {
        return Err(ProjectionError::Limit("depth"));
    }
    match v {
        Value::Array(a) => {
            for v in a {
                check_json_depth(v, d + 1, max)?
            }
        }
        Value::Object(o) => {
            for v in o.values() {
                check_json_depth(v, d + 1, max)?
            }
        }
        _ => {}
    }
    Ok(())
}
fn check_type_depth(
    v: &ufo_types::revision::TypeRef,
    d: usize,
    max: usize,
) -> Result<(), ProjectionError> {
    if d > max {
        return Err(ProjectionError::Limit("depth"));
    }
    if let ufo_types::revision::TypeRef::List(t) = v {
        check_type_depth(t, d + 1, max)?;
    }
    Ok(())
}
fn check_value_depth(
    v: &ufo_types::revision::TypedValue,
    d: usize,
    max: usize,
) -> Result<(), ProjectionError> {
    if d > max {
        return Err(ProjectionError::Limit("depth"));
    }
    if let ufo_types::revision::TypedValue::List(values) = v {
        for v in values {
            check_value_depth(v, d + 1, max)?;
        }
    }
    Ok(())
}
/// Independently parse a sealed graph and verify bytes, scope, count and metadata.
/// Accepted candidate semantics still require recomputation from the retained bundle.
pub fn validate_graph(
    descriptor: &RevisionGraphDescriptor,
    bytes: &[u8],
) -> Result<(), ProjectionError> {
    load_graph_with_check(descriptor, bytes, || Ok(())).map(|_| ())
}
pub(crate) fn load_graph_with_check<F: FnMut() -> Result<(), ProjectionError>>(
    descriptor: &RevisionGraphDescriptor,
    bytes: &[u8],
    mut check: F,
) -> Result<Store, ProjectionError> {
    check()?;

    descriptor
        .validate()
        .map_err(|e| ProjectionError::Graph(e.to_string()))?;
    if descriptor.projection_schema != PROJECTION_SCHEMA {
        return Err(ProjectionError::Graph("unsupported projection".into()));
    }
    if bytes.len() > ProjectionLimits::default().max_bytes {
        return Err(ProjectionError::Limit("bytes"));
    }
    if ArtifactDigest::of(bytes) != descriptor.artifact_digest {
        return Err(ProjectionError::Graph("artifact digest mismatch".into()));
    }
    let store = Store::new().map_err(|e| ProjectionError::Graph(e.to_string()))?;
    for quad in RdfParser::from_format(RdfFormat::NQuads).for_slice(bytes) {
        check()?;
        let quad = quad.map_err(|e| ProjectionError::Graph(e.to_string()))?;
        store
            .insert(&quad)
            .map_err(|e| ProjectionError::Graph(e.to_string()))?;
    }
    let graph = node(graph_iri(
        &descriptor.checkpoint.project,
        &descriptor.checkpoint.revision,
    ));
    let mut count = 0u64;
    for q in &store {
        check()?;
        let q = q.map_err(|e| ProjectionError::Graph(e.to_string()))?;
        if q.graph_name != graph.clone().into()
            || !matches!(q.subject, oxigraph::model::NamedOrBlankNode::NamedNode(_))
            || matches!(q.object, Term::BlankNode(_))
        {
            return Err(ProjectionError::Graph(
                "unexpected graph scope or blank node".into(),
            ));
        }
        count += 1;
    }
    if count != descriptor.quad_count || count > ProjectionLimits::default().max_quads as u64 {
        return Err(ProjectionError::Graph("quad count mismatch".into()));
    }
    for (p, v) in [
        ("projection_schema", PROJECTION_SCHEMA),
        ("project", descriptor.checkpoint.project.as_str()),
        ("revision", descriptor.checkpoint.revision.as_str()),
        (
            "accepted_candidate_digest",
            descriptor.accepted_candidate_digest.as_str(),
        ),
    ] {
        let quad = Quad::new(
            graph.clone(),
            node(format!("{VOCAB}{p}")),
            Literal::new_simple_literal(v),
            graph.clone(),
        );
        if !store
            .contains(&quad)
            .map_err(|e| ProjectionError::Graph(e.to_string()))?
        {
            return Err(ProjectionError::Graph("missing descriptor metadata".into()));
        }
    }
    Ok(store)
}
/// All requirement definitions/usages without an authored Verify relation.
pub fn unverified_requirements_query() -> String {
    format!("SELECT ?requirement ?id WHERE {{ ?requirement <{VOCAB}element_kind> ?kind ; <{VOCAB}id> ?id . FILTER(?kind IN (\"requirement_definition\",\"requirement_usage\")) FILTER NOT EXISTS {{ ?relation <{VOCAB}relation_kind> \"verify\" ; <{VOCAB}requirement> ?requirement ; <{VOCAB}authority> \"authored\" }} }} ORDER BY ?id")
}
pub fn unsatisfied_requirements_query() -> String {
    unverified_requirements_query().replace("\"verify\"", "\"satisfy\"")
}
/// Source coordinates on satisfying elements; relation authority stays visible.
pub fn source_requirement_query() -> String {
    format!("SELECT ?requirement ?id ?symbol ?file ?line ?authority WHERE {{ ?relation <{VOCAB}relation_kind> \"satisfy\" ; <{VOCAB}requirement> ?requirement ; <{VOCAB}subject> ?subject ; <{VOCAB}authority> ?authority . ?requirement <{VOCAB}id> ?id . ?subject <{VOCAB}evidence> ?evidence . ?evidence <{VOCAB}anchor_kind> ?anchor . FILTER(?anchor IN (\"symbol_path\",\"rust_span\")) OPTIONAL {{ ?evidence <{VOCAB}coordinate> ?symbol }} OPTIONAL {{ ?evidence <{VOCAB}file> ?file ; <{VOCAB}line> ?line }} }} ORDER BY ?id ?symbol ?file ?line")
}
/// Domain implements_trait is evidence, not a new native KerML assertion.
pub fn trait_implementation_query() -> String {
    format!("SELECT ?implementer ?trait ?authority WHERE {{ ?relation <{VOCAB}relation_kind> \"domain\" ; <{VOCAB}domain_kind> \"implements_trait\" ; <{VOCAB}source> ?implementer ; <{VOCAB}target> ?trait ; <{VOCAB}authority> ?authority }}")
}
pub fn transition_evidence_query() -> String {
    format!("SELECT ?transition ?evidence ?artifact WHERE {{ ?transition <{VOCAB}relation_kind> \"succession\" ; <{VOCAB}evidence> ?evidence . ?evidence <{VOCAB}artifact_path> ?artifact }}")
}
pub fn revision_artifacts_query() -> String {
    format!("SELECT DISTINCT ?artifact ?digest WHERE {{ ?record <{VOCAB}artifact_path> ?artifact ; <{VOCAB}artifact_digest> ?digest }}")
}
