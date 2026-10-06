//! Trusted host configuration and fixture tooling; no provisioning HTTP endpoint.
use ledgrrr_revision_io::*;
use ledgrrr_sysml_adapter::{
    client::{Bounds, NativeClient},
    native,
    promotion::Owner,
    server::{self, Credential, Host},
};
use serde::Deserialize;
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use ufo_types::revision::{ArtifactPath, PortableModel, RevisionContext};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    bind: String,
    backend_origin: String,
    store_path: PathBuf,
    owner_actor: ActorId,
    credentials: Vec<Credential>,
    projects: Vec<Project>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Project {
    project: ProjectId,
    remote_project: String,
    branch: BranchId,
    remote_branch: String,
    model_dialect: String,
    actors: Vec<ProjectActor>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectActor {
    actor: ActorId,
    read: bool,
    propose: bool,
    administer: bool,
}
fn arg(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
}
fn invalid(message: &str) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::InvalidInput, message)
}
fn fixture(args: &[String]) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let project =
        ProjectId::new(arg(args, "--project").ok_or_else(|| invalid("fixture needs --project"))?)?;
    let output = arg(args, "--output").ok_or_else(|| invalid("fixture needs --output"))?;
    let case = arg(args, "--case").unwrap_or_else(|| "base".into());
    let mut bundle = if let Some(input) = arg(args, "--input") {
        PortableBundle::from_bytes(&std::fs::read(input)?)?
    } else {
        let f: serde_json::Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/native_revision_cases.json"
        ))?;
        let model: PortableModel = serde_json::from_value(f["model"].clone())?;
        let context: RevisionContext = serde_json::from_value(f["context"].clone())?;
        let artifacts: BTreeMap<ArtifactPath, Vec<u8>> =
            serde_json::from_value(f["artifacts"].clone())?;
        PortableBundle::dehydrate(model, context, artifacts)?
    };
    bundle.manifest.context.project = project;
    bundle.manifest.context.parent = arg(args, "--parent").map(RevisionId::new).transpose()?;
    bundle.manifest.context.revision = RevisionId::new(format!("candidate-{case}"))?;
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../tests/fixtures/owner_operation_cases.json"
    ))?;
    let changes = cases
        .get(&case)
        .ok_or_else(|| invalid("unknown JSON fixture case"))?;
    if changes.get("empty").and_then(serde_json::Value::as_bool) == Some(true) {
        bundle.model.elements.clear();
        bundle.model.relations.clear();
    }
    if let Some(remove) = changes.get("remove_relations").and_then(serde_json::Value::as_array) {
        for r in remove {
            if let Some(rel_id) = r.as_str() {
                bundle.model.relations.remove(rel_id);
            }
        }
    }
    if let Some(id) = changes.get("element").and_then(serde_json::Value::as_str) {
        let element = bundle
            .model
            .elements
            .get_mut(id)
            .ok_or_else(|| invalid("fixture element missing"))?;
        if let Some(name) = changes.get("name").and_then(serde_json::Value::as_str) {
            element.name = name.into();
        }
        if let Some(kind) = changes.get("kind") {
            element.kind = serde_json::from_value(kind.clone())?;
        }
    }
    bundle.manifest.semantic_digest = bundle.model.semantic_digest()?;
    std::fs::write(output, bundle.to_bytes()?)?;
    Ok(())
}
#[tokio::main]
async fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().map(String::as_str) == Some("fixture") {
        return fixture(&args);
    }
    let path = arg(&args, "--config").ok_or_else(|| invalid("--config required"))?;
    let config: Config = serde_json::from_value(native::decode_json(&std::fs::read(path)?)?)?;
    if config.credentials.is_empty() || config.credentials.iter().any(|c| c.token.len() < 32) {
        return Err(invalid("host credentials require at least 32 bytes").into());
    }
    let mut tokens = std::collections::BTreeSet::new();
    if config
        .credentials
        .iter()
        .any(|c| !tokens.insert(c.token.clone()))
    {
        return Err(invalid("duplicate host credential").into());
    }
    let client = NativeClient::new(&config.backend_origin, Bounds::default())?;
    // The task topology has one shared authority directory. Its persistent guard
    // prevents a different owner database being configured in that topology.
    let directory = config
        .store_path
        .parent()
        .ok_or_else(|| invalid("store needs parent directory"))?;
    std::fs::create_dir_all(directory)?;
    let guard = directory.join("provider-authority.json");
    let identity = ufo_types::revision::canonical_bytes(&(
        config.backend_origin.clone(),
        config.store_path.clone(),
    ))?;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&guard)
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(&identity)?;
            file.sync_all()?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if std::fs::read(&guard)? != identity {
                return Err(
                    invalid("physical topology already bound to another owner database").into(),
                );
            }
        }
        Err(e) => return Err(e.into()),
    }
    let mut store = Store::open(&config.store_path)?;
    for project in config.projects {
        if !canonical_uuid(&project.remote_project) || !canonical_uuid(&project.remote_branch) {
            return Err(invalid(
                "native physical project/branch handles must be canonical lowercase UUIDs",
            )
            .into());
        }
        let binding = ProjectBinding {
            provider: "omg-native".into(),
            remote_project: project.remote_project,
            model_dialect: project.model_dialect,
        };
        store.bootstrap_project(&config.owner_actor, &project.project, &binding)?;
        store.register_branch(
            &config.owner_actor,
            &project.project,
            &project.branch,
            &project.remote_branch,
        )?;
        for grant in project.actors {
            store.set_grant(
                &config.owner_actor,
                &project.project,
                &grant.actor,
                Grant {
                    read: grant.read,
                    propose: grant.propose,
                    administer: grant.administer,
                },
            )?;
        }
    }
    drop(store);
    let host = Arc::new(Host {
        owner: Owner {
            store_path: config.store_path,
            actor: config.owner_actor,
            client,
        },
        credentials: config.credentials,
        permits: tokio::sync::Semaphore::new(16),
    });
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    axum::serve(listener, server::router(host)).await?;
    Ok(())
}

fn canonical_uuid(value: &str) -> bool {
    value.len() == 36
        && value.bytes().enumerate().all(|(position, b)| {
            if matches!(position, 8 | 13 | 18 | 23) {
                b == b'-'
            } else {
                b.is_ascii_digit() || (b'a'..=b'f').contains(&b)
            }
        })
}
