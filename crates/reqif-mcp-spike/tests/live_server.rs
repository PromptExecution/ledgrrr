//! End-to-end proof against a real, running `reqif-opa-mcp` server.
//!
//! `#[ignore]`d by default: it requires a checkout of
//! `PromptExecution/reqif-opa-mcp` with its Python deps installed
//! (`uv sync --extra ingest-lite`) and a server already started via
//! `just serve <port>` (or `uv run python -m reqif_mcp --http --port <port>`),
//! neither of which CI or a fresh clone of this repo has. Run manually:
//!
//! ```sh
//! REQIF_MCP_URL=http://localhost:8123 cargo test -p reqif-mcp-spike \
//!     --test live_server -- --ignored --nocapture
//! ```
//!
//! Verified working end-to-end 2026-08-22 against `nist_ssdf_dogfood.reqif`
//! and `owasp_asvs_cwe.reqif` from reqif-opa-mcp's own
//! `samples/standards/derived/`.

use std::{env, fs};

use chrono::Utc;
use reqif_mcp_spike::{requirement_record_to_node, McpHttpClient, RequirementRecord};
use serde_json::json;

#[test]
#[ignore = "requires a live reqif-opa-mcp server; see module docs"]
fn parses_and_converts_nist_ssdf_sample() {
    let base_url = env::var("REQIF_MCP_URL").expect("set REQIF_MCP_URL to a running reqif-opa-mcp server");
    let reqif_path = env::var("REQIF_SAMPLE_PATH")
        .unwrap_or_else(|_| "samples/standards/derived/nist_ssdf_dogfood.reqif".to_string());

    let xml = fs::read_to_string(&reqif_path)
        .unwrap_or_else(|e| panic!("could not read {reqif_path}: {e}"));
    let xml_b64 = base64_encode(xml.as_bytes());

    let mut client = McpHttpClient::new(base_url);
    client
        .initialize("reqif-mcp-spike", env!("CARGO_PKG_VERSION"))
        .expect("initialize handshake");

    let parsed = client
        .call_tool(
            "reqif_parse",
            json!({
                "xml_b64": xml_b64,
                "policy_baseline_id": "nist-ssdf",
                "policy_baseline_version": "2026.01",
            }),
        )
        .expect("reqif_parse");
    let handle = parsed["handle"].as_str().expect("handle field").to_string();
    let requirement_count = parsed["requirement_count"].as_u64().expect("requirement_count");
    assert!(requirement_count > 0, "expected at least one requirement");

    let queried = client
        .call_tool("reqif_query", json!({ "handle": handle }))
        .expect("reqif_query");
    let requirements: Vec<RequirementRecord> =
        serde_json::from_value(queried["requirements"].clone()).expect("deserialize requirement records");
    assert_eq!(requirements.len() as u64, requirement_count);

    let now = Utc::now();
    let nodes: Vec<_> = requirements
        .iter()
        .map(|rec| requirement_record_to_node(rec, now))
        .collect();

    for (rec, node) in requirements.iter().zip(nodes.iter()) {
        assert_eq!(node.requirement_id, rec.uid);
        assert_eq!(node.title, rec.key);
        assert_eq!(node.status, rec.status);
        println!(
            "{} -> ArtifactKind::Requirement node_id={:?}",
            rec.key,
            node.node_id()
        );
    }
}

/// Tiny inline base64 encoder so this test doesn't need a new dependency
/// just for a one-shot request payload.
fn base64_encode(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied();
        let b2 = chunk.get(2).copied();
        out.push(ALPHABET[(b0 >> 2) as usize] as char);
        out.push(ALPHABET[(((b0 & 0x03) << 4) | (b1.unwrap_or(0) >> 4)) as usize] as char);
        if let Some(b1) = b1 {
            out.push(ALPHABET[(((b1 & 0x0f) << 2) | (b2.unwrap_or(0) >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if let Some(b2) = b2 {
            out.push(ALPHABET[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}
