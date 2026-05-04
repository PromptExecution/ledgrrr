//! Integration tests for b00t inter-node capability discovery/trading.
//!
//! Each test simulates a two-node scenario: node A writes its capability
//! document to a temp directory; node B is configured with `peer_path_override`
//! pointing at that document, so it can discover A without touching the real
//! `~/.b00t/mesh/` tree.

#![cfg(feature = "b00t")]

use b00t_iface::handshake::{
    CapabilityKind, CapabilityOffer, HandshakeDocument, HandshakeResult, HandshakeSurface,
};
use b00t_iface::core::ProcessSurface;
use serde_json;
use std::collections::HashMap;
use tempfile::TempDir;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Build a ready `HandshakeSurface` for "node A" that has already written its
/// capability document to `<tmp>/node_a/handshake/l3dg3rr.json`.
///
/// Returns the surface and the path to the written document so node B can
/// configure `peer_path_override` to point at it.
fn node_a_surface(
    tmp: &TempDir,
    variant_id: &str,
) -> Result<(HandshakeSurface, std::path::PathBuf), Box<dyn std::error::Error>> {
    let hs_dir = tmp.path().join("node_a").join("handshake");
    let mut node_a = HandshakeSurface::new("node-a", variant_id, "host-a");
    node_a.handshake_dir = hs_dir;
    node_a.write_doc()?;
    let doc_path = node_a.doc_path();
    Ok((node_a, doc_path))
}

/// Build a `HandshakeSurface` for "node B" whose peer document path is set to
/// `peer_doc_path` (i.e. node A's written document).
fn node_b_surface(
    tmp: &TempDir,
    variant_id: &str,
    peer_doc_path: std::path::PathBuf,
) -> HandshakeSurface {
    let hs_dir = tmp.path().join("node_b").join("handshake");
    let node_b = HandshakeSurface::new("node-b", variant_id, "host-b");
    let mut node_b = node_b.with_peer_path(peer_doc_path);
    node_b.handshake_dir = hs_dir;
    node_b
}

// ── test 1 ───────────────────────────────────────────────────────────────────

/// Full two-node handshake: A writes its doc (surfaces + models), B discovers
/// A and gets `Matched` with non-empty `peer_surfaces` and `peer_models`.
#[test]
fn two_node_full_handshake() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let variant = "l3dg3rr";

    let (_node_a, a_doc_path) = node_a_surface(&tmp, variant)?;

    // Verify A wrote the expected surfaces/models in its document.
    let raw = std::fs::read_to_string(&a_doc_path)?;
    let doc: HandshakeDocument = serde_json::from_str(&raw)?;
    assert!(
        doc.surfaces.iter().any(|s| s == "datum-watcher"),
        "expected datum-watcher in A's surfaces; got {:?}",
        doc.surfaces
    );
    assert!(
        doc.surfaces.iter().any(|s| s == "llm-machine"),
        "expected llm-machine in A's surfaces; got {:?}",
        doc.surfaces
    );
    assert!(
        doc.models.iter().any(|m| m == "phi-4-mini-reasoning"),
        "expected phi-4-mini-reasoning in A's models; got {:?}",
        doc.models
    );

    // B discovers A.
    let node_b = node_b_surface(&tmp, variant, a_doc_path);
    let handle = node_b.operate()?;

    assert_eq!(
        handle.result,
        HandshakeResult::Matched,
        "expected Matched; got {:?}",
        handle.result
    );
    assert!(
        !handle.peer_surfaces.is_empty(),
        "peer_surfaces must be non-empty after Matched handshake"
    );
    assert!(
        !handle.peer_models.is_empty(),
        "peer_models must be non-empty after Matched handshake"
    );

    Ok(())
}

// ── test 2 ───────────────────────────────────────────────────────────────────

/// Capability trading — model endpoint offer.
///
/// Node A advertises a `CapabilityOffer` with a model endpoint.
/// Node B discovers A; `handle.acquired` contains the offer including its URL.
#[test]
fn capability_trade_model_endpoint() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let variant = "l3dg3rr";
    let endpoint = "http://127.0.0.1:15115/v1/chat/completions";

    let offer = CapabilityOffer {
        kind: CapabilityKind::Model,
        name: "phi-4-mini-reasoning".into(),
        endpoint: Some(endpoint.into()),
        api_key: Some("local-tool-tray".into()),
        params: HashMap::new(),
    };

    let hs_dir = tmp.path().join("node_a").join("handshake");
    let mut node_a = HandshakeSurface::new("node-a", variant, "host-a")
        .with_offers(vec![offer]);
    node_a.handshake_dir = hs_dir;
    node_a.write_doc()?;
    let a_doc_path = node_a.doc_path();

    let mut node_b = HandshakeSurface::new("node-b", variant, "host-b")
        .with_peer_path(a_doc_path);
    node_b.handshake_dir = tmp.path().join("node_b").join("handshake");

    let handle = node_b.operate()?;

    assert_eq!(handle.result, HandshakeResult::Matched);
    assert!(
        !handle.acquired.is_empty(),
        "acquired must be non-empty after Matched handshake with offers"
    );
    let model_offer = handle.acquired.iter().find(|o| o.name == "phi-4-mini-reasoning");
    assert!(model_offer.is_some(), "acquired must contain the phi model offer");
    assert_eq!(
        model_offer.unwrap().endpoint.as_deref(),
        Some(endpoint),
        "endpoint URL must be preserved through handshake"
    );

    Ok(())
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// Variant mismatch: A advertises `variant_id = "l3dg3rr"`, B expects
/// `"other-tool"`.  Result must be `VariantMismatch`.
#[test]
fn variant_mismatch_rejected() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;

    // A writes its doc with variant "l3dg3rr".
    let (_node_a, a_doc_path) = node_a_surface(&tmp, "l3dg3rr")?;

    // B expects "other-tool".
    let node_b = node_b_surface(&tmp, "other-tool", a_doc_path);
    let handle = node_b.operate()?;

    match &handle.result {
        HandshakeResult::VariantMismatch { expected, got } => {
            assert_eq!(expected, "other-tool", "expected field should be B's variant");
            assert_eq!(got, "l3dg3rr", "got field should be A's variant");
        }
        other => panic!("expected VariantMismatch, got {:?}", other),
    }

    Ok(())
}

// ── test 4 ───────────────────────────────────────────────────────────────────

/// No peer document: B's peer path points to a non-existent file.
/// Result must be `NoPeer` and `peer_surfaces` must be empty.
#[test]
fn no_peer_doc_is_no_peer() -> Result<(), Box<dyn std::error::Error>> {
    let tmp = TempDir::new()?;
    let ghost_path = tmp.path().join("ghost").join("l3dg3rr.handshake");
    // ghost_path does not exist — no std::fs::write here.

    let node_b = node_b_surface(&tmp, "l3dg3rr", ghost_path);
    let handle = node_b.operate()?;

    assert_eq!(
        handle.result,
        HandshakeResult::NoPeer,
        "expected NoPeer when peer doc absent; got {:?}",
        handle.result
    );
    assert!(
        handle.peer_surfaces.is_empty(),
        "peer_surfaces must be empty when NoPeer"
    );
    assert!(
        handle.peer_models.is_empty(),
        "peer_models must be empty when NoPeer"
    );

    Ok(())
}
