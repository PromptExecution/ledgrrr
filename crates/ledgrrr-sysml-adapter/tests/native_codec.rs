use ledgrrr_sysml_adapter::native::{emit, hydrate, OperationMarker};
use ufo_types::revision::{PortableBundle, PortableModel, RevisionContext};
fn fixture() -> PortableBundle {
    let v: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    let model: PortableModel = serde_json::from_value(v["model"].clone()).unwrap();
    let context: RevisionContext = serde_json::from_value(v["context"].clone()).unwrap();
    let artifacts = serde_json::from_value(v["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}
fn marker() -> OperationMarker {
    OperationMarker {
        project: "project-1".into(),
        branch: "main".into(),
        operation: "opaque-operation-日本語".into(),
    }
}
#[test]
fn deterministic_native_roundtrip_and_tamper_refusal() {
    let bundle = fixture();
    let projection = emit(&bundle, &marker()).unwrap();
    assert_eq!(projection, emit(&bundle, &marker()).unwrap());
    assert_eq!(
        hydrate(&projection.elements, &bundle.to_bytes().unwrap(), &marker()).unwrap(),
        bundle
    );
    let mut altered = projection.elements;
    altered[0].declared_name = Some("tampered".into());
    assert!(hydrate(&altered, &bundle.to_bytes().unwrap(), &marker()).is_err());
}
#[test]
fn empty_model_has_an_operation_anchor() {
    let mut bundle = fixture();
    bundle.model.elements.clear();
    bundle.model.relations.clear();
    bundle.manifest.semantic_digest = bundle.model.semantic_digest().unwrap();
    let projection = emit(&bundle, &marker()).unwrap();
    assert_eq!(projection.elements.len(), 1);
    assert!(hydrate(&[], &bundle.to_bytes().unwrap(), &marker()).is_err());
}
#[test]
fn real_reference_shapes_and_non_default_semantics() {
    let b = fixture();
    let p = emit(&b, &marker()).unwrap();
    let req = p
        .elements
        .iter()
        .find(|e| e.kind == "RequirementUsage")
        .unwrap();
    assert!(req.requirement_definition.is_none());
    let verify = p
        .elements
        .iter()
        .find(|e| e.kind == "VerificationCaseUsage")
        .unwrap();
    assert!(verify.verification_case_definition.is_none());
    assert_eq!(verify.verified_requirement.len(), 1);
    let satisfy = p
        .elements
        .iter()
        .find(|e| e.kind == "SatisfyRequirementUsage")
        .unwrap();
    assert!(satisfy.satisfied_requirement.is_some());
    assert!(satisfy.satisfying_feature.is_some());
    let allocate = p
        .elements
        .iter()
        .find(|e| e.kind == "AllocationUsage")
        .unwrap();
    assert!(allocate.source_feature.is_some());
    assert_eq!(allocate.target_feature.len(), 1);
    let defaults: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    let mut native = p.elements.clone();
    for row in &mut native {
        for (k, v) in defaults["native_defaults"][&row.kind].as_object().unwrap() {
            row.fields.entry(k.clone()).or_insert_with(|| v.clone());
        }
        row.fields.insert("elementId".into(), row.id.clone().into());
    }
    assert_eq!(
        hydrate(&native, &b.to_bytes().unwrap(), &marker()).unwrap(),
        b
    );
    native[0].fields.insert("isAbstract".into(), true.into());
    assert!(hydrate(&native, &b.to_bytes().unwrap(), &marker()).is_err());
}
#[test]
fn unsupported_behavior_derivation_endpoints_and_duplicate_keys_fail() {
    use ufo_types::{ElementKind, Relation};
    let b = fixture();
    for kind in [ElementKind::ActionDefinition, ElementKind::StateUsage] {
        let mut changed = b.clone();
        changed.model.elements.get_mut("controller").unwrap().kind = kind;
        changed.manifest.semantic_digest = changed.model.semantic_digest().unwrap();
        assert!(matches!(
            emit(&changed, &marker()),
            Err(ledgrrr_sysml_adapter::native::Error::Fidelity(_))
        ));
    }
    let mut changed = b.clone();
    changed.model.relations.get_mut("cycle-a").unwrap().relation = Relation::Domain {
        source: "REQ-日本語".into(),
        target: "REQ-2".into(),
        kind: "derivation".into(),
    };
    changed.manifest.semantic_digest = changed.model.semantic_digest().unwrap();
    assert!(emit(&changed, &marker()).is_err());
    let mut changed = b.clone();
    changed.model.elements.get_mut("REQ-日本語").unwrap().kind = ElementKind::RequirementDefinition;
    changed.model.relations.remove("type-req");
    changed.manifest.semantic_digest = changed.model.semantic_digest().unwrap();
    assert!(emit(&changed, &marker()).is_err());
    assert!(ledgrrr_sysml_adapter::native::decode_json(br#"[{"@id":"a","@id":"b"}]"#).is_err());
    assert!(ledgrrr_sysml_adapter::native::decode_json(br#"{"a":{"x":1,"x":2}}"#).is_err());
}
#[test]
fn dropped_markers_aliases_unknown_fields_and_opaque_uuid_collisions_refuse() {
    let b = fixture();
    let p = emit(&b, &marker()).unwrap();
    let mut wrong = marker();
    wrong.operation = "other".into();
    assert!(hydrate(&p.elements, &b.to_bytes().unwrap(), &wrong).is_err());
    for mutation in 0..4 {
        let mut elements = p.elements.clone();
        match mutation {
            0 => elements[0].alias_ids.clear(),
            1 => elements.push(elements[0].clone()),
            2 => elements[0]
                .fields
                .insert("unknown".into(), serde_json::json!([]))
                .map(|_| ())
                .unwrap_or(()),
            _ => {
                let duplicate = elements[0].alias_ids[0].clone();
                elements[0].alias_ids.push(duplicate);
            }
        }
        assert!(hydrate(&elements, &b.to_bytes().unwrap(), &marker()).is_err());
    }
    use ledgrrr_sysml_adapter::native::provider_uuid;
    assert_ne!(provider_uuid("ab", "c"), provider_uuid("a", "bc"));
    assert_ne!(provider_uuid("main", "REQ-日本語"), "REQ-日本語");
    assert_eq!(provider_uuid("main", "REQ-日本語").len(), 36);
}
use ledgrrr_sysml_adapter::client::{Bounds, NativeClient};
use std::io::{Read, Write};
use std::time::Duration;
const CURSOR: &str = "MTIzfDAwMDAwMDAwLTAwMDAtMDAwMC0wMDAwLTAwMDAwMDAwMDAwMQ";
struct MockReply {
    body: String,
    link: Option<String>,
    delay: Duration,
    status: u16,
}
fn reply(body: &str) -> MockReply {
    MockReply {
        body: body.into(),
        link: None,
        delay: Duration::ZERO,
        status: 200,
    }
}
fn mock(replies: Vec<MockReply>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut requests = vec![];
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            let mut request = vec![];
            loop {
                let mut b = [0; 1024];
                let n = stream.read(&mut b).unwrap();
                if n == 0 {
                    break;
                }
                request.extend_from_slice(&b[..n]);
                if request.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            requests.push(String::from_utf8(request).unwrap());
            std::thread::sleep(reply.delay);
            let headers = reply
                .link
                .map(|l| {
                    format!(
                        "Link: {}\r\n",
                        l.replace("{origin}", &format!("http://{addr}"))
                    )
                })
                .unwrap_or_default();
            let response = format!(
                "HTTP/1.1 {} OK\r\nContent-Length: {}\r\nConnection: close\r\n{headers}\r\n{}",
                reply.status,
                reply.body.len(),
                reply.body
            );
            let _ = stream.write_all(response.as_bytes());
        }
        requests
    });
    (format!("http://{addr}"), handle)
}
#[tokio::test]
async fn exact_pagination_reconstructs_cursor_and_preserves_exclude_used() {
    // Dynamic server origin is needed in Link: reserve a listener using this helper's
    // placeholder replacement at response time via an independent first mock.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let handle = std::thread::spawn(move || {
        let mut paths = vec![];
        for i in 0..2 {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 4096];
            let n = stream.read(&mut request).unwrap();
            paths.push(String::from_utf8(request[..n].to_vec()).unwrap());
            let body = if i == 0 {
                r#"[{"@id":"native-a","@type":"Package"}]"#
            } else {
                "[]"
            };
            let link = if i == 0 {
                format!("Link: <http://{addr}/projects/p/commits/c/elements?page[after]={CURSOR}&page[size]=1>; rel=\"next\"\r\n")
            } else {
                String::new()
            };
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n{link}\r\n{body}",body.len()).as_bytes()).unwrap();
        }
        paths
    });
    let client = NativeClient::new(
        &format!("http://{addr}"),
        Bounds {
            page_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(client.snapshot("p", "c", false).await.unwrap().len(), 1);
    let paths = handle.join().unwrap();
    assert!(paths.iter().all(|p| p.contains("excludeUsed=false")));
    assert!(paths[1].contains("page%5Bafter%5D="));
}
#[tokio::test]
async fn pagination_partial_cross_origin_duplicate_json_and_byte_bound_refuse() {
    for (body,link,max_bytes) in [
        (r#"[{"@id":"a","@type":"Package"}]"#,None,4096),
        (r#"[{"@id":"a","@type":"Package"}]"#,Some(format!("<http://evil.test/projects/p/commits/c/elements?page[after]={CURSOR}&page[size]=1>; rel=\"next\"")),4096),
        (r#"[{"@id":"a","@id":"b","@type":"Package"}]"#,None,4096),
        (r#"[{"@id":"a","@type":"Package"}]"#,None,4),
    ] {let mut response=reply(body);response.link=link;let(origin,handle)=mock(vec![response]);let client=NativeClient::new(&origin,Bounds{page_size:1,max_bytes,..Default::default()}).unwrap();assert!(client.snapshot("p","c",false).await.is_err());handle.join().unwrap();}
}
#[tokio::test]
async fn late_pages_and_redirects_refuse() {
    let mut late = reply("[]");
    late.delay = Duration::from_millis(150);
    let (origin, handle) = mock(vec![late]);
    let client = NativeClient::new(
        &origin,
        Bounds {
            deadline: Duration::from_millis(30),
            ..Default::default()
        },
    )
    .unwrap();
    assert!(client.snapshot("p", "c", true).await.is_err());
    handle.join().unwrap();
    let mut redirected = reply("[]");
    redirected.status = 302;
    let (origin, handle) = mock(vec![redirected]);
    let client = NativeClient::new(&origin, Bounds::default()).unwrap();
    assert!(client.snapshot("p", "c", true).await.is_err());
    handle.join().unwrap();
}
#[tokio::test]
async fn history_only_traverses_reachable_parents_and_checks_head_again() {
    let branch = r#"{"@id":"b","owningProject":{"@id":"p"},"head":{"@id":"c2"}}"#;
    let (origin, handle) = mock(vec![
        reply(branch),
        reply(r#"{"@id":"c2","previousCommit":{"@id":"c1"}}"#),
        reply(r#"{"@id":"c1","previousCommit":null}"#),
        reply(branch),
    ]);
    let client = NativeClient::new(&origin, Bounds::default()).unwrap();
    let history = client.history("p", "b").await.unwrap();
    assert_eq!(
        history.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(),
        ["c2", "c1"]
    );
    let paths = handle.join().unwrap();
    assert!(paths[1].starts_with("GET /projects/p/commits/c2 "));
    let (origin, handle) = mock(vec![
        reply(branch),
        reply(r#"{"@id":"c2","previousCommit":{"@id":"c2"}}"#),
    ]);
    let client = NativeClient::new(&origin, Bounds::default()).unwrap();
    assert!(client.history("p", "b").await.is_err());
    handle.join().unwrap();
}
#[tokio::test]
async fn malformed_cross_resource_cursor_loops_and_page_bounds_refuse() {
    for query in [
        "page[after]=!&page[size]=1",
        "page[after]=YWJj&page[size]=1",
        "page[after]=x&page[after]=y&page[size]=1",
    ] {
        let mut response = reply(r#"[{"@id":"a","@type":"Package"}]"#);
        response.link = Some(format!(
            "<{{origin}}/projects/p/commits/c/elements?{query}>; rel=\"next\""
        ));
        let (origin, handle) = mock(vec![response]);
        let client = NativeClient::new(
            &origin,
            Bounds {
                page_size: 1,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(client.snapshot("p", "c", false).await.is_err());
        handle.join().unwrap();
    }
    let link=format!("<{{origin}}/projects/p/commits/c/elements?page[after]={CURSOR}&page[size]=1>; rel=\"next\"");
    let mut first = reply(r#"[{"@id":"a","@type":"Package"}]"#);
    first.link = Some(link.clone());
    let mut second = reply(r#"[{"@id":"b","@type":"Package"}]"#);
    second.link = Some(link);
    let (origin, handle) = mock(vec![first, second]);
    let client = NativeClient::new(
        &origin,
        Bounds {
            page_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(client.snapshot("p", "c", false).await.is_err());
    handle.join().unwrap();
    let mut first = reply(r#"[{"@id":"a","@type":"Package"}]"#);
    first.link=Some(format!("<{{origin}}/projects/p/commits/c/elements?page[after]={CURSOR}&page[size]=1>; rel=\"next\""));
    let (origin, handle) = mock(vec![first]);
    let client = NativeClient::new(
        &origin,
        Bounds {
            page_size: 1,
            max_pages: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(client.snapshot("p", "c", false).await.is_err());
    handle.join().unwrap();
    let mut cross = reply(r#"[{"@id":"a","@type":"Package"}]"#);
    cross.link=Some(format!("<{{origin}}/projects/p/commits/other/elements?page[after]={CURSOR}&page[size]=1>; rel=\"next\""));
    let (origin, handle) = mock(vec![cross]);
    let client = NativeClient::new(
        &origin,
        Bounds {
            page_size: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(client.snapshot("p", "c", false).await.is_err());
    handle.join().unwrap();
}
#[tokio::test]
async fn incomplete_history_missing_parent_changed_head_and_identity_bounds_refuse() {
    let branch = r#"{"@id":"b","owningProject":{"@id":"p"},"head":{"@id":"c"}}"#;
    for replies in [
        vec![reply(branch), reply(r#"{"@id":"c"}"#)],
        vec![
            reply(branch),
            reply(r#"{"@id":"c","previousCommit":null}"#),
            reply(r#"{"@id":"b","owningProject":{"@id":"p"},"head":{"@id":"other"}}"#),
        ],
        vec![
            reply(branch),
            reply(r#"{"@id":"wrong","previousCommit":null}"#),
        ],
    ] {
        let (origin, handle) = mock(replies);
        let client = NativeClient::new(&origin, Bounds::default()).unwrap();
        assert!(client.history("p", "b").await.is_err());
        handle.join().unwrap();
    }
    let (origin, handle) = mock(vec![reply(
        r#"[{"@id":"a","@type":"Package"},{"@id":"b","@type":"Package"}]"#,
    )]);
    let client = NativeClient::new(
        &origin,
        Bounds {
            max_ids: 1,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(client.snapshot("p", "c", false).await.is_err());
    handle.join().unwrap();
    assert!(NativeClient::new("http://user:pass@localhost/", Bounds::default()).is_err());
    assert!(NativeClient::new("http://localhost/projects", Bounds::default()).is_err());
}
#[test]
fn native_singleton_typing_shapes_are_not_generic_specialization() {
    use ledgrrr_sysml_adapter::native::NativeElement;
    let req:NativeElement=serde_json::from_value(serde_json::json!({"@id":"usage","@type":"RequirementUsage","requirementDefinition":{"@id":"definition"}})).unwrap();
    assert_eq!(req.requirement_definition.unwrap().id, "definition");
    assert!(serde_json::from_value::<NativeElement>(serde_json::json!({"@id":"usage","@type":"RequirementUsage","requirementDefinition":[{"@id":"definition"}]})).is_err());
    let verification:NativeElement=serde_json::from_value(serde_json::json!({"@id":"usage","@type":"VerificationCaseUsage","verificationCaseDefinition":{"@id":"definition"},"verifiedRequirement":[{"@id":"requirement"}]})).unwrap();
    assert_eq!(
        verification.verification_case_definition.unwrap().id,
        "definition"
    );
    let mut b = fixture();
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    b.model.relations.extend(
        serde_json::from_value::<
            std::collections::BTreeMap<String, ufo_types::revision::ModelRelation>,
        >(value["pending_typing"].clone())
        .unwrap(),
    );
    b.manifest.semantic_digest = b.model.semantic_digest().unwrap();
    assert!(emit(&b, &marker()).is_err());
}
#[test]
fn verified_machine_evidence_does_not_fabricate_native_behavior() {
    let mut b = fixture();
    let value: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    b.model.elements.extend(
        serde_json::from_value::<
            std::collections::BTreeMap<String, ufo_types::revision::ModelElement>,
        >(value["verified_machine"]["elements"].clone())
        .unwrap(),
    );
    b.model.relations.extend(
        serde_json::from_value::<
            std::collections::BTreeMap<String, ufo_types::revision::ModelRelation>,
        >(value["verified_machine"]["relations"].clone())
        .unwrap(),
    );
    b.manifest.semantic_digest = b.model.semantic_digest().unwrap();
    assert!(matches!(
        emit(&b, &marker()),
        Err(ledgrrr_sysml_adapter::native::Error::Fidelity(_))
    ));
}

#[test]
fn marker_binds_the_logical_owner_project() {
    let b = fixture();
    let mut wrong = marker();
    wrong.project = "different-owner-project".into();
    assert!(emit(&b, &wrong).is_err());
    let p = emit(&b, &marker()).unwrap();
    assert!(hydrate(&p.elements, &b.to_bytes().unwrap(), &wrong).is_err());
}
#[test]
fn true_feature_typing_projects_usage_specific_definition_links() {
    let mut b = fixture();
    for (id, feature, definition) in [
        ("type-req", "REQ-日本語", "REQ-2"),
        ("type-verification", "test", "verification-definition"),
    ] {
        let relation=serde_json::from_value(serde_json::json!({"id":id,"relation":{"feature_typing":{"feature":feature,"type":definition}},"authority":"authored","anchors":[],"rule":null,"extensions":{}})).unwrap();
        b.model.relations.insert(id.into(), relation);
    }
    b.manifest.semantic_digest = b.model.semantic_digest().unwrap();
    let projection = emit(&b, &marker()).unwrap();
    let req = projection
        .elements
        .iter()
        .find(|e| e.kind == "RequirementUsage")
        .unwrap();
    assert!(req.requirement_definition.is_some());
    let verify = projection
        .elements
        .iter()
        .find(|e| e.kind == "VerificationCaseUsage")
        .unwrap();
    assert!(verify.verification_case_definition.is_some());
    assert_eq!(
        projection
            .elements
            .iter()
            .filter(|e| e.kind == "FeatureTyping")
            .count(),
        2
    );
    assert_eq!(
        hydrate(&projection.elements, &b.to_bytes().unwrap(), &marker()).unwrap(),
        b
    );
}
