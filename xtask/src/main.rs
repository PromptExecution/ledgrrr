use std::fs;
use std::path::PathBuf;

mod kerm;
mod viz_manifest;

use clap::{Parser, Subcommand};

use xtask_mcpb::{
    bundler::McpbBundler,
    manifest::{ManifestAuthor, ManifestServer, McpConfig, McpbManifest, ServerType},
    publisher::{GitHubPublisher, McpRegistryPublisher},
    server_json::ServerJson,
    verify::verify_bundle,
};

#[derive(Parser)]
#[command(name = "xtask", about = "l3dg3rr build and publish automation")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Compile binary + assemble a deterministic .mcpb bundle
    Bundle {
        /// Path to the compiled ledgerr-mcp-server binary
        #[arg(long, default_value = "target/release/ledgerr-mcp-server")]
        binary: PathBuf,
        /// Output .mcpb path
        #[arg(long, default_value = "dist/ledgerr-mcp.mcpb")]
        output: PathBuf,
        /// Version string to embed in manifest (e.g. v0.1.0)
        #[arg(long)]
        version: String,
    },
    /// Print the manifest.json for a given version (no bundle created)
    Manifest {
        #[arg(long)]
        version: String,
    },
    /// Upload a .mcpb artifact to a GitHub release (requires gh CLI + GITHUB_TOKEN)
    PublishGithub {
        #[arg(long)]
        release_tag: String,
        #[arg(long)]
        artifact: PathBuf,
        /// Override repository (e.g. PromptExecution/l3dg3rr)
        #[arg(long)]
        repo: Option<String>,
    },
    /// Submit bundle to MCP Registry (requires mcp-publisher on PATH + auth)
    PublishRegistry {
        #[arg(long)]
        release_tag: String,
        /// Public download URL of the .mcpb artifact
        #[arg(long)]
        artifact_url: String,
        /// Hex SHA-256 of the .mcpb file (from `xtask bundle` output)
        #[arg(long)]
        sha256: String,
        #[arg(long, default_value = "io.github.prompt-execution/ledgerr-mcp")]
        server_name: String,
    },
    /// Validate a .mcpb bundle: ZIP structure, manifest, and entry_point presence
    Verify { path: PathBuf },
    /// Update server.json version, mcpb identifier URL, and fileSha256 for a release.
    /// Run this before `mcp-publisher publish`.
    UpdateServerJson {
        /// Release version tag (e.g. v0.1.0)
        #[arg(long)]
        version: String,
        /// Public download URL of the canonical .mcpb artifact
        #[arg(long)]
        artifact_url: String,
        /// Hex SHA-256 of the .mcpb file (printed by `xtask-mcpb bundle`)
        #[arg(long)]
        sha256: String,
        /// Path to server.json (default: ./server.json)
        #[arg(long, default_value = "server.json")]
        path: PathBuf,
    },
    /// Regenerate MCP contract docs and runnable examples from Rust.
    GenerateMcpArtifacts,
    /// Generate type compatibility tables for mdbook documentation.
    GenerateTypeTables {
        /// Output directory for generated markdown tables.
        #[arg(long, default_value = "book/src/")]
        output: PathBuf,
    },
    /// Export VisualizationSpec JSON manifest for the docs UI
    ExportVizManifest {
        /// Output path (default: ui/docs/public/viz-manifest.json)
        #[arg(long, default_value = "ui/docs/public/viz-manifest.json")]
        output: PathBuf,
    },
    /// Regenerate holon-viz seed from types/domain.kerm
    GenerateKermArtifacts {
        /// Path to domain.kerm (default: types/domain.kerm)
        #[arg(long, default_value = "types/domain.kerm")]
        kerm: PathBuf,
        /// Output path for generated Rust (default: crates/holon-viz/src/gen.rs)
        #[arg(long, default_value = "crates/holon-viz/src/gen.rs")]
        output: PathBuf,
    },
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run(cli: Cli) -> Result<(), Box<dyn std::error::Error>> {
    match cli.command {
        Commands::Bundle {
            binary,
            output,
            version,
        } => {
            let binary_name = binary
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("ledgerr-mcp-server")
                .to_string();
            let manifest = ledgerr_manifest(&version, &binary_name);
            let bundler = McpbBundler::new(manifest, binary, output);
            let artifact = bundler.bundle()?;
            println!("bundled: {}", artifact.path.display());
            println!("sha256:  {}", artifact.sha256);
            println!("size:    {} bytes", artifact.size_bytes);
        }

        Commands::Manifest { version } => {
            let manifest = ledgerr_manifest(&version, "ledgerr-mcp-server");
            println!("{}", serde_json::to_string_pretty(&manifest)?);
        }

        Commands::PublishGithub {
            release_tag,
            artifact,
            repo,
        } => {
            let mut publisher = GitHubPublisher::new(&release_tag);
            if let Some(r) = repo {
                publisher = publisher.with_repo(r);
            }
            publisher.upload(&artifact)?;
            println!("uploaded {} → release {}", artifact.display(), release_tag);
        }

        Commands::PublishRegistry {
            release_tag,
            artifact_url,
            sha256,
            server_name,
        } => {
            let manifest = ledgerr_manifest(&release_tag, "ledgerr-mcp-server");
            let publisher =
                McpRegistryPublisher::new(&release_tag, &server_name, &manifest.description);
            publisher.publish(&artifact_url, &sha256)?;
            println!("published {server_name} to MCP Registry @ {release_tag}");
        }

        Commands::Verify { path } => {
            let manifest = verify_bundle(&path)?;
            println!(
                "ok: {} ({} {})",
                path.display(),
                manifest.name,
                manifest.version
            );
        }

        Commands::UpdateServerJson {
            version,
            artifact_url,
            sha256,
            path,
        } => {
            let mut server_json = ServerJson::load(&path)?;
            server_json.update_mcpb(&version, &artifact_url, &sha256)?;
            server_json.save(&path)?;
            println!(
                "updated {}: version={version} sha256={sha256}",
                path.display()
            );
        }
        Commands::GenerateMcpArtifacts => {
            let capability_doc = ledgerr_mcp::contract::generated_capability_contract_markdown();
            let runbook_doc = ledgerr_mcp::contract::generated_agent_runbook_markdown();
            let demo_script = ledgerr_mcp::contract::generated_mcp_cli_demo_script();

            fs::write("docs/mcp-capability-contract.md", capability_doc)?;
            fs::write("docs/agent-mcp-runbook.md", runbook_doc)?;
            fs::write("scripts/mcp_cli_demo.sh", demo_script)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let perms = fs::Permissions::from_mode(0o755);
                fs::set_permissions("scripts/mcp_cli_demo.sh", perms)?;
            }

            println!("generated MCP contract artifacts");
        }
        Commands::GenerateTypeTables { output } => {
            generate_type_tables(&output)?;
        }
        Commands::ExportVizManifest { output } => {
            viz_manifest::export_viz_manifest(&output)?;
        }
        Commands::GenerateKermArtifacts { kerm, output } => {
            let domain = kerm::load(&kerm)?;
            let code = kerm::codegen(&domain);
            fs::write(&output, &code)?;
            println!(
                "generated: {} ({} types, {} rels)",
                output.display(),
                domain.types.len(),
                domain.rel.len(),
            );
        }
    }
    Ok(())
}

/// Generate type compatibility tables for mdbook documentation.
/// This produces markdown files that document the full mesh of pipeline
/// stage input/output type compatibility.
fn generate_type_tables(output: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(output)?;

    // Pipeline stage I/O compatibility table
    let pipeline_table = r#"# Pipeline Stage Type Compatibility

> Auto-generated by `cargo run -p xtask-mcpb -- generate-type-tables`.
> Do not edit manually. Source of truth: `crates/ledger-core/tests/type_mesh.rs`

## Stage I/O Matrix

| Stage | Input Type | Output Type | Confidence | Jurisdiction | Notes |
|-------|-----------|-------------|------------|--------------|-------|
| Ingest | `TransactionInput` | `IngestedTransaction`, `JournalTransaction` | Deterministic (1.0) | All | Blake3 content-hash IDs |
| Validate | `TransactionInput` | `MetaCtx` | 0.0–1.0 | All | Type checks, constraint evaluation |
| Classify | `SampleTransaction` | `ClassificationOutcome`, `ClassifiedTransaction` | 0.0–1.0 | US/AU/UK | Rhai rule waterfall |
| Reconcile | `ClassifiedTransaction` | `OperationResult` | 0.0–1.0 | All | Xero match/diff |
| Export | `OperationContext` | `rust_xlsxwriter::Workbook` | Deterministic (1.0) | All | CPA-auditable Excel |
| Verify | `RepairProposal` | `VerificationOutcome` | 0.0–1.0 | All | Multi-model proposer/reviewer |

## Cross-Stage Compatibility

| From Type | To Type | Compatible? | Bridge | Test |
|-----------|---------|-------------|--------|------|
| `TransactionInput` | `SampleTransaction` | Yes | `deterministic_tx_id()` | `test_transaction_input_to_sample_transaction_shape` |
| `TransactionInput` | `JournalTransaction` | Yes | `JournalTransaction::from_input()` | `test_transaction_input_to_journal_shape` |
| `ClassificationOutcome` | `ClassifiedTransaction` | Yes | Field mapping + `tx_id` | `test_classification_outcome_to_classified_shape` |
| `ClassifiedTransaction` | `TxProjectionRow` | Partial | Requires upstream context | `test_classified_to_projection_row_requires_context` |
| `Issue` | `MetaCtx` | Yes | `MetaCtx::advance()` | `test_validation_pipeline_mesh` |
| `MetaCtx` | `StageResult<T>` | Yes | `and_then()` combinator | `test_validation_pipeline_mesh` |
| `LegalRule` + `TransactionFacts` | `Z3Result` | Yes | `LegalSolver::verify()` | — |
| `VendorConstraintSet` | `ConstraintEvaluation` | Yes | `VendorConstraintSet::evaluate()` | — |

## Known Type Gaps

### `ClassifiedTransaction` → `TxProjectionRow`

`ClassifiedTransaction` lacks `account_id`, `date`, `amount`, `description`, and `source_ref` fields that `TxProjectionRow` requires. The `ExportWorkbookOp` must reconstruct these from the `OperationContext`'s upstream ingest data.

**Status**: Documented in `test_classified_to_projection_row_requires_context`. Future work should either:
1. Add missing fields to `ClassifiedTransaction`, or
2. Create an explicit `ExportContext` type that carries full row data.

### `StageResult<T>` constructors use `MetaCtx::default()`

`StageResult::ok()` and `StageResult::with_issues()` initialize `meta` with `MetaCtx::default()` (confidence 0.0). The correct way to chain stages is via the `and_then()` combinator, which properly advances the `MetaCtx` with multiplicative confidence.

**Status**: Documented in `test_validation_pipeline_mesh`.

## Type Invariants

All pipeline I/O types must implement `Send + Sync` for async dispatch:

- `TransactionInput` ✓
- `IngestedTransaction` ✓
- `SampleTransaction` ✓
- `ClassificationOutcome` ✓
- `ClassifiedTransaction` ✓
- `JournalTransaction` ✓
- `TxProjectionRow` ✓
- `Issue` ✓
- `MetaCtx` ✓
"#;

    let output_path = output.join("type-compatibility.md");
    fs::write(&output_path, pipeline_table)?;
    println!("generated: {}", output_path.display());

    // Concept affinity table
    let concept_table = r#"# Concept Affinity Table

> Auto-generated by `cargo run -p xtask-mcpb -- generate-type-tables`.
> Maps domain concepts to their implementing types and modules.

## Domain Concepts

| Concept | Primary Type | Module | Related Types | Derives |
|---------|-------------|--------|---------------|---------|
| Transaction | `TransactionInput` | `ingest` | `IngestedTransaction`, `JournalTransaction`, `SampleTransaction` | `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` |
| Classification | `ClassificationOutcome` | `classify` | `ClassifiedTransaction`, `ClassificationBatch`, `ReviewFlag` | `Debug, Clone, PartialEq` |
| Validation | `Issue` | `validation` | `Disposition`, `IssueSource`, `MetaCtx`, `StageResult<T>` | `Debug, Clone, PartialEq, Serialize, Deserialize` |
| Pipeline State | `PipelineState<S>` | `pipeline` | Type-state markers: `Ingested`, `Validated`, `Classified`, `Reconciled`, `Committed`, `NeedsReview` | `Debug, Clone, Serialize, Deserialize` |
| Legal Rule | `LegalRule` | `legal` | `TransactionFacts`, `Z3Result`, `LegalSolver`, `Jurisdiction` | `Debug, Clone, Serialize, Deserialize` |
| Constraint | `VendorConstraintSet` | `constraints` | `ConstraintEvaluation`, `ConstraintStrength`, `InvoiceConstraintSolver` | `Debug, Clone, Serialize, Deserialize` |
| Verification | `MultiModelVerifier<C>` | `verify` | `RepairProposal`, `ReviewResult`, `VerificationOutcome`, `ModelClient` | `Debug, Clone, Serialize, Deserialize` |
| Workflow | `WorkflowToml` | `workflow` | `StateDecl`, `TransitionDecl` | `Debug, Clone, Deserialize, Serialize` |
| Calendar | `BusinessCalendar` | `calendar` | `ScheduledEvent`, `RecurrenceRule`, `CalendarError` | — |
| Operation | `LedgerOperation` (trait) | `ledger_ops` | `OperationContext`, `OperationResult`, `OperationKind`, `OperationDispatcher` | — |
| Document | `DocumentRecord` | `document` | `DocType`, `DocumentStatus`, `XeroLink`, `XeroEntityType` | `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` |
| Shape | `DocumentShape` | `document_shape` | `StatementVendor`, `ColumnMap` | `Debug, Clone, Serialize, Deserialize` |
| Rule Registry | `RuleRegistry` | `rule_registry` | `ReqIfCandidate`, `DocumentChunk`, `SemanticRuleSelector` | — |
| Workbook | `TxProjectionRow` | `workbook` | `REQUIRED_SHEETS` constant | `Debug, Clone, PartialEq, Eq, Serialize, Deserialize` |

## Module Dependency Graph

```
ingest ──┬──> journal
         ├──> classify (via SampleTransaction)
         └──> workbook (via TxProjectionRow)

classify ──> ledger_ops (via ClassifiedTransaction → OperationContext)
           ──> rule_registry (via ClassificationEngine)

validation ──> pipeline (via Issue, MetaCtx → PipelineState<S>)

legal ──┬──> pipeline (via Jurisdiction)
        └──> calendar (via Jurisdiction)

constraints ──> validation (via ConstraintEvaluation → Disposition)
              ──> pipeline (via ConstraintStrength)

verify ──> classify (via ModelClient trait)

workflow ──> pipeline (via state machine definition)

document ──> ingest (via DocType detection)
           ──> document_shape (via vendor classification)
```
"#;

    let concept_path = output.join("concept-affinity.md");
    fs::write(&concept_path, concept_table)?;
    println!("generated: {}", concept_path.display());

    Ok(())
}

/// Canonical manifest definition for ledgerr-mcp.
fn ledgerr_manifest(version: &str, binary_name: &str) -> McpbManifest {
    McpbManifest {
        manifest_version: "0.3".into(),
        name: "ledgerr-mcp".into(),
        version: version.into(),
        description: "Local-first U.S. expat tax document intelligence MCP server. \
            Ingests PDF statements, classifies transactions, and produces \
            CPA-auditable Excel workbooks — no data leaves your machine."
            .into(),
        author: ManifestAuthor {
            name: "Prompt Execution Pty Ltd.".into(),
            email: None,
            url: Some("https://github.com/PromptExecution/l3dg3rr".into()),
        },
        server: ManifestServer {
            server_type: ServerType::Binary,
            entry_point: format!("server/{binary_name}"),
            mcp_config: McpConfig {
                command: format!("${{__dirname}}/server/{binary_name}"),
                args: vec![],
                env: None,
            },
        },
        configuration: None,
    }
}
