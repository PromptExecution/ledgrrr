//! b00t/MCP contract stability — PRD-11 §8 "b00t contract fixtures" done
//! criterion: the `ledgrrr_*` tool registry and schemas must not silently
//! drift.

use ledgerr_desktop_agent::contract::{self, TOOL_REGISTRY};
use ledgerr_desktop_agent::install_plan::{self, InstallScope, PrivilegeLevel};
use ledgerr_desktop_agent::office_artifact;
use ledgerr_desktop_agent::playbook::PlaybookModel;
use std::sync::Mutex;

// `LEDGRRR_STATE_DIR` is intentionally a process-wide override for portable
// controller/service invocation. Contract tests that use it must serialize so
// parallel test execution cannot redirect another test's state files.
static STATE_ENV_LOCK: Mutex<()> = Mutex::new(());

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
fn render_tool_exposes_ooda_state_machine_projection() {
    let model = load_fixture("b00t-learn-ooda.json");
    let result = contract::dispatch(
        contract::RENDER_DIAGRAM_TOOL,
        &serde_json::json!({ "playbook": model, "format": "state-machine" }),
    )
    .expect("render state machine");
    assert_eq!(result["format"], "state-machine");
    assert!(result["content"]
        .as_str()
        .expect("state machine content")
        .contains("memoization_authorized"));
}

#[test]
fn package_plan_is_per_user_by_default_and_names_all_owned_paths() {
    let plan = install_plan::install_plan();
    assert_eq!(plan.scope, InstallScope::PerUser);
    assert_eq!(plan.privilege_required, PrivilegeLevel::User);
    assert_eq!(plan.package_family, install_plan::PACKAGE_FAMILY_NAME);
    assert!(
        plan.paths.len() >= 4,
        "plan must cover install/config/data/log paths"
    );
    assert!(plan.unattended_command.contains("-Quiet"));
}

#[test]
fn package_action_requires_plan_approval_before_any_mutation() {
    let result = install_plan::invoke(
        "install_desktop",
        install_plan::PackageActionArgs {
            approved: false,
            scope: InstallScope::Machine,
            package_path: Some("C:\\dogfood\\ledgrrr.msix".to_string()),
        },
    );
    assert!(!result.ok);
    assert!(!result.launched);
    assert_eq!(result.privilege_required, PrivilegeLevel::Elevated);
    assert!(result.message.contains("approval required"));
}

#[test]
fn uninstall_plan_does_not_require_the_original_msix_file() {
    let plan = install_plan::action_plan("uninstall", &Default::default());
    assert!(!plan.unattended_command.contains("-PackagePath"));
    assert!(!plan
        .blocked_reason
        .as_deref()
        .unwrap_or_default()
        .contains("requires an explicit test-signed MSIX"));
}

#[test]
fn repair_plan_uses_cached_test_package_when_present() {
    let _state_env = STATE_ENV_LOCK.lock().expect("lock state environment");
    let temp = tempfile_dir();
    std::env::set_var("LEDGRRR_STATE_DIR", &temp);
    let cache = ledgerr_desktop_agent::state::cached_package_path();
    std::fs::create_dir_all(cache.parent().expect("cache parent")).expect("create cache");
    std::fs::write(&cache, b"test package").expect("write cache marker");

    let plan = install_plan::action_plan("repair", &Default::default());
    assert!(plan
        .unattended_command
        .contains(&cache.display().to_string()));
    assert!(
        plan.blocked_reason
            .as_deref()
            .unwrap_or_default()
            .contains("Windows")
            || plan.executable_now
    );

    std::env::remove_var("LEDGRRR_STATE_DIR");
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn installed_payload_record_drives_status_and_tray_discovery() {
    let _state_env = STATE_ENV_LOCK.lock().expect("lock state environment");
    let temp = tempfile_dir();
    let payload = temp.join("payload");
    std::fs::create_dir_all(&payload).expect("create payload");
    let tray_name = "ledgrrr-tray.exe";
    let tray = payload.join(tray_name);
    std::fs::write(&tray, b"tray").expect("write tray marker");
    std::env::set_var("LEDGRRR_STATE_DIR", &temp);
    ledgerr_desktop_agent::state::write_package_install(
        &ledgerr_desktop_agent::state::PackageInstallRecord {
            schema_version: 1,
            package_family: install_plan::PACKAGE_FAMILY_NAME.to_string(),
            external_payload_dir: payload.display().to_string(),
            scope: "per_user".to_string(),
            installed_at_unix: 1,
        },
    )
    .expect("write install record");

    let status = ledgerr_desktop_agent::status::collect();
    assert!(status.desktop_package.installed);
    assert_eq!(
        status.desktop_package.install_location.as_deref(),
        Some(payload.to_str().expect("payload string"))
    );
    assert_eq!(
        status.tray.binary_path.as_deref(),
        Some(tray.to_str().expect("tray string"))
    );

    std::env::remove_var("LEDGRRR_STATE_DIR");
    let _ = std::fs::remove_dir_all(temp);
}

#[test]
fn status_keeps_desktop_controller_distinct_from_ledger_catalog() {
    let status = ledgerr_desktop_agent::status::collect();
    assert_eq!(status.claude_controller.expected_tools, 11);
    assert_eq!(
        status.desktop_package.package_family,
        install_plan::PACKAGE_FAMILY_NAME
    );
}

#[test]
fn export_office_artifact_bumps_version_on_repeat_export_and_never_overwrites() {
    let _state_env = STATE_ENV_LOCK.lock().expect("lock state environment");
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

    assert!(
        path.is_file(),
        "package-desktop-agent.sh must exist at {path:?}"
    );

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path).expect("failed to read metadata");
        assert!(
            meta.permissions().mode() & 0o111 != 0,
            "script must be executable"
        );
    }
}

#[test]
fn sparse_msix_contract_declares_external_content_and_package_script_uses_it() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("resolve workspace root");
    let manifest = std::fs::read_to_string(root.join("windows/package/AppxManifest.xml"))
        .expect("read sparse MSIX manifest");
    let support_manifest =
        std::fs::read_to_string(root.join("windows/package/support-manifest.json"))
            .expect("read package support manifest");
    let script = std::fs::read_to_string(root.join("windows/package/ledgrrr-package.ps1"))
        .expect("read package script");
    assert!(manifest.contains("AllowExternalContent>true"));
    assert!(manifest.contains("runFullTrust"));
    assert!(support_manifest.contains("ledgrrr-mcp.exe"));
    assert!(support_manifest.contains("ledgrrr-service.exe"));
    assert!(support_manifest.contains("ledgrrr-tray.exe"));
    assert!(script.contains("ExternalLocation"));
    assert!(script.contains("Add-AppxPackage @installArgs"));
    assert!(script.contains("Write-PackageInstallRecord"));
    assert!(script.contains("Assert-ExternalPayload"));
    assert!(script.contains("-PayloadPath"));
    assert!(script.contains("-CertificatePath"));

    let build_script = std::fs::read_to_string(root.join("scripts/windows-package.ps1"))
        .expect("read Windows package build script");
    assert!(build_script.contains("Compress-Archive"));
    assert!(build_script.contains(".sha256"));
    assert!(build_script.contains("VsDevCmd.bat"));
    assert!(build_script.contains("LEDGRRR_VSDEVCMD"));
    assert!(build_script.contains("build.rustc-wrapper"));
    assert!(build_script.contains("support-manifest.json"));
    assert!(build_script.contains("ledgrrr_start_service"));
    assert!(build_script.contains("ledgrrr_render_diagram"));
    assert!(build_script.contains("ledgrrr_stop_service"));
    assert!(build_script.contains("-Action Repair"));
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
