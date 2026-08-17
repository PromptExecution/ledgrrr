//! Phase 4 integration test (gh#111): runs `ReconcileRunner` end-to-end
//! against whatever real `aws`/`gcloud`/`az`/`hf` CLIs and credentials
//! actually exist in this environment.
//!
//! Full-green, all-`Pass` results across every provider require the
//! human-only cloud auth steps tracked in the Phase 2 tracking issues (see
//! #111) — this test does not depend on that. The report-shape assertions
//! below hold unconditionally, since `ReconcileRunner::run` is contractually
//! required to degrade every provider to a `Fail`/`Skip` status rather than
//! erroring when a CLI or credential is missing. Provider-specific
//! Pass-when-authenticated assertions are additionally guarded with an
//! env-var check that SKIPs (not fails) when that provider isn't
//! configured, mirroring this workspace's existing live-test convention
//! (see `live_openmetadata_provider_lists_prefixed_tools_when_configured` in
//! `crates/ledgerr-mcp/tests/mcp_provider_smoke.rs`).

use ledgerr_cloud::{ReconcileRunner, ReconcileStatus};

#[tokio::test]
async fn reconcile_report_is_always_well_formed() {
    let report = ReconcileRunner::default_providers().run().await;

    assert_eq!(
        report.providers.len(),
        4,
        "expected exactly one entry per provider, got: {:?}",
        report.providers
    );

    let names: Vec<&str> = report.providers.iter().map(|p| p.provider.as_str()).collect();
    assert_eq!(names, ["aws", "gcp", "azure", "hf"]);

    for p in &report.providers {
        if p.cap_cake.is_none() {
            assert!(
                !p.authoritative,
                "provider {} has no cap but claims authoritative",
                p.provider
            );
        }
    }
}

#[tokio::test]
async fn hf_is_always_structurally_skip() {
    // HF Jobs has no billing/budget API at all — this holds regardless of
    // environment, unlike the other providers' auth-dependent assertions.
    let report = ReconcileRunner::default_providers().run().await;
    let hf = report
        .providers
        .iter()
        .find(|p| p.provider == "hf")
        .expect("hf entry present");
    assert_eq!(hf.status, ReconcileStatus::Skip);
}

#[tokio::test]
async fn aws_check_auth_passes_when_credentials_are_configured() {
    let Ok(_) = std::env::var("AWS_ACCESS_KEY_ID") else {
        eprintln!(
            "skipping live AWS budget-reconcile assertion: AWS_ACCESS_KEY_ID not set (Phase 2 tracking issue, see #111)"
        );
        return;
    };

    let report = ReconcileRunner::default_providers().run().await;
    let aws = report
        .providers
        .iter()
        .find(|p| p.provider == "aws")
        .expect("aws entry present");
    assert_eq!(
        aws.status,
        ReconcileStatus::Pass,
        "AWS_ACCESS_KEY_ID is set but check_auth/fetch_budget did not pass: {:?}",
        aws.error
    );
}
