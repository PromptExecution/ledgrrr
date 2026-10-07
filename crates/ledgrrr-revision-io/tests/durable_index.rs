use ledgrrr_revision_io::*;
use ufo_types::revision::{ArtifactPath, PortableModel, RevisionContext, RevisionGraphDescriptor};
use std::collections::BTreeMap;

fn a() -> ActorId { ActorId::new("owner").unwrap() }
fn p() -> ProjectId { ProjectId::new("project-1").unwrap() }
fn b() -> BranchId { BranchId::new("main").unwrap() }
fn candidate(parent: Option<&str>) -> PortableBundle {
    let f: serde_json::Value = serde_json::from_str(include_str!("fixtures/durable_revision_cases.json")).unwrap();
    let model: PortableModel = serde_json::from_value(f["model"].clone()).unwrap();
    let mut context: RevisionContext = serde_json::from_value(f["context"].clone()).unwrap();
    context.parent = parent.map(|v| RevisionId::new(v).unwrap());
    let artifacts: BTreeMap<ArtifactPath, Vec<u8>> = serde_json::from_value(f["artifacts"].clone()).unwrap();
    PortableBundle::dehydrate(model, context, artifacts).unwrap()
}
fn setup(path: &std::path::Path) -> Store {
    let mut s = Store::open(path).unwrap();
    let binding: ProjectBinding = serde_json::from_value(serde_json::from_str::<serde_json::Value>(include_str!("fixtures/durable_revision_cases.json")).unwrap()["binding"].clone()).unwrap();
    s.bootstrap_project(&a(), &p(), &binding).unwrap();
    s.register_branch(&a(), &p(), &b(), "remote-main").unwrap();
    s
}
fn accept(s: &mut Store, op: &str, revision: &str, parent: Option<&str>) -> StoredOperation {
    let bundle = candidate(parent);
    let i = Intake {project:p(),branch:b(),operation:OperationId::new(op).unwrap(),expected_head:parent.map(|v|ExpectedHead::Revision(RevisionId::new(v).unwrap())).unwrap_or(ExpectedHead::Empty),binding:s.project_binding(&a(),&p()).unwrap()};
    let initial = s.intake(&a(), &i, &bundle.to_bytes().unwrap()).unwrap();
    let reserved=s.reserve(&a(),&p(),&i.operation,initial.generation,"reserve").unwrap();
    let token=reserved.dispatch.as_ref().unwrap();
    let prepared=s.prepare(&a(),token,reserved.generation,&bundle.to_bytes().unwrap(),ArtifactDigest::of(b"native"),ArtifactDigest::of(b"identities")).unwrap();
    let sent=s.authorize_send(&a(),token,prepared.generation,true).unwrap();
    s.record_commit(&a(),sent.generation,&CommitEvidence{token:token.clone(),binding:i.binding,expected_head:i.expected_head,proposal_digest:initial.receipt.proposal_digest,actual_revision:RevisionId::new(revision).unwrap(),observation:"actual adapter acceptance".into()}).unwrap()
}
fn graph(revision: &str, digest: ArtifactDigest) -> (RevisionGraphDescriptor,Vec<u8>) {
    let bytes=format!("<urn:revision:{revision}> <urn:predicate> \"complete\" <urn:graph:{revision}> .\n").into_bytes();
    (RevisionGraphDescriptor {checkpoint:IndexCheckpoint {project:p(),revision:RevisionId::new(revision).unwrap(),dialect:candidate(None).manifest.context.model_dialect,graph_digest:ArtifactDigest::of(&bytes)},projection_schema:"test-projection-1".into(),accepted_candidate_digest:digest,artifact_digest:ArtifactDigest::of(&bytes),quad_count:1},bytes)
}

#[test]
fn checkpoint_requires_sealed_fenced_graph_and_retains_accepted_input() {
    let d=tempfile::tempdir().unwrap(); let mut s=setup(&d.path().join("owner.db"));
    let op=accept(&mut s,"op-a","actual-a",None);
    let (descriptor,bytes)=graph("actual-a",op.prepared.as_ref().unwrap().candidate_digest.clone());
    assert!(s.publisher(&a(),&p()).unwrap().record_index(&op.receipt.operation,op.generation,&descriptor.checkpoint).is_err());
    let job=s.claim_index(&a(),&p(),Some(&descriptor.checkpoint.revision),"test-projection-1","worker",10000).unwrap().unwrap();
    assert_eq!(job.candidate.bundle_digest().unwrap(),descriptor.accepted_candidate_digest);
    s.seal_index(&a(),&job.token,&descriptor,&bytes).unwrap();
    s.publish_index(&a(),&job.token).unwrap();
    assert_eq!(s.graph(&a(),&p(),&descriptor.checkpoint.revision,"test-projection-1").unwrap().unwrap(),(descriptor.clone(),bytes));
    assert_eq!(s.operation(&a(),&p(),&op.receipt.operation).unwrap().receipt.status,SyncStatus::Indexed{checkpoint:descriptor.checkpoint});
}

#[test]
fn out_of_order_jobs_do_not_rewind_and_rebuild_uses_retained_candidate() {
    let d=tempfile::tempdir().unwrap(); let path=d.path().join("owner.db"); let mut s=setup(&path);
    let old=accept(&mut s,"old","r-old",None); let new=accept(&mut s,"new","r-new",Some("r-old"));
    let old_job=s.claim_index(&a(),&p(),Some(&RevisionId::new("r-old").unwrap()),"test-projection-1","old-worker",10000).unwrap().unwrap();
    let new_job=s.claim_index(&a(),&p(),Some(&RevisionId::new("r-new").unwrap()),"test-projection-1","new-worker",10000).unwrap().unwrap();
    for (op,job,r) in [(&new,&new_job,"r-new"),(&old,&old_job,"r-old")] {let (g,bytes)=graph(r,op.prepared.as_ref().unwrap().candidate_digest.clone()); s.seal_index(&a(),&job.token,&g,&bytes).unwrap();s.publish_index(&a(),&job.token).unwrap();}
    let state=s.index_state(&a(),&p(),&b(),"test-projection-1").unwrap();
    assert_eq!(state.model_revision,Some(RevisionId::new("r-new").unwrap()));assert_eq!(state.available_graph.as_ref().unwrap().checkpoint.revision,RevisionId::new("r-new").unwrap());
    assert!(s.is_ancestor(&a(),&p(),&RevisionId::new("r-old").unwrap(),&RevisionId::new("r-new").unwrap()).unwrap());
    let before=state.available_graph.unwrap();s.remove_projection(&a(),&p(),&before.checkpoint.revision,"test-projection-1").unwrap();
    assert!(s.graph(&a(),&p(),&before.checkpoint.revision,"test-projection-1").is_err());drop(s);
    let mut s=Store::open(path).unwrap();let job=s.claim_index(&a(),&p(),Some(&before.checkpoint.revision),"test-projection-1","rebuild",10000).unwrap().unwrap();let (after,bytes)=graph("r-new",job.candidate.bundle_digest().unwrap());assert_eq!(after,before);s.seal_index(&a(),&job.token,&after,&bytes).unwrap();s.publish_index(&a(),&job.token).unwrap();
}
