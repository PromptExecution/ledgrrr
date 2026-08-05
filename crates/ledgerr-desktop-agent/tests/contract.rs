//! b00t/MCP contract stability — PRD-10 §8 "b00t contract fixtures" done
//! criterion: the `ledgrrr_*` tool registry and schemas must not silently
//! drift.

use ledgerr_desktop_agent::contract::{self, TOOL_REGISTRY};
use ledgerr_desktop_agent::office_artifact;
use ledgerr_desktop_agent::playbook::PlaybookModel;

fn load_fixture(name: &str) -> PlaybookModel {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let body = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    serde_json::from_str(&body).unwrap_or_else(|e| panic!("parse {path}: {e}"))
}

#[test]
fn tool_registry_matches_prd10_eleven_tool_surface() {
    assert_eq!(TOOL_REGISTRY.len(), 11);
    for name in TOOL_REGISTRY {
        assert!(
            name.starts_with("ledgrrr_"),
            "{name} must use the ledgrrr_ prefix"
        );
    }
}

#[test]
fn tool_descriptors_cover_every_registered_tool_with_a_schema() {
    let descriptors = contract::tool_descriptors();
    assert_eq!(descriptors.len(), TOOL_REGISTRY.len());
    for descriptor in &descriptors {
        let name = descriptor["name"].as_str().expect("name field");
        assert!(
            TOOL_REGISTRY.contains(&name),
            "unexpected tool in descriptors: {name}"
        );
        assert!(
            descriptor["inputSchema"].is_object(),
            "{name} missing inputSchema"
        );
    }
}

#[test]
fn dispatch_rejects_unknown_tool_name() {
    let err = contract::dispatch("ledgrrr_does_not_exist", &serde_json::json!({})).unwrap_err();
    assert!(
        matches!(err, contract::ToolError::UnknownTool(name) if name == "ledgrrr_does_not_exist")
    );
}

#[test]
fn export_office_artifact_bumps_version_on_repeat_export_and_never_overwrites() {
    let tmp = tempfile_dir();
    std::env::set_var("LEDGRRR_STATE_DIR", &tmp);

    let model = load_fixture("sample-playbook-linear.json");
    let first = office_artifact::export(&model).expect("first export");
    let second = office_artifact::export(&model).expect("second export");

    assert_eq!(first.artifact_version, 1);
    assert_eq!(second.artifact_version, 2);
    assert_ne!(first.bundle_dir, second.bundle_dir);
    // Refreshing must not touch the previous version's files.
    assert!(std::path::Path::new(&first.playbook_json_path).exists());
    assert!(std::path::Path::new(&second.playbook_json_path).exists());

    std::env::remove_var("LEDGRRR_STATE_DIR");
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn packaging_script_exists_and_is_executable() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("scripts").join("package-desktop-agent.sh"))
        .expect("failed to resolve script path");

    assert!(path.is_file(), "package-desktop-agent.sh must exist at {path:?}");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path).expect("failed to read metadata");
        assert!(meta.permissions().mode() & 0o111 != 0, "script must be executable");
    }
}

fn tempfile_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ledgrrr-contract-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&dir).expect("create temp state dir");
    dir
}
