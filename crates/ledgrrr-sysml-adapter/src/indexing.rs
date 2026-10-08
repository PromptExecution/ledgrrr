//! Accepted-revision indexing and authorized immutable revision reads.
use crate::{
    projection::{self, PROJECTION_SCHEMA},
    promotion::{Error, Owner, Result},
    query::QueryEngine,
};
use ledgrrr_revision_io::*;
use std::time::{Duration, Instant};
use ufo_types::revision::{
    QueryFreshness, QueryUnavailableReason, RevisionGraphDescriptor, RevisionQueryOutcome,
    RevisionQueryRequest, RevisionQueryResponse, RevisionQuerySelector,
};

impl Owner {
    pub fn index_once(
        &self,
        project: &ProjectId,
        revision: Option<&RevisionId>,
    ) -> Result<Option<RevisionGraphDescriptor>> {
        let mut store = self.store()?;
        let Some(work) = store.claim_index(
            &self.actor,
            project,
            revision,
            PROJECTION_SCHEMA,
            "owner-indexer",
            60_000,
        )?
        else {
            return Ok(None);
        };
        let graph = projection::project_bundle(&work.candidate, &work.token.revision)
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        projection::validate_graph(&graph.descriptor, &graph.nquads)
            .map_err(|e| Error::Unavailable(e.to_string()))?;
        store.seal_index(&self.actor, &work.token, &graph.descriptor, &graph.nquads)?;
        Ok(Some(store.publish_index(&self.actor, &work.token)?))
    }
    /// Poll native authority, independently validating any recognized accepted envelope.
    pub async fn observe_index_head(&self, project: &ProjectId, branch: &BranchId) -> Result<()> {
        self.observe_index_head_bounded(project, branch, Duration::from_secs(3))
            .await
    }
    async fn observe_index_head_bounded(
        &self,
        project: &ProjectId,
        branch: &BranchId,
        budget: Duration,
    ) -> Result<()> {
        let started = Instant::now();
        let mut store = Store::open_bounded(&self.store_path, budget)?;
        let binding = store.project_binding(&self.actor, project)?;
        let remote = store.remote_branch(&self.actor, project, branch)?;
        let head = self.client.head(&binding.remote_project, &remote).await?;
        store.bound_work_for(budget.saturating_sub(started.elapsed()))?;
        let revision = head.as_ref().map(RevisionId::new).transpose()?;
        let reason = if let Some(r) = &revision {
            match store.accepted_bundle(&self.actor, project, r) {
                Ok(accepted) => {
                    let elements = self
                        .client
                        .snapshot(&binding.remote_project, r.as_str(), false)
                        .await?;
                    store.bound_work_for(budget.saturating_sub(started.elapsed()))?;
                    let verified = crate::native::marker_from_elements(&elements)
                        .ok()
                        .filter(|reference| {
                            reference.marker.project == project.as_str()
                                && reference.marker.branch == branch.as_str()
                        })
                        .and_then(|reference| {
                            let operation = OperationId::new(&reference.marker.operation).ok()?;
                            let observed =
                                store.operation(&self.actor, project, &operation).ok()?;
                            if observed.actual_revision.as_ref() != Some(r) {
                                return None;
                            }
                            crate::native::hydrate(
                                &elements,
                                &accepted.to_bytes().ok()?,
                                &reference.marker,
                            )
                            .ok()
                        });
                    match verified {
                        Some(bundle) if bundle.bundle_digest()? == accepted.bundle_digest()? => {
                            None
                        }
                        _ => Some(
                            "native head cannot be independently validated against owner envelope",
                        ),
                    }
                }
                Err(_) => Some("external or legacy revision lacks retained accepted identity"),
            }
        } else {
            None
        };
        store.observe_head(&self.actor, project, branch, revision.as_ref(), reason)?;
        Ok(())
    }
    pub async fn revision_query(
        &self,
        actor: &ActorId,
        request: &RevisionQueryRequest,
        engine: &QueryEngine,
    ) -> Result<RevisionQueryResponse> {
        request.validate()?;
        if request.deadline_ms > 5000 {
            return Err(Error::Unavailable(
                "query deadline exceeds service cap".into(),
            ));
        }
        let started = Instant::now();
        let budget = Duration::from_millis(request.deadline_ms);
        let result = self
            .revision_query_inner(actor, request, engine, started, budget)
            .await;
        match result {
            Err(_) if started.elapsed() >= budget => Ok(unavailable(
                request,
                None,
                QueryUnavailableReason::DeadlineExceeded,
            )),
            other => other,
        }
    }
    fn query_store(&self, started: Instant, budget: Duration) -> Result<Store> {
        Ok(Store::open_bounded_read(
            &self.store_path,
            budget.saturating_sub(started.elapsed()),
        )?)
    }
    async fn revision_query_inner(
        &self,
        actor: &ActorId,
        request: &RevisionQueryRequest,
        engine: &QueryEngine,
        started: Instant,
        budget: Duration,
    ) -> Result<RevisionQueryResponse> {
        // Recheck requested branch/project authorization before contacting provider.
        self.query_store(started, budget)?.remote_branch(
            actor,
            &request.project,
            &request.branch,
        )?;
        // Freshness binds the independently observed model head for non-exact queries.
        // Exact queries resolve a specific historical revision and skip the head
        // observation step to avoid consuming the deadline on an unrelated network call.
        if !matches!(request.selector, RevisionQuerySelector::Exact { .. }) {
            let observed = tokio::time::timeout(
                budget.saturating_sub(started.elapsed()),
                self.observe_index_head_bounded(
                    &request.project,
                    &request.branch,
                    budget.saturating_sub(started.elapsed()),
                ),
            )
            .await;
            if !matches!(observed, Ok(Ok(()))) {
                return Ok(unavailable(
                    request,
                    None,
                    if started.elapsed() >= budget {
                        QueryUnavailableReason::DeadlineExceeded
                    } else {
                        QueryUnavailableReason::ExternalRevision
                    },
                ));
            }
        }
        loop {
            // Deadline check before attempting store open so the loop always
            // exits with Pending rather than an open_bounded Invalid error
            // when the budget drains between the sleep and the next iteration.
            if started.elapsed() >= budget {
                let response = RevisionQueryResponse {
                    project: request.project.clone(),
                    branch: request.branch.clone(),
                    outcome: RevisionQueryOutcome::Pending {
                        model_revision: None,
                        requested_revision: match &request.selector {
                            RevisionQuerySelector::Exact { revision } => Some(revision.clone()),
                            RevisionQuerySelector::Minimum { checkpoint, .. } => {
                                Some(checkpoint.revision.clone())
                            }
                            RevisionQuerySelector::Current { .. } => None,
                        },
                        available_graph: None,
                    },
                };
                response.validate_for(request)?;
                return Ok(response);
            }
            // query_store can fail with Invalid("expired store deadline") when the
            // remaining budget is so small that open_bounded_read's progress handler
            // fires before the connection setup completes. Treat that as a regular
            // deadline expiry — return Pending rather than propagating an error.
            let mut store = match self.query_store(started, budget) {
                Ok(s) => s,
                Err(_) if started.elapsed() >= budget => {
                    let response = RevisionQueryResponse {
                        project: request.project.clone(),
                        branch: request.branch.clone(),
                        outcome: RevisionQueryOutcome::Pending {
                            model_revision: None,
                            requested_revision: match &request.selector {
                                RevisionQuerySelector::Exact { revision } => {
                                    Some(revision.clone())
                                }
                                RevisionQuerySelector::Minimum { checkpoint, .. } => {
                                    Some(checkpoint.revision.clone())
                                }
                                RevisionQuerySelector::Current { .. } => None,
                            },
                            available_graph: None,
                        },
                    };
                    response.validate_for(request)?;
                    return Ok(response);
                }
                Err(e) => return Err(e),
            };
            let state_result = if matches!(request.selector, RevisionQuerySelector::Exact { .. }) {
                store.index_state_identity(
                    actor,
                    &request.project,
                    &request.branch,
                    PROJECTION_SCHEMA,
                )
            } else {
                store.index_state(actor, &request.project, &request.branch, PROJECTION_SCHEMA)
            };
            let state = match state_result {
                Ok(s) => s,
                Err(ledgrrr_revision_io::Error::Corrupt(reason)) => {
                    return Ok(unavailable(
                        request,
                        None,
                        if reason == "published graph artifact missing" {
                            QueryUnavailableReason::MissingArtifact
                        } else {
                            QueryUnavailableReason::CorruptArtifact
                        },
                    ))
                }
                Err(e) => return Err(e.into()),
            };
            let requested = match &request.selector {
                RevisionQuerySelector::Exact { revision } => Some(revision.clone()),
                RevisionQuerySelector::Minimum { checkpoint, .. } => {
                    Some(checkpoint.revision.clone())
                }
                RevisionQuerySelector::Current { .. } => None,
            };
            let mut selected = None;
            match &request.selector {
                RevisionQuerySelector::Exact { revision } => {
                    if store
                        .accepted_bundle(actor, &request.project, revision)
                        .is_err()
                    {
                        return Ok(unavailable(
                            request,
                            state.model_revision,
                            QueryUnavailableReason::MissingRevision,
                        ));
                    }
                    selected = graph_read(&mut store, actor, &request.project, revision)?;
                    // Distinguish "accepted but not yet indexed" (truly Pending) from
                    // "previously published but artifact removed" (MissingArtifact).
                    // state='queued' with descriptor set means remove_projection was called.
                    if selected.is_none()
                        && store.is_projection_removed(
                            actor,
                            &request.project,
                            revision,
                            PROJECTION_SCHEMA,
                        )?
                    {
                        return Ok(unavailable(
                            request,
                            state.model_revision,
                            QueryUnavailableReason::MissingArtifact,
                        ));
                    }
                }
                RevisionQuerySelector::Minimum {
                    checkpoint,
                    allow_older,
                } => {
                    if store
                        .accepted_bundle(actor, &request.project, &checkpoint.revision)
                        .is_err()
                    {
                        return Ok(unavailable(
                            request,
                            state.model_revision,
                            QueryUnavailableReason::MissingRevision,
                        ));
                    }
                    let known = store.graph_descriptor(
                        actor,
                        &request.project,
                        &checkpoint.revision,
                        PROJECTION_SCHEMA,
                    )?;
                    if known.as_ref().is_some_and(|g| &g.checkpoint != checkpoint) {
                        return Ok(unavailable(
                            request,
                            state.model_revision,
                            QueryUnavailableReason::MissingCheckpoint,
                        ));
                    }
                    if let Some(head) = &state.model_revision {
                        if !store.is_ancestor(
                            actor,
                            &request.project,
                            &checkpoint.revision,
                            head,
                        )? {
                            return Ok(unavailable(
                                request,
                                state.model_revision,
                                QueryUnavailableReason::UnknownLineage,
                            ));
                        }
                    }
                    if let Some(current) = &state.available_graph {
                        let satisfies = store.is_ancestor(
                            actor,
                            &request.project,
                            &checkpoint.revision,
                            &current.checkpoint.revision,
                        )?;
                        if (satisfies && known.is_some()) || *allow_older {
                            selected = graph_read(
                                &mut store,
                                actor,
                                &request.project,
                                &current.checkpoint.revision,
                            )?;
                        }
                    }
                }
                RevisionQuerySelector::Current { allow_older } => {
                    if let Some(current) = &state.available_graph {
                        if *allow_older
                            || state.model_revision.as_ref() == Some(&current.checkpoint.revision)
                        {
                            selected = graph_read(
                                &mut store,
                                actor,
                                &request.project,
                                &current.checkpoint.revision,
                            )?;
                        }
                    }
                }
            }
            // External/unknown current head cannot be portrayed as a known accepted model.
            if state.unavailable_reason.is_some()
                && !matches!(request.selector, RevisionQuerySelector::Exact { .. })
            {
                return Ok(unavailable(
                    request,
                    state.model_revision,
                    QueryUnavailableReason::ExternalRevision,
                ));
            }
            if let Some((graph, bytes)) = selected {
                // Exact queries are authoritative for the requested revision; the
                // current model head is only used for freshness comparison. If the
                // head observation was skipped or the store has no observed model
                // head yet, fall back to the graph's own revision — the indexed
                // graph is the answer for the exact query either way.
                let model_revision = state
                    .model_revision
                    .clone()
                    .unwrap_or_else(|| graph.checkpoint.revision.clone());
                let remaining = budget.saturating_sub(started.elapsed());
                if remaining.is_zero() {
                    return Ok(unavailable(
                        request,
                        Some(model_revision),
                        QueryUnavailableReason::DeadlineExceeded,
                    ));
                }
                let mut bounded = request.clone();
                bounded.deadline_ms = remaining.as_millis().max(1) as u64;
                let results = match engine.evaluate(&bounded, &graph, &bytes).await {
                    Ok(results) => results,
                    Err(e) => return Ok(unavailable(request, Some(model_revision), e.reason())),
                };
                // Auth revocation while the worker was running must prevent its result escaping.
                self.query_store(started, budget)?
                    .project_binding(actor, &request.project)?;
                let indexed_revision = graph.checkpoint.revision.clone();
                let freshness = if model_revision == indexed_revision {
                    QueryFreshness::Fresh
                } else {
                    QueryFreshness::Stale
                };
                let response = RevisionQueryResponse {
                    project: request.project.clone(),
                    branch: request.branch.clone(),
                    outcome: RevisionQueryOutcome::Completed {
                        model_revision,
                        indexed_revision,
                        graph,
                        freshness,
                        results,
                    },
                };
                response.validate_for(request)?;
                return Ok(response);
            }
            if started.elapsed() >= budget {
                let response = RevisionQueryResponse {
                    project: request.project.clone(),
                    branch: request.branch.clone(),
                    outcome: RevisionQueryOutcome::Pending {
                        model_revision: state.model_revision,
                        requested_revision: requested,
                        available_graph: state.available_graph,
                    },
                };
                response.validate_for(request)?;
                return Ok(response);
            }
            tokio::time::sleep(
                Duration::from_millis(10).min(budget.saturating_sub(started.elapsed())),
            )
            .await;
        }
    }
}
fn graph_read(
    store: &mut Store,
    actor: &ActorId,
    p: &ProjectId,
    r: &RevisionId,
) -> Result<Option<(RevisionGraphDescriptor, Vec<u8>)>> {
    Ok(store.graph(actor, p, r, PROJECTION_SCHEMA)?)
}
fn unavailable(
    request: &RevisionQueryRequest,
    model_revision: Option<RevisionId>,
    reason: QueryUnavailableReason,
) -> RevisionQueryResponse {
    let requested_revision = match &request.selector {
        RevisionQuerySelector::Exact { revision } => Some(revision.clone()),
        RevisionQuerySelector::Minimum { checkpoint, .. } => Some(checkpoint.revision.clone()),
        RevisionQuerySelector::Current { .. } => None,
    };
    RevisionQueryResponse {
        project: request.project.clone(),
        branch: request.branch.clone(),
        outcome: RevisionQueryOutcome::Unavailable {
            model_revision,
            requested_revision,
            reason,
        },
    }
}
