use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};

fn parse_response_payload(response: &Value) -> Value {
    let text = response["content"][0]["text"].as_str().unwrap_or("null");
    serde_json::from_str(text).unwrap_or(Value::Null)
}

struct McpClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpClient {
    fn spawn() -> Result<Self, String> {
        let current_exe = std::env::current_exe().map_err(|err| err.to_string())?;
        let server_bin = current_exe.with_file_name("ledgerr-mcp-server");

        let mut child = Command::new(&server_bin)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| {
                format!(
                    "failed to spawn ledgerr-mcp-server at {}: {err}",
                    server_bin.display()
                )
            })?;

        let stdin = child.stdin.take().ok_or("server stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("server stdout unavailable")?;
        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            next_id: 1,
        })
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value, String> {
        let id = self.next_id;
        self.next_id += 1;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.send_and_read(payload)
    }

    fn send_notification_initialized(&mut self) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        });
        let line = serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        writeln!(self.stdin, "{line}").map_err(|err| err.to_string())?;
        self.stdin.flush().map_err(|err| err.to_string())
    }

    fn send_and_read(&mut self, payload: Value) -> Result<Value, String> {
        let line = serde_json::to_string(&payload).map_err(|err| err.to_string())?;
        writeln!(self.stdin, "{line}").map_err(|err| err.to_string())?;
        self.stdin.flush().map_err(|err| err.to_string())?;

        let mut response = String::new();
        self.stdout
            .read_line(&mut response)
            .map_err(|err| err.to_string())?;
        if response.trim().is_empty() {
            return Err("empty response line from MCP server".to_string());
        }
        serde_json::from_str::<Value>(response.trim()).map_err(|err| err.to_string())
    }
}

impl Drop for McpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct FlowContext {
    client: McpClient,
    tmp_dir: PathBuf,
    source_ref: PathBuf,
    journal_path: PathBuf,
    workbook_path: PathBuf,
}

impl FlowContext {
    fn new() -> Result<Self, String> {
        let client = McpClient::spawn()?;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|err| err.to_string())?
            .as_millis();
        let tmp_dir =
            std::env::temp_dir().join(format!("l3dg3rr-outcome-flow-{}-{ts}", std::process::id()));
        fs::create_dir_all(&tmp_dir).map_err(|err| err.to_string())?;

        let source_ref = tmp_dir.join("WF--BH-CHK--2023-01--statement.rkyv");
        let journal_path = tmp_dir.join("ledger.beancount");
        let workbook_path = tmp_dir.join("tax-ledger.xlsx");

        Ok(Self {
            client,
            tmp_dir,
            source_ref,
            journal_path,
            workbook_path,
        })
    }
}

impl Drop for FlowContext {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.tmp_dir);
    }
}

struct Step {
    name: &'static str,
    mode: &'static str,
    max_attempts: u8,
    run: fn(&mut FlowContext) -> Result<(), String>,
}

fn step_initialize(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "initialize",
        json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {},
            "clientInfo": { "name": "mcp-outcome-test", "version": "0.1.0" }
        }),
    )?;
    if response.get("result").is_none() {
        return Err(format!("initialize failed: {response}"));
    }
    ctx.client.send_notification_initialized()?;
    Ok(())
}

fn step_tools_list(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request("tools/list", json!({}))?;
    let tools = response["result"]["tools"]
        .as_array()
        .ok_or_else(|| format!("tools/list missing array: {response}"))?;
    let names = tools
        .iter()
        .filter_map(|entry| entry.get("name").and_then(Value::as_str))
        .collect::<Vec<_>>();

    for required in [
        "ledgerr_documents",
        "ledgerr_workflow",
        "ledgerr_reconciliation",
        "ledgerr_audit",
    ] {
        if !names.contains(&required) {
            return Err(format!(
                "missing tool `{required}` in tools/list: {response}"
            ));
        }
    }
    Ok(())
}

fn step_pipeline_status(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": { "action": "pipeline_status" }
        }),
    )?;
    let p = parse_response_payload(&response["result"]);
    let status = &p["status"];
    if *status != "ready" {
        return Err(format!("pipeline status not ready: {response}"));
    }
    Ok(())
}

fn step_list_accounts(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": { "action": "list_accounts" }
        }),
    )?;
    let accounts_p = parse_response_payload(&response["result"]);
    let accounts = accounts_p["accounts"]
        .as_array()
        .ok_or_else(|| format!("accounts not returned: {response}"))?;
    if accounts.is_empty() {
        return Err(format!("accounts should not be empty: {response}"));
    }
    Ok(())
}

fn step_ingest_sample(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": {
                "action": "ingest_pdf",
                "pdf_path": "WF--BH-CHK--2023-01--statement.pdf",
                "journal_path": path_json(&ctx.journal_path),
                "workbook_path": path_json(&ctx.workbook_path),
                "raw_context_bytes": [99, 116, 120],
                "extracted_rows": [{
                    "account_id": "WF-BH-CHK",
                    "date": "2023-01-15",
                    "amount": "-42.11",
                    "description": "Coffee Shop",
                    "source_ref": path_json(&ctx.source_ref)
                }]
            }
        }),
    )?;
    if response["result"]["isError"] != Value::Bool(false) {
        return Err(format!("ingest should succeed: {response}"));
    }
    if parse_response_payload(&response["result"])["inserted_count"] != json!(1) {
        return Err(format!("expected inserted_count=1: {response}"));
    }
    Ok(())
}

fn step_get_raw_context(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_documents",
            "arguments": {
                "action": "get_raw_context",
                "rkyv_ref": path_json(&ctx.source_ref)
            }
        }),
    )?;
    if response["result"]["isError"] != Value::Bool(false) {
        return Err(format!("get_raw_context should succeed: {response}"));
    }
    if parse_response_payload(&response["result"])["bytes"] != json!([99, 116, 120]) {
        return Err(format!("raw context bytes mismatch: {response}"));
    }
    Ok(())
}

fn step_hsm_resume_blocked(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_workflow",
            "arguments": {
                "action": "resume",
                "state_marker": "invalid-checkpoint"
            }
        }),
    )?;
    if response["result"]["isError"] != Value::Bool(true) {
        return Err(format!("expected blocked hsm resume: {response}"));
    }
    let ep = parse_response_payload(&response["result"]);
    let error_type = &ep["error_type"];
    if *error_type != "HsmResumeBlocked" {
        return Err(format!("expected HsmResumeBlocked error type: {response}"));
    }
    Ok(())
}

fn step_reconciliation_blocked(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_reconciliation",
            "arguments": {
                "action": "commit",
                "source_total": "100.00",
                "extracted_total": "95.00",
                "posting_amounts": ["-95.00", "95.00"]
            }
        }),
    )?;
    if response["result"]["isError"] != Value::Bool(true) {
        return Err(format!(
            "expected blocked reconciliation commit: {response}"
        ));
    }
    let ep2 = parse_response_payload(&response["result"]);
    let error_type = &ep2["error_type"];
    if *error_type != "ReconciliationBlocked" {
        return Err(format!(
            "expected ReconciliationBlocked error type: {response}"
        ));
    }
    Ok(())
}

fn step_event_history_blocked(ctx: &mut FlowContext) -> Result<(), String> {
    let response = ctx.client.request(
        "tools/call",
        json!({
            "name": "ledgerr_audit",
            "arguments": {
                "action": "event_history",
                "time_start": "2026-12-31",
                "time_end": "2026-01-01"
            }
        }),
    )?;
    if response["result"]["isError"] != Value::Bool(true) {
        return Err(format!(
            "expected blocked event_history request: {response}"
        ));
    }
    let rp = parse_response_payload(&response["result"]);
    let reason = &rp["reason"];
    if *reason != "time_range_invalid" {
        return Err(format!("expected time_range_invalid reason: {response}"));
    }
    Ok(())
}

fn path_json(path: &Path) -> String {
    path.display().to_string()
}

fn run_step(ctx: &mut FlowContext, step: &Step) -> Result<(), String> {
    let mut attempts = 0u8;
    loop {
        attempts += 1;
        match (step.run)(ctx) {
            Ok(()) => {
                println!(
                    "[PASS] [{}] {} (attempt {attempts}/{})",
                    step.mode, step.name, step.max_attempts
                );
                return Ok(());
            }
            Err(err) if attempts < step.max_attempts => {
                println!(
                    "[RETRY] [{}] {} (attempt {attempts}/{}) -> {err}",
                    step.mode, step.name, step.max_attempts
                );
                thread::sleep(Duration::from_millis(200));
            }
            Err(err) => {
                return Err(format!(
                    "[FAIL] [{}] {} after {attempts} attempt(s): {err}",
                    step.mode, step.name
                ));
            }
        }
    }
}

fn main() {
    let mut ctx = match FlowContext::new() {
        Ok(ctx) => ctx,
        Err(err) => {
            eprintln!("[FAIL] setup: {err}");
            std::process::exit(1);
        }
    };

    println!("== OUTCOME FLOW (basic) ==");
    let basic_steps = [
        Step {
            name: "initialize",
            mode: "basic",
            max_attempts: 2,
            run: step_initialize,
        },
        Step {
            name: "tools_list_contains_required",
            mode: "basic",
            max_attempts: 2,
            run: step_tools_list,
        },
        Step {
            name: "pipeline_status_ready",
            mode: "basic",
            max_attempts: 2,
            run: step_pipeline_status,
        },
        Step {
            name: "list_accounts_nonempty",
            mode: "basic",
            max_attempts: 2,
            run: step_list_accounts,
        },
        Step {
            name: "ingest_sample_statement",
            mode: "basic",
            max_attempts: 2,
            run: step_ingest_sample,
        },
        Step {
            name: "read_raw_context_bytes",
            mode: "basic",
            max_attempts: 2,
            run: step_get_raw_context,
        },
    ];

    for step in &basic_steps {
        if let Err(err) = run_step(&mut ctx, step) {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }

    println!("== OUTCOME FLOW (spinning-wheels diagnostics) ==");
    let diagnostic_steps = [
        Step {
            name: "hsm_resume_blocked_checkpoint",
            mode: "spinning-wheels",
            max_attempts: 2,
            run: step_hsm_resume_blocked,
        },
        Step {
            name: "commit_guarded_blocked_totals_mismatch",
            mode: "spinning-wheels",
            max_attempts: 2,
            run: step_reconciliation_blocked,
        },
        Step {
            name: "event_history_blocked_invalid_time_range",
            mode: "spinning-wheels",
            max_attempts: 2,
            run: step_event_history_blocked,
        },
    ];

    for step in &diagnostic_steps {
        if let Err(err) = run_step(&mut ctx, step) {
            eprintln!("{err}");
            std::process::exit(1);
        }
    }

    println!("== OUTCOME FLOW COMPLETE ==");
}
