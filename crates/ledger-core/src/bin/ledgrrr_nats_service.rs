//! Exposes ledgrrr's own classification logic as a NATS Micro Service —
//! this, not `reqif-opa-mcp`'s Docling wrapper, is the "shared ledgrrr":
//! ledgrrr's deterministic bank-statement classifier
//! (`ledger_core::bank_statement`) becomes network-reachable and
//! discoverable via the standard `nats service list` protocol, the same
//! way `reqif-opa-mcp`'s `nats_docling_service.py` already exposes Docling
//! extraction. The two are meant to be chained by a caller: extract via
//! `ledgrrr-docling`'s `ledgrrr.extract`, then classify+bridge via this
//! service's `ledgrrr.classify`.
//!
//! Endpoint: "classify" in group "ledgrrr" (subject `ledgrrr.classify`).
//! Request: `{"graph": <DoclingDocumentGraph JSON>, "account_id": "..."}`.
//! Reply: `ClassifyResponse` JSON, or a NATS service error on failure.
//!
//! Run: `cargo run -p ledger-core --features nats-service --bin ledgrrr-nats-service`
//! Env: NATS_URL (default nats://127.0.0.1:4222), NATS_USER, NATS_PASSWORD.

use async_nats::service::ServiceExt;
use futures_util::StreamExt;
use ledger_core::bank_statement::{
    classify_document, extract_statement_header, node_to_transaction_input, NodeCategory,
};
use ledger_core::docling_bridge::DoclingDocumentGraph;
use ledger_core::ingest::TransactionInput;
use serde::{Deserialize, Serialize};

const SERVICE_NAME: &str = "ledgrrr";
const SERVICE_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Deserialize)]
struct ClassifyRequest {
    graph: DoclingDocumentGraph,
    #[serde(default = "default_account_id")]
    account_id: String,
}

fn default_account_id() -> String {
    "unknown".to_string()
}

#[derive(Debug, Serialize)]
struct ClassifiedNodeSummary {
    node_id: String,
    category: NodeCategory,
    subtypes: Vec<String>,
    satisfied: bool,
    confidence: f64,
}

#[derive(Debug, Serialize)]
struct ClassifyResponse {
    classified: Vec<ClassifiedNodeSummary>,
    transactions: Vec<TransactionInput>,
    statement_header: Option<ledger_core::bank_statement::StatementHeader>,
}

fn handle_request(payload: &[u8]) -> Result<Vec<u8>, String> {
    let request: ClassifyRequest =
        serde_json::from_slice(payload).map_err(|e| format!("invalid request JSON: {e}"))?;

    let classified = classify_document(&request.graph);

    let mut transactions = Vec::new();
    let mut summaries = Vec::with_capacity(classified.len());
    for c in &classified {
        summaries.push(ClassifiedNodeSummary {
            node_id: c.node.node_id.clone(),
            category: c.category,
            subtypes: c.category.sarif_subtypes(),
            satisfied: c.result.disposition.is_satisfied(),
            confidence: c.result.confidence,
        });
        if c.category == NodeCategory::TransactionRow {
            if let Ok(tx) = node_to_transaction_input(c, &request.account_id) {
                transactions.push(tx);
            }
        }
    }

    let statement_header = extract_statement_header(&request.graph);

    let response = ClassifyResponse {
        classified: summaries,
        transactions,
        statement_header,
    };
    serde_json::to_vec(&response).map_err(|e| format!("failed to serialize response: {e}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".to_string());
    let mut options = async_nats::ConnectOptions::new();
    if let (Ok(user), Ok(password)) = (std::env::var("NATS_USER"), std::env::var("NATS_PASSWORD")) {
        options = options.user_and_password(user, password);
    }
    let client = options.connect(&url).await?;

    let service = client
        .service_builder()
        .description("ledgrrr's deterministic (non-LLM) document classification: bank-statement node categorization + TransactionInput bridging over a DoclingDocumentGraph")
        .start(SERVICE_NAME, SERVICE_VERSION)
        .await?;

    let group = service.group("ledgrrr");
    let mut endpoint = group.endpoint("classify").await?;

    println!("[{SERVICE_NAME}] listening on '{url}' as subject 'ledgrrr.classify'");

    while let Some(request) = endpoint.next().await {
        let result = handle_request(&request.message.payload);
        match result {
            Ok(bytes) => {
                if let Err(e) = request.respond(Ok(bytes.into())).await {
                    eprintln!("failed to send reply: {e}");
                }
            }
            Err(msg) => {
                if let Err(e) = request
                    .respond(Err(async_nats::service::error::Error {
                        code: 400,
                        status: msg,
                    }))
                    .await
                {
                    eprintln!("failed to send error reply: {e}");
                }
            }
        }
    }

    Ok(())
}
