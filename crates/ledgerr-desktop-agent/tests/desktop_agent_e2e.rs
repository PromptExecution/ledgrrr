use std::io::{BufRead, Write};
use std::process::{Command, Stdio};

#[test]
fn test_desktop_agent_stdio_initialize() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledgerr-desktop-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ledgerr-desktop-agent");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    let stdout = child.stdout.as_mut().expect("failed to open stdout");

    // Send initialize request
    let req = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}"#;
    writeln!(stdin, "{req}").expect("failed to write to stdin");

    // Read response
    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(stdout);
    reader.read_line(&mut buf).expect("failed to read stdout");

    let resp: serde_json::Value =
        serde_json::from_str(&buf).expect("invalid JSON response");

    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["serverInfo"]["name"], "ledgerr-desktop-agent");
    assert!(resp["result"]["serverInfo"]["version"].is_string());
    assert_eq!(resp["result"]["protocolVersion"], "2024-11-05");

    // Graceful shutdown
    let _ = stdin;
    let _ = child.wait();
}

#[test]
fn test_desktop_agent_tools_list() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledgerr-desktop-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ledgerr-desktop-agent");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    let stdout = child.stdout.as_mut().expect("failed to open stdout");

    // Initialize first
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#)
        .expect("failed to write");
    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(stdout);
    reader.read_line(&mut buf).expect("failed to read");

    // Now list tools
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{{}}}}"#)
        .expect("failed to write");
    buf.clear();
    reader.read_line(&mut buf).expect("failed to read");

    let resp: serde_json::Value =
        serde_json::from_str(&buf).expect("invalid JSON response");
    let tools = resp["result"]["tools"].as_array().expect("tools must be an array");
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .collect();

    assert!(names.contains(&"desktop_status"), "should contain desktop_status");
    assert!(names.contains(&"desktop_ping"), "should contain desktop_ping");

    let _ = stdin;
    let _ = child.wait();
}

#[test]
fn test_desktop_agent_ping() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledgerr-desktop-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ledgerr-desktop-agent");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    let stdout = child.stdout.as_mut().expect("failed to open stdout");

    // Initialize
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#)
        .expect("failed to write");
    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(stdout);
    reader.read_line(&mut buf).expect("failed to read");

    // Call desktop_ping
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"desktop_ping","arguments":{{}}}}}}"#
    )
    .expect("failed to write");
    buf.clear();
    reader.read_line(&mut buf).expect("failed to read");

    let resp: serde_json::Value =
        serde_json::from_str(&buf).expect("invalid JSON response");
    assert_eq!(resp["result"]["pong"], true);
    assert!(resp["result"]["timestamp"].is_string());

    let _ = stdin;
    let _ = child.wait();
}

#[test]
fn test_desktop_agent_status() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ledgerr-desktop-agent"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn ledgerr-desktop-agent");

    let stdin = child.stdin.as_mut().expect("failed to open stdin");
    let stdout = child.stdout.as_mut().expect("failed to open stdout");

    // Initialize
    writeln!(stdin, r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{}}}}"#)
        .expect("failed to write");
    let mut buf = String::new();
    let mut reader = std::io::BufReader::new(stdout);
    reader.read_line(&mut buf).expect("failed to read");

    // Call desktop_status
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{{"name":"desktop_status","arguments":{{}}}}}}"#
    )
    .expect("failed to write");
    buf.clear();
    reader.read_line(&mut buf).expect("failed to read");

    let resp: serde_json::Value =
        serde_json::from_str(&buf).expect("invalid JSON response");
    assert_eq!(resp["result"]["agent"], "ledgerr-desktop-agent");
    assert_eq!(resp["result"]["status"], "running");

    let _ = stdin;
    let _ = child.wait();
}

#[test]
fn test_packaging_script_exists() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .map(|root| root.join("scripts").join("package-desktop-agent.sh"))
        .expect("failed to resolve script path");

    assert!(path.exists(), "package-desktop-agent.sh must exist at {path:?}");
    assert!(path.is_file(), "package-desktop-agent.sh must be a file");

    // Verify it's executable
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path).expect("failed to read metadata");
        assert!(meta.permissions().mode() & 0o111 != 0, "script must be executable");
    }
}
