//! Authenticated owner routes. Host provisioning and native mutations are absent.
use crate::{
    native,
    promotion::{self, Owner},
};
use axum::{
    body::Bytes,
    extract::{DefaultBodyLimit, Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use ledgrrr_revision_io::*;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credential {
    pub token: String,
    pub actor: ActorId,
}
pub struct Host {
    pub owner: Owner,
    pub credentials: Vec<Credential>,
    pub permits: tokio::sync::Semaphore,
    pub query_engine: crate::query::QueryEngine,
}
fn principal(host: &Host, headers: &HeaderMap) -> std::result::Result<ActorId, ApiError> {
    let token = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)?;
    let digest = blake3::hash(token.as_bytes());
    for credential in &host.credentials {
        let configured = blake3::hash(credential.token.as_bytes());
        let different = digest
            .as_bytes()
            .iter()
            .zip(configured.as_bytes())
            .fold(0u8, |v, (a, b)| v | (a ^ b));
        if different == 0 {
            return Ok(credential.actor.clone());
        }
    }
    Err(ApiError::Unauthorized)
}
#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    Busy,
    Invalid,
    Owner(promotion::Error),
}
impl From<promotion::Error> for ApiError {
    fn from(e: promotion::Error) -> Self {
        Self::Owner(e)
    }
}
impl From<ledgrrr_revision_io::Error> for ApiError {
    fn from(e: ledgrrr_revision_io::Error) -> Self {
        Self::Owner(e.into())
    }
}
impl From<ufo_types::revision::RevisionError> for ApiError {
    fn from(_: ufo_types::revision::RevisionError) -> Self {
        Self::Invalid
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, message) = match self {
            Self::Unauthorized => (StatusCode::UNAUTHORIZED, "authentication required"),
            Self::Busy => (StatusCode::TOO_MANY_REQUESTS, "owner busy"),
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid bounded request"),
            Self::Owner(promotion::Error::Store(Error::Denied)) => {
                (StatusCode::FORBIDDEN, "project permission denied")
            }
            Self::Owner(promotion::Error::Store(Error::NotFound)) => {
                (StatusCode::NOT_FOUND, "record not found")
            }
            Self::Owner(promotion::Error::Store(Error::BranchBlocked)) => {
                (StatusCode::CONFLICT, "branch outcome unresolved")
            }
            Self::Owner(promotion::Error::Native(native::Error::Fidelity(report))) => {
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(serde_json::json!({"kind":"fidelity","report":report})),
                )
                    .into_response()
            }
            Self::Owner(_) => (
                StatusCode::SERVICE_UNAVAILABLE,
                "owner outcome unavailable; inspect receipt",
            ),
        };
        (code, Json(serde_json::json!({"error":message}))).into_response()
    }
}
type Result<T> = std::result::Result<T, ApiError>;
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Proposal {
    expected_head: ExpectedHead,
    bundle: Box<serde_json::value::RawValue>,
}
async fn intake(
    State(host): State<Arc<Host>>,
    Path((p, b, o)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<StoredOperation>> {
    let actor = principal(&host, &headers)?;
    native::decode_json(&body).map_err(|_| ApiError::Invalid)?; // Reject duplicate keys before typed maps.
    let proposal: Proposal = serde_json::from_slice(&body).map_err(|_| ApiError::Invalid)?;
    PortableBundle::from_bytes(proposal.bundle.get().as_bytes()).map_err(|_| ApiError::Invalid)?;
    let project = ProjectId::new(p)?;
    let mut store = host.owner.store()?;
    let binding = store.project_binding(&actor, &project)?;
    let request = Intake {
        project,
        branch: BranchId::new(b)?,
        operation: OperationId::new(o)?,
        expected_head: proposal.expected_head,
        binding,
    };
    Ok(Json(store.intake(
        &actor,
        &request,
        proposal.bundle.get().as_bytes(),
    )?))
}
async fn promote(
    State(host): State<Arc<Host>>,
    Path((p, b, o)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<StoredOperation>> {
    if !body.is_empty() {
        return Err(ApiError::Invalid);
    }
    let actor = principal(&host, &headers)?;
    let _permit = host.permits.try_acquire().map_err(|_| ApiError::Busy)?;
    let project = ProjectId::new(p)?;
    let operation = OperationId::new(o)?;
    if host
        .owner
        .store()?
        .operation(&actor, &project, &operation)?
        .receipt
        .branch
        .as_str()
        != b
    {
        return Err(ApiError::Invalid);
    }
    Ok(Json(
        host.owner.promote(&actor, &project, &operation).await?,
    ))
}
async fn receipt(
    State(host): State<Arc<Host>>,
    Path((p, o)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<StoredOperation>> {
    let actor = principal(&host, &headers)?;
    Ok(Json(host.owner.store()?.operation(
        &actor,
        &ProjectId::new(p)?,
        &OperationId::new(o)?,
    )?))
}
async fn reconcile(
    State(host): State<Arc<Host>>,
    Path((p, o)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<StoredOperation>> {
    if !body.is_empty() {
        return Err(ApiError::Invalid);
    }
    let actor = principal(&host, &headers)?;
    let _permit = host.permits.try_acquire().map_err(|_| ApiError::Busy)?;
    Ok(Json(
        host.owner
            .reconcile(&actor, &ProjectId::new(p)?, &OperationId::new(o)?)
            .await?,
    ))
}
async fn bundle(
    State(host): State<Arc<Host>>,
    Path((p, r)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<PortableBundle>> {
    let actor = principal(&host, &headers)?;
    let _permit = host.permits.try_acquire().map_err(|_| ApiError::Busy)?;
    Ok(Json(
        host.owner.export(&actor, &ProjectId::new(p)?, &r).await?,
    ))
}
async fn artifact(
    State(host): State<Arc<Host>>,
    Path((p, d)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response> {
    let actor = principal(&host, &headers)?;
    let bytes =
        host.owner
            .store()?
            .artifact(&actor, &ProjectId::new(p)?, &ArtifactDigest::try_from(d)?)?;
    Ok(([("content-type", "application/octet-stream")], bytes).into_response())
}
async fn recovery(
    State(host): State<Arc<Host>>,
    Path(p): Path<String>,
    headers: HeaderMap,
) -> Result<Json<Vec<StoredOperation>>> {
    let actor = principal(&host, &headers)?;
    Ok(Json(host.owner.store()?.recovery(
        &actor,
        &ProjectId::new(p)?,
        None,
        256,
    )?))
}
async fn head(
    State(host): State<Arc<Host>>,
    Path((p, b)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>> {
    let actor = principal(&host, &headers)?;
    let p = ProjectId::new(p)?;
    let b = BranchId::new(b)?;
    let mut store = host.owner.store()?;
    let binding = store.project_binding(&actor, &p)?;
    let remote = store.remote_branch(&actor, &p, &b)?;
    let _permit = host.permits.try_acquire().map_err(|_| ApiError::Busy)?;
    let revision = host
        .owner
        .client
        .head(&binding.remote_project, &remote)
        .await
        .map_err(promotion::Error::from)?;
    host.owner.observe_index_head(&p, &b).await?;
    let state =
        host.owner
            .store()?
            .index_state(&actor, &p, &b, crate::projection::PROJECTION_SCHEMA)?;
    let indexed_revision = state
        .available_graph
        .as_ref()
        .map(|g| &g.checkpoint.revision);
    let freshness = if state.unavailable_reason.is_some() {
        "unavailable"
    } else if revision.as_deref() == indexed_revision.map(RevisionId::as_str) {
        "fresh"
    } else {
        "pending"
    };
    Ok(Json(
        serde_json::json!({"model_revision":revision,"indexed_revision":indexed_revision,"freshness":freshness,"graph":state.available_graph}),
    ))
}
pub fn router(host: Arc<Host>) -> Router {
    Router::new()
        .route("/v1/projects/:p/branches/:b/operations/:o", post(intake))
        .route(
            "/v1/projects/:p/branches/:b/operations/:o/promote",
            post(promote),
        )
        .route("/v1/projects/:p/operations/:o", get(receipt))
        .route("/v1/projects/:p/operations/:o/reconcile", post(reconcile))
        .route("/v1/projects/:p/revisions/:r/bundle", get(bundle))
        .route("/v1/projects/:p/artifacts/:d", get(artifact))
        .route("/v1/projects/:p/recovery", get(recovery))
        .route("/v1/projects/:p/branches/:b/head", get(head))
        .route("/v1/projects/:p/branches/:b/query", post(query))
        .route("/v1/projects/:p/revisions/:r/checkpoint", get(checkpoint))
        .layer(DefaultBodyLimit::max(32 * 1024 * 1024))
        .layer(axum::middleware::from_fn(deadline))
        .with_state(host)
}

async fn query(
    State(host): State<Arc<Host>>,
    Path((p, b)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Json<ufo_types::revision::RevisionQueryResponse>> {
    let actor = principal(&host, &headers)?;
    if body.len() > 128 * 1024 {
        return Err(ApiError::Invalid);
    }
    let request: ufo_types::revision::RevisionQueryRequest =
        serde_json::from_slice(&body).map_err(|_| ApiError::Invalid)?;
    if request.project.as_str() != p || request.branch.as_str() != b || request.deadline_ms > 5000 {
        return Err(ApiError::Invalid);
    }
    let _permit = host.permits.try_acquire().map_err(|_| ApiError::Busy)?;
    Ok(Json(
        host.owner
            .revision_query(&actor, &request, &host.query_engine)
            .await?,
    ))
}
async fn checkpoint(
    State(host): State<Arc<Host>>,
    Path((p, r)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Json<ufo_types::revision::RevisionGraphDescriptor>> {
    let actor = principal(&host, &headers)?;
    let p = ProjectId::new(p)?;
    let r = RevisionId::new(r)?;
    let graph = host
        .owner
        .store()?
        .graph(&actor, &p, &r, crate::projection::PROJECTION_SCHEMA)?
        .ok_or(ledgrrr_revision_io::Error::NotFound)?;
    Ok(Json(graph.0))
}

async fn deadline(request: axum::extract::Request, next: axum::middleware::Next) -> Response {
    match tokio::time::timeout(std::time::Duration::from_secs(60),next.run(request)).await {
        Ok(response)=>response,
        Err(_)=>(StatusCode::GATEWAY_TIMEOUT,Json(serde_json::json!({"error":"bounded owner deadline; dispatched intent remains fenced, inspect/reconcile receipt"}))).into_response(),
    }
}
