//! Handshake — b00t ↔ l3dg3rr variant protocol.
//!
//! When b00t and l3dg3rr are on the same system, they exchange state using
//! a handshake variant that does not presently exist in either codebase.
//! This module defines that variant as a formal b00t surface.
//!
//! # Handshake protocol
//!
//! 1. Discover: each side advertises its capability document on a well-known
//!    path. b00t writes to `~/.b00t/mesh/l3dg3rr.handshake`, l3dg3rr writes
//!    to `_b00t_/handshake/l3dg3rr.json`.
//! 2. Verify: each side reads the other's capability and validates it against
//!    the governance policy.
//! 3. Exchange: surfaces, models, and audit logs are shared.
//! 4. Monitor: heartbeat pings at configurable interval.
//!
//! The handshake IS the integration — it's a b00t surface that, on operate(),
//! performs the full exchange.

use crate::core::{
    AuditRecord, GovernancePolicy, MaintenanceAction, ProcessSurface, Requirement,
    SurfaceCapability,
};
use crate::AgentRole;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// Category of capability being offered by a node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Model,
    Surface,
    Tool,
    Datum,
}

/// A single capability a node is willing to share with a peer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityOffer {
    pub kind: CapabilityKind,
    pub name: String,
    /// Endpoint the peer can call directly (e.g. an OpenAI-compatible URL).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// API key for the endpoint — only safe for local mesh traffic.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub api_key: Option<String>,
    /// Arbitrary extra metadata (model params, surface config, etc.).
    #[serde(default)]
    pub params: HashMap<String, String>,
}

/// An indexed view of capabilities acquired from a peer after a successful handshake.
#[derive(Debug, Clone, Default)]
pub struct CapabilityRegistry(pub Vec<CapabilityOffer>);

impl CapabilityRegistry {
    pub fn models(&self) -> Vec<&CapabilityOffer> {
        self.0.iter().filter(|o| o.kind == CapabilityKind::Model).collect()
    }

    pub fn surfaces(&self) -> Vec<&CapabilityOffer> {
        self.0.iter().filter(|o| o.kind == CapabilityKind::Surface).collect()
    }

    pub fn find(&self, name: &str) -> Option<&CapabilityOffer> {
        self.0.iter().find(|o| o.name == name)
    }
}

/// The capability document exchanged between b00t and l3dg3rr.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandshakeDocument {
    /// Sender identity (e.g. "b00t" or "l3dg3rr").
    pub sender: String,
    /// Variant ID matching the node datum.
    pub variant_id: String,
    /// Hostname.
    pub host: String,
    /// Available surfaces (short names) — derived from `offers` for backwards compat.
    pub surfaces: Vec<String>,
    /// Available LLM models — derived from `offers` for backwards compat.
    pub models: Vec<String>,
    /// Protocol version for forward compatibility.
    pub version: String,
    /// Rich capability offers; supersedes `surfaces`/`models` when non-empty.
    #[serde(default)]
    pub offers: Vec<CapabilityOffer>,
}

/// Handshake result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeResult {
    Matched,
    VariantMismatch { expected: String, got: String },
    NoPeer,
}

impl std::fmt::Display for HandshakeResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Matched => write!(f, "matched"),
            Self::VariantMismatch { expected, got } => {
                write!(f, "variant mismatch: expected '{expected}', got '{got}'")
            }
            Self::NoPeer => write!(f, "no peer"),
        }
    }
}

/// The handshake surface — performs b00t↔l3dg3rr peer discovery and state exchange.
pub struct HandshakeSurface {
    pub identity: String,
    pub variant_id: String,
    pub host: String,
    pub handshake_dir: std::path::PathBuf,
    pub heartbeat_interval: Duration,
    pub result: Option<HandshakeResult>,
    pub peer_doc: Option<HandshakeDocument>,
    /// Optional override for the peer document path. When set, `read_peer` reads
    /// from this path instead of the default `~/.b00t/mesh/l3dg3rr.handshake`.
    /// Intended for testing two-node scenarios without touching the real home dir.
    pub peer_path_override: Option<std::path::PathBuf>,
    /// Capabilities this node advertises to peers.
    pub local_offers: Vec<CapabilityOffer>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HandshakeConfig {
    pub identity: String,
    pub variant_id: String,
    pub host: String,
    #[serde(default = "default_handshake_dir")]
    pub handshake_dir: String,
    #[serde(default = "default_heartbeat")]
    pub heartbeat_secs: u64,
}

fn default_handshake_dir() -> String {
    "_b00t_/handshake".into()
}

fn default_heartbeat() -> u64 {
    30
}

#[derive(Debug, Clone)]
pub enum HandshakeError {
    Dir(String),
    Write(String),
    Read(String),
    Parse(String),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Dir(e) => write!(f, "handshake dir: {e}"),
            Self::Write(e) => write!(f, "handshake write: {e}"),
            Self::Read(e) => write!(f, "handshake read: {e}"),
            Self::Parse(e) => write!(f, "handshake parse: {e}"),
        }
    }
}

impl std::error::Error for HandshakeError {}

/// Handle returned by operate().
#[derive(Debug, Clone)]
pub struct HandshakeHandle {
    pub result: HandshakeResult,
    pub peer_surfaces: Vec<String>,
    pub peer_models: Vec<String>,
    /// Rich capabilities acquired from the peer after a successful handshake.
    pub acquired: Vec<CapabilityOffer>,
}

impl HandshakeSurface {
    pub fn new(identity: &str, variant_id: &str, host: &str) -> Self {
        Self {
            identity: identity.to_owned(),
            variant_id: variant_id.to_owned(),
            host: host.to_owned(),
            handshake_dir: Path::new("_b00t_").join("handshake"),
            heartbeat_interval: Duration::from_secs(30),
            result: None,
            peer_doc: None,
            peer_path_override: None,
            local_offers: Vec::new(),
        }
    }

    /// Override the path from which the peer document is read.
    pub fn with_peer_path(mut self, path: std::path::PathBuf) -> Self {
        self.peer_path_override = Some(path);
        self
    }

    /// Set the capabilities this node will advertise to peers.
    pub fn with_offers(mut self, offers: Vec<CapabilityOffer>) -> Self {
        self.local_offers = offers;
        self
    }

    pub fn doc_path(&self) -> std::path::PathBuf {
        self.handshake_dir.join("l3dg3rr.json")
    }

    /// Write our capability document to the handshake dir.
    pub fn write_doc(&self) -> Result<(), HandshakeError> {
        let dir = &self.handshake_dir;
        std::fs::create_dir_all(dir).map_err(|e| HandshakeError::Dir(e.to_string()))?;

        // Derive backwards-compat surface/model lists from offers; fall back to
        // static defaults so existing peers that don't understand `offers` still work.
        let surfaces: Vec<String> = if self.local_offers.is_empty() {
            vec![
                "datum-watcher".into(),
                "autoresearch".into(),
                "llm-machine".into(),
                "opencode-provider".into(),
            ]
        } else {
            self.local_offers
                .iter()
                .filter(|o| o.kind == CapabilityKind::Surface)
                .map(|o| o.name.clone())
                .collect()
        };

        let models: Vec<String> = if self.local_offers.is_empty() {
            vec!["phi-4-mini-reasoning".into()]
        } else {
            self.local_offers
                .iter()
                .filter(|o| o.kind == CapabilityKind::Model)
                .map(|o| o.name.clone())
                .collect()
        };

        let doc = HandshakeDocument {
            sender: self.identity.clone(),
            variant_id: self.variant_id.clone(),
            host: self.host.clone(),
            surfaces,
            models,
            version: "1.0.0".into(),
            offers: self.local_offers.clone(),
        };

        let json =
            serde_json::to_string_pretty(&doc).map_err(|e| HandshakeError::Write(e.to_string()))?;
        std::fs::write(self.doc_path(), json).map_err(|e| HandshakeError::Write(e.to_string()))?;
        Ok(())
    }

    /// Read the peer's capability document.
    ///
    /// Checks `peer_path_override` first; falls back to the canonical
    /// `~/.b00t/mesh/l3dg3rr.handshake` path.
    fn read_peer(&self) -> Result<Option<HandshakeDocument>, HandshakeError> {
        let path = if let Some(override_path) = &self.peer_path_override {
            if override_path.exists() {
                override_path.clone()
            } else {
                return Ok(None);
            }
        } else {
            // b00t writes to ~/.b00t/mesh/l3dg3rr.handshake
            let b00t_path =
                dirs::home_dir().map(|h| h.join(".b00t").join("mesh").join("l3dg3rr.handshake"));
            match b00t_path {
                Some(p) if p.exists() => p,
                _ => return Ok(None),
            }
        };

        let content =
            std::fs::read_to_string(&path).map_err(|e| HandshakeError::Read(e.to_string()))?;
        let doc: HandshakeDocument =
            serde_json::from_str(&content).map_err(|e| HandshakeError::Parse(e.to_string()))?;
        Ok(Some(doc))
    }

    /// Perform the handshake: write our doc, read peer, compare.
    fn perform(&mut self) -> Result<HandshakeResult, HandshakeError> {
        self.write_doc()?;
        match self.read_peer()? {
            Some(peer) => {
                self.peer_doc = Some(peer.clone());
                if peer.variant_id == self.variant_id {
                    self.result = Some(HandshakeResult::Matched);
                    Ok(HandshakeResult::Matched)
                } else {
                    let r = HandshakeResult::VariantMismatch {
                        expected: self.variant_id.clone(),
                        got: peer.variant_id,
                    };
                    self.result = Some(r.clone());
                    Ok(r)
                }
            }
            None => {
                self.result = Some(HandshakeResult::NoPeer);
                Ok(HandshakeResult::NoPeer)
            }
        }
    }
}

impl ProcessSurface for HandshakeSurface {
    type Config = HandshakeConfig;
    type Error = HandshakeError;
    type Handle = HandshakeHandle;

    fn capability(&self) -> SurfaceCapability {
        SurfaceCapability {
            name: "handshake",
            requirements: vec![Requirement::PathExists(
                self.handshake_dir.display().to_string(),
            )],
            governance: GovernancePolicy {
                allowed_starters: vec![AgentRole::Executive],
                max_ttl: Duration::from_secs(3600),
                auto_restart: true,
                crash_budget: 3,
            },
        }
    }

    fn init(&mut self, config: Self::Config) -> Result<(), Self::Error> {
        self.identity = config.identity;
        self.variant_id = config.variant_id;
        self.host = config.host;
        self.handshake_dir = Path::new(&config.handshake_dir).to_path_buf();
        self.heartbeat_interval = Duration::from_secs(config.heartbeat_secs);
        std::fs::create_dir_all(&self.handshake_dir)
            .map_err(|e| HandshakeError::Dir(e.to_string()))?;
        tracing::info!(
            "HandshakeSurface initialized: {}@{}",
            self.identity,
            self.host
        );
        Ok(())
    }

    fn operate(&self) -> Result<Self::Handle, Self::Error> {
        // operate performs the handshake — but since operate takes &self,
        // we use a trick: write our doc, read the peer.
        // The actual handshake state is stored in the filesystem.
        let mut surface_clone = Self {
            identity: self.identity.clone(),
            variant_id: self.variant_id.clone(),
            host: self.host.clone(),
            handshake_dir: self.handshake_dir.clone(),
            heartbeat_interval: self.heartbeat_interval,
            result: None,
            peer_doc: None,
            peer_path_override: self.peer_path_override.clone(),
            local_offers: self.local_offers.clone(),
        };
        let result = surface_clone.perform()?;

        match &result {
            HandshakeResult::Matched => tracing::info!("b00t↔l3dg3rr handshake matched"),
            HandshakeResult::NoPeer => tracing::warn!("b00t↔l3dg3rr handshake: no peer"),
            HandshakeResult::VariantMismatch { expected, got } => {
                tracing::warn!("b00t↔l3dg3rr variant mismatch: expected {expected}, got {got}");
            }
        }

        let peer_doc = surface_clone.peer_doc;
        let acquired = if matches!(result, HandshakeResult::Matched) {
            peer_doc.as_ref().map(|d| d.offers.clone()).unwrap_or_default()
        } else {
            Vec::new()
        };
        Ok(HandshakeHandle {
            result,
            peer_surfaces: peer_doc
                .as_ref()
                .map(|d| d.surfaces.clone())
                .unwrap_or_default(),
            peer_models: peer_doc
                .as_ref()
                .map(|d| d.models.clone())
                .unwrap_or_default(),
            acquired,
        })
    }

    fn terminate(handle: Self::Handle) -> Result<AuditRecord, Self::Error> {
        Ok(AuditRecord {
            surface_name: "handshake".into(),
            uptime: Duration::from_secs(0),
            exit_reason: format!("handshake result: {}", handle.result),
            crash_count: 0,
            bytes_logged: 0,
        })
    }

    fn maintain(&self) -> MaintenanceAction {
        std::fs::create_dir_all(&self.handshake_dir).ok();
        MaintenanceAction::NoOp
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn write_and_read_handshake_doc() {
        let tmp = TempDir::new().unwrap();
        let mut s = HandshakeSurface::new("l3dg3rr", "test-variant", "test-host");
        s.handshake_dir = tmp.path().join("handshake");
        s.write_doc().expect("write doc");

        let doc_path = s.doc_path();
        assert!(doc_path.exists());
        let content = std::fs::read_to_string(doc_path).unwrap();
        assert!(content.contains("l3dg3rr"));
        assert!(content.contains("test-variant"));
    }

    #[test]
    fn handshake_result_display() {
        assert_eq!(HandshakeResult::Matched.to_string(), "matched");
        assert_eq!(HandshakeResult::NoPeer.to_string(), "no peer");
        let mismatch = HandshakeResult::VariantMismatch {
            expected: "a".into(),
            got: "b".into(),
        };
        assert!(mismatch.to_string().contains("variant mismatch"));
    }

    #[test]
    fn init_creates_dir() {
        let tmp = TempDir::new().unwrap();
        let mut s = HandshakeSurface::new("test", "v1", "h1");
        let config = HandshakeConfig {
            identity: "test".into(),
            variant_id: "v1".into(),
            host: "h1".into(),
            handshake_dir: tmp.path().join("custom-hs").display().to_string(),
            heartbeat_secs: 10,
        };
        s.init(config).expect("init");
        assert!(tmp.path().join("custom-hs").exists());
    }

    #[test]
    fn operate_no_peer_returns_nopeer() {
        let tmp = TempDir::new().unwrap();
        let mut s = HandshakeSurface::new("l3dg3rr", "variant-1", "host-1");
        s.handshake_dir = tmp.path().join("hs");
        let config = HandshakeConfig {
            identity: "l3dg3rr".into(),
            variant_id: "variant-1".into(),
            host: "host-1".into(),
            handshake_dir: s.handshake_dir.display().to_string(),
            heartbeat_secs: 30,
        };
        s.init(config).expect("init");
        let handle = s.operate().expect("operate");
        assert_eq!(handle.result, HandshakeResult::NoPeer);
        assert!(handle.acquired.is_empty());
    }

    #[test]
    fn capability_registry_filters() {
        let offers = vec![
            CapabilityOffer {
                kind: CapabilityKind::Model,
                name: "phi-4-mini-reasoning".into(),
                endpoint: Some("http://127.0.0.1:15115/v1/chat/completions".into()),
                api_key: Some("local-tool-tray".into()),
                params: HashMap::new(),
            },
            CapabilityOffer {
                kind: CapabilityKind::Surface,
                name: "datum-watcher".into(),
                endpoint: None,
                api_key: None,
                params: HashMap::new(),
            },
        ];
        let reg = CapabilityRegistry(offers);
        assert_eq!(reg.models().len(), 1);
        assert_eq!(reg.surfaces().len(), 1);
        assert!(reg.find("phi-4-mini-reasoning").is_some());
        assert!(reg.find("nonexistent").is_none());
    }

    #[test]
    fn with_offers_builder_populates_doc() {
        let tmp = TempDir::new().unwrap();
        let offer = CapabilityOffer {
            kind: CapabilityKind::Model,
            name: "phi-4-mini-reasoning".into(),
            endpoint: Some("http://127.0.0.1:15115/v1/chat/completions".into()),
            api_key: Some("local-tool-tray".into()),
            params: HashMap::new(),
        };
        let mut s = HandshakeSurface::new("l3dg3rr", "v1", "host")
            .with_offers(vec![offer.clone()]);
        s.handshake_dir = tmp.path().join("hs");
        s.write_doc().expect("write doc");

        let content = std::fs::read_to_string(s.doc_path()).unwrap();
        assert!(content.contains("phi-4-mini-reasoning"));
        assert!(content.contains("15115"));
    }

    #[test]
    fn capability_offer_roundtrips_json() {
        let offer = CapabilityOffer {
            kind: CapabilityKind::Model,
            name: "test-model".into(),
            endpoint: Some("http://localhost:8080".into()),
            api_key: None,
            params: [("version".to_string(), "2".to_string())].into_iter().collect(),
        };
        let json = serde_json::to_string(&offer).expect("serialize");
        let back: CapabilityOffer = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.name, "test-model");
        assert_eq!(back.kind, CapabilityKind::Model);
        assert_eq!(back.endpoint.as_deref(), Some("http://localhost:8080"));
        assert!(back.api_key.is_none());
    }

    #[test]
    fn handshake_document_with_empty_offers_deserializes() {
        let json = r#"{"sender":"b00t","variant_id":"v1","host":"h1","surfaces":[],"models":[],"version":"1.0.0"}"#;
        let doc: HandshakeDocument = serde_json::from_str(json).expect("deserialize");
        assert!(doc.offers.is_empty());
    }
}
