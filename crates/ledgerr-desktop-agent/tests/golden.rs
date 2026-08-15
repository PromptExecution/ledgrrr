//! Golden/determinism tests — PRD-11 §9 "Diagram and Office" done criteria:
//! "Mermaid/SVG/PNG exports are deterministic for a fixed playbook input."
//! Fixtures live in `tests/fixtures/*.json`, not embedded in test code.

use ledgerr_desktop_agent::playbook::PlaybookModel;
use ledgerr_desktop_agent::render;
use ledgerr_desktop_agent::simulate::{self, StepStatus};

fn load_fixture(name: &str) -> PlaybookModel {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn mermaid_render_is_deterministic() {
    let model = load_fixture("sample-playbook-linear.json");
    let first = render::render_mermaid(&model).expect("render");
    let second = render::render_mermaid(&model).expect("render");
    assert_eq!(first, second);
    assert!(first.starts_with("flowchart TD\n"));
    assert!(first.contains("start"));
    assert!(first.contains("--> ingest"));
}

#[test]
fn json_render_is_deterministic_and_round_trips() {
    let model = load_fixture("sample-playbook-gated.json");
    let first = render::render_json(&model).expect("render");
    let second = render::render_json(&model).expect("render");
    assert_eq!(first, second);

    let round_tripped: PlaybookModel = serde_json::from_str(&first).expect("parse rendered json");
    assert_eq!(round_tripped, model);
}

#[test]
fn svg_render_contains_one_rect_per_node() {
    let model = load_fixture("sample-playbook-linear.json");
    let svg = render::render_svg(&model).expect("render");
    assert_eq!(svg.matches("<rect").count(), model.nodes.len());
}

#[test]
fn png_format_is_explicitly_unsupported() {
    let model = load_fixture("sample-playbook-linear.json");
    let err = render::render(&model, "png").unwrap_err();
    assert!(matches!(err, render::RenderError::UnsupportedFormat(_)));
}

#[test]
fn simulation_run_id_is_deterministic_for_fixed_input() {
    let model = load_fixture("sample-playbook-linear.json");
    let first = simulate::simulate(&model, simulate::DETERMINISTIC_PROFILE).expect("simulate");
    let second = simulate::simulate(&model, simulate::DETERMINISTIC_PROFILE).expect("simulate");
    assert_eq!(first.run_id, second.run_id);
    assert_eq!(first.evidence_ids, second.evidence_ids);
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

#[test]
fn simulation_run_id_changes_with_profile() {
    let model = load_fixture("sample-playbook-linear.json");
    let deterministic = simulate::simulate(&model, "deterministic").expect("simulate");
    let local_cpu = simulate::simulate(&model, "local-cpu").expect("simulate");
    assert_ne!(deterministic.run_id, local_cpu.run_id);
}

#[test]
fn linear_playbook_executes_every_node() {
    let model = load_fixture("sample-playbook-linear.json");
    let trace = simulate::simulate(&model, simulate::DETERMINISTIC_PROFILE).expect("simulate");
    assert_eq!(trace.steps.len(), model.nodes.len());
    assert!(trace.steps.iter().all(|s| s.status == StepStatus::Executed));
    assert!(trace.gate_decisions.is_empty());
}

#[test]
fn required_gate_without_auto_approve_halts_traversal() {
    let model = load_fixture("sample-playbook-gated.json");
    let trace = simulate::simulate(&model, simulate::DETERMINISTIC_PROFILE).expect("simulate");

    // start -> ingest -> approve(blocked); export/end never execute.
    assert_eq!(trace.steps.len(), 3);
    let gate_step = trace.steps.last().expect("at least one step");
    assert_eq!(gate_step.node_id, "approve");
    assert_eq!(gate_step.status, StepStatus::GateBlocked);
    assert_eq!(trace.gate_decisions.len(), 1);
    assert_eq!(
        trace.gate_decisions[0].decision,
        simulate::GateDecisionKind::Pending
    );
    assert!(!trace
        .steps
        .iter()
        .any(|s| s.node_id == "export" || s.node_id == "end"));
}

#[test]
fn validate_rejects_playbook_without_start_node() {
    let mut model = load_fixture("sample-playbook-linear.json");
    for node in &mut model.nodes {
        if node.kind == ledgerr_desktop_agent::playbook::NodeKind::Start {
            node.kind = ledgerr_desktop_agent::playbook::NodeKind::Task;
        }
    }
    let err = model.validate().unwrap_err();
    assert!(matches!(
        err,
        ledgerr_desktop_agent::playbook::PlaybookError::NoStartNode
    ));
}

#[test]
fn validate_rejects_edge_to_unknown_node() {
    let mut model = load_fixture("sample-playbook-linear.json");
    model
        .edges
        .push(ledgerr_desktop_agent::playbook::PlaybookEdge {
            from: "ingest".to_string(),
            to: "does-not-exist".to_string(),
            label: None,
            outcome: None,
        });
    let err = model.validate().unwrap_err();
    assert!(matches!(
        err,
        ledgerr_desktop_agent::playbook::PlaybookError::UnknownNode(id) if id == "does-not-exist"
    ));
}

#[test]
fn ooda_learning_is_a_deterministic_authorized_state_machine() {
    let model = load_fixture("b00t-learn-ooda.json");
    model.validate().expect("OODA authorization boundary is valid");

    let rendered = render::render(&model, "state-machine").expect("render state machine");
    assert!(rendered.starts_with("stateDiagram-v2\n"));
    assert!(rendered.contains("observe --> orient : observation_captured"));
    assert!(rendered.contains("decide --> act : memoization_authorized"));

    let trace = simulate::simulate(&model, simulate::DETERMINISTIC_PROFILE).expect("simulate");
    assert_eq!(trace.steps.len(), 6);
    assert_eq!(trace.steps[1].outcome, "observation_captured");
    assert_eq!(trace.steps[1].execution_role.as_deref(), Some("governance-agent"));
    assert_eq!(trace.steps[1].capability.as_deref(), Some("b00t.learn"));
    assert_eq!(trace.steps[3].status, StepStatus::Executed);
    assert_eq!(trace.steps[4].outcome, "learning_memo_recorded");
}

#[test]
fn role_cannot_exceed_declared_b00t_capabilities() {
    let mut model = load_fixture("b00t-learn-ooda.json");
    model.role_authorizations[0]
        .capabilities
        .clear();
    let error = model.validate().expect_err("missing grant must fail closed");
    assert!(matches!(
        error,
        ledgerr_desktop_agent::playbook::PlaybookError::UnauthorizedRole { node_id, role, capability }
            if node_id == "observe" && role == "governance-agent" && capability == "b00t.learn"
    ));
}

#[test]
fn process_cannot_invoke_an_undeclared_b00t_capability() {
    let mut model = load_fixture("b00t-learn-ooda.json");
    model.capability_refs.clear();
    let error = model.validate().expect_err("undeclared capability must fail closed");
    assert!(matches!(
        error,
        ledgerr_desktop_agent::playbook::PlaybookError::UndeclaredCapability { node_id, capability }
            if node_id == "observe" && capability == "b00t.learn"
    ));
}
