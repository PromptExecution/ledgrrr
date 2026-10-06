use ledgrrr_sysml_adapter::native::{emit, hydrate, OperationMarker};
use ufo_types::revision::{PortableBundle, PortableModel, RevisionContext};
fn fixture() -> PortableBundle {
    let v: serde_json::Value = serde_json::from_str(include_str!("fixtures/native_revision_cases.json")).unwrap();
    let model: PortableModel = serde_json::from_value(v["model"].clone()).unwrap();
    let context: RevisionContext = serde_json::from_value(v["context"].clone()).unwrap();
    let artifacts = serde_json::from_value(v["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}
fn marker() -> OperationMarker { OperationMarker {project:"project-1".into(),branch:"main".into(),operation:"opaque-operation-日本語".into()} }
#[test]
fn deterministic_native_roundtrip_and_tamper_refusal() {
    let bundle=fixture(); let projection=emit(&bundle,&marker()).unwrap();
    assert_eq!(projection,emit(&bundle,&marker()).unwrap());
    assert_eq!(hydrate(&projection.elements,&bundle.to_bytes().unwrap(),&marker()).unwrap(),bundle);
    let mut altered=projection.elements; altered[0].declared_name=Some("tampered".into());
    assert!(hydrate(&altered,&bundle.to_bytes().unwrap(),&marker()).is_err());
}
#[test]
fn empty_model_has_an_operation_anchor() {
    let mut bundle=fixture(); bundle.model.elements.clear();bundle.model.relations.clear();
    bundle.manifest.semantic_digest=bundle.model.semantic_digest().unwrap();
    let projection=emit(&bundle,&marker()).unwrap(); assert_eq!(projection.elements.len(),1);
    assert!(hydrate(&[],&bundle.to_bytes().unwrap(),&marker()).is_err());
}
