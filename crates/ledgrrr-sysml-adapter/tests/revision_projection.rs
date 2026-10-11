use ledgrrr_sysml_adapter::projection::{
    project_bundle, project_bundle_with_limits, ProjectionLimits, VOCAB,
};
use ufo_types::revision::{PortableBundle, PortableModel, RevisionContext, RevisionId};
fn fixture() -> (PortableBundle, serde_json::Value) {
    let f: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/revision_projection.json")).unwrap();
    let b = PortableBundle::dehydrate(
        serde_json::from_value::<PortableModel>(f["model"].clone()).unwrap(),
        serde_json::from_value::<RevisionContext>(f["context"].clone()).unwrap(),
        serde_json::from_value(f["artifacts"].clone()).unwrap(),
    )
    .unwrap();
    (b, f)
}
#[test]
fn all_canonical_records_are_deterministic_queryable_and_bound_to_actual_acceptance() {
    let (b, f) = fixture();
    let revision = RevisionId::new("actual-accepted-日本語").unwrap();
    let p = project_bundle(&b, &revision).unwrap();
    assert_eq!(p, project_bundle(&b, &revision).unwrap());
    assert_eq!(p.descriptor.checkpoint.revision, revision);
    assert_eq!(
        p.descriptor.accepted_candidate_digest,
        ufo_types::revision::ArtifactDigest::of(&b.to_bytes().unwrap())
    );
    let text = String::from_utf8(p.nquads.clone()).unwrap();
    for kind in f["expected_element_kinds"]
        .as_array()
        .unwrap()
        .iter()
        .chain(f["expected_relation_kinds"].as_array().unwrap())
    {
        assert!(
            text.contains(&format!("\"{}\"", kind.as_str().unwrap())),
            "missing {kind}"
        );
    }
    for record in b.model.elements.values() {
        assert!(text.contains(
            &oxigraph::model::Literal::new_simple_literal(record.id.as_str()).to_string()
        ));
    }
    for expected in [
        "01.2300",
        "authored",
        "inferred",
        "compiler_proven",
        "documented-refinement-rule",
        "crate::controller::control",
        "REQIF:original-日本語",
        "ScalarValues",
        "rustc-1.98.0",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    assert!(!text.contains("_:"));
    assert!(text.ends_with('\n'));
    let lines: Vec<_> = text.lines().collect();
    assert!(lines.windows(2).all(|w| w[0] < w[1]));
    assert!(text.contains(&format!("<{VOCAB}assigned> \"false\"")));
    assert!(text.contains(&format!("<{VOCAB}assigned> \"true\"")));
    assert_ne!(
        p.descriptor.checkpoint.graph_digest,
        project_bundle(&b, &RevisionId::new("other-revision").unwrap())
            .unwrap()
            .descriptor
            .checkpoint
            .graph_digest
    );
}
#[test]
fn empty_and_deleted_all_have_complete_metadata_and_bounds_fail_closed() {
    let (mut b, _) = fixture();
    let old = project_bundle(&b, &RevisionId::new("old").unwrap()).unwrap();
    b.model.elements.clear();
    b.model.relations.clear();
    b.manifest.semantic_digest = b.model.semantic_digest().unwrap();
    let p = project_bundle(&b, &RevisionId::new("deleted-all").unwrap()).unwrap();
    assert!(p.descriptor.quad_count > 0);
    assert_ne!(
        p.descriptor.checkpoint.graph_digest,
        old.descriptor.checkpoint.graph_digest
    );
    for limits in [
        ProjectionLimits {
            max_bytes: 10,
            ..Default::default()
        },
        ProjectionLimits {
            max_quads: 1,
            ..Default::default()
        },
        ProjectionLimits {
            max_depth: 1,
            ..Default::default()
        },
    ] {
        assert!(
            project_bundle_with_limits(&b, &RevisionId::new("bounded").unwrap(), &limits).is_err()
        );
    }
    b.blobs.values_mut().next().unwrap().push(0);
    assert!(project_bundle(&b, &RevisionId::new("tampered").unwrap()).is_err());
}
fn recover(store: &oxigraph::store::Store, n: &oxigraph::model::NamedNode) -> serde_json::Value {
    use oxigraph::model::{NamedNode, Term};
    let objects = |predicate: &str| {
        store
            .quads_for_pattern(
                Some(n.as_ref().into()),
                Some(NamedNode::new_unchecked(predicate).as_ref()),
                None,
                None,
            )
            .map(|q| q.unwrap().object)
            .collect::<Vec<_>>()
    };
    let kind = objects(&format!("{VOCAB}node_kind"));
    let Term::Literal(kind) = &kind[0] else {
        panic!()
    };
    match kind.value() {
        "object" => {
            let mut fields = serde_json::Map::new();
            for q in store.quads_for_pattern(Some(n.as_ref().into()), None, None, None) {
                let q = q.unwrap();
                if let Some(encoded) = q.predicate.as_str().strip_prefix(&format!("{VOCAB}field:"))
                {
                    let bytes = (0..encoded.len())
                        .step_by(2)
                        .map(|i| u8::from_str_radix(&encoded[i..i + 2], 16).unwrap())
                        .collect();
                    let Term::NamedNode(child) = q.object else {
                        panic!()
                    };
                    fields.insert(String::from_utf8(bytes).unwrap(), recover(store, &child));
                }
            }
            serde_json::Value::Object(fields)
        }
        "array" => {
            let mut children = Vec::new();
            for child in objects(&format!("{VOCAB}item")) {
                let Term::NamedNode(child) = child else {
                    panic!()
                };
                let index = store
                    .quads_for_pattern(
                        Some(child.as_ref().into()),
                        Some(NamedNode::new_unchecked(format!("{VOCAB}index")).as_ref()),
                        None,
                        None,
                    )
                    .next()
                    .unwrap()
                    .unwrap()
                    .object;
                let Term::Literal(index) = index else {
                    panic!()
                };
                children.push((
                    index.value().parse::<usize>().unwrap(),
                    recover(store, &child),
                ));
            }
            children.sort_by_key(|x| x.0);
            serde_json::Value::Array(children.into_iter().map(|x| x.1).collect())
        }
        "null" => serde_json::Value::Null,
        scalar => {
            let value = objects(&format!("{VOCAB}value"));
            let Term::Literal(value) = &value[0] else {
                panic!()
            };
            if scalar == "string" {
                value.value().into()
            } else {
                serde_json::from_str(value.value()).unwrap()
            }
        }
    }
}
#[test]
fn independent_rdf_reader_recovers_every_original_typed_field_and_extension() {
    use oxigraph::{
        io::RdfFormat,
        model::{NamedNode, Term},
        store::Store,
    };
    let (b, _) = fixture();
    let p = project_bundle(&b, &RevisionId::new("accepted").unwrap()).unwrap();
    let store = Store::new().unwrap();
    store.load_from_slice(RdfFormat::NQuads, &p.nquads).unwrap();
    let graph = NamedNode::new(&p.graph_iri).unwrap();
    for (key, expected) in [
        ("element", serde_json::to_value(&b.model.elements).unwrap()),
        (
            "relation",
            serde_json::to_value(&b.model.relations).unwrap(),
        ),
    ] {
        let mut actual = serde_json::Map::new();
        for q in store.quads_for_pattern(
            Some(graph.as_ref().into()),
            Some(NamedNode::new_unchecked(format!("{VOCAB}{key}")).as_ref()),
            None,
            None,
        ) {
            let Term::NamedNode(record) = q.unwrap().object else {
                panic!()
            };
            let value = recover(&store, &record);
            actual.insert(value["id"].as_str().unwrap().into(), value);
        }
        assert_eq!(serde_json::Value::Object(actual), expected);
    }
    let Term::NamedNode(manifest) = store
        .quads_for_pattern(
            Some(graph.as_ref().into()),
            Some(NamedNode::new_unchecked(format!("{VOCAB}manifest")).as_ref()),
            None,
            None,
        )
        .next()
        .unwrap()
        .unwrap()
        .object
    else {
        panic!()
    };
    assert_eq!(
        recover(&store, &manifest),
        serde_json::to_value(&b.manifest).unwrap()
    );
}
