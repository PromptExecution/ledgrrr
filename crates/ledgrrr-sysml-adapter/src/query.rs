//! Authorized artifact selection belongs to the owner. This module evaluates
//! only the supplied, identity-checked immutable graph; no network or updates.
use crate::projection::{graph_iri, load_graph_with_check, ProjectionError};
use oxigraph::{
    model::Term,
    sparql::{CancellationToken, QueryEvaluationError, QueryResults, SparqlEvaluator},
};
use serde::{Deserialize, Serialize};
use spargebra::{
    algebra::{
        AggregateExpression, Expression, Function, GraphPattern, OrderExpression,
        PropertyPathExpression, QueryDataset,
    },
    term::NamedNodePattern,
    Query, SparqlParser,
};
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    sync::Semaphore,
};
use ufo_types::revision::{
    QueryUnavailableReason, RdfTerm, RevisionGraphDescriptor, RevisionQueryRequest,
    RevisionQueryResults, RevisionQuerySelector, MAX_REVISION_QUERY_BYTES,
    MAX_REVISION_QUERY_RESULT_BYTES, MAX_REVISION_QUERY_ROWS, MAX_REVISION_QUERY_VARIABLES,
};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryLimits {
    pub max_query_bytes: usize,
    pub max_ast_nodes: usize,
    pub max_ast_depth: usize,
    pub max_result_bytes: usize,
    pub max_rows: usize,
    pub max_graph_bytes: usize,
    pub max_quads: usize,
    pub max_workers: usize,
    pub max_deadline_ms: u64,
}
impl Default for QueryLimits {
    fn default() -> Self {
        Self {
            max_query_bytes: MAX_REVISION_QUERY_BYTES,
            max_ast_nodes: 10_000,
            max_ast_depth: 128,
            max_result_bytes: MAX_REVISION_QUERY_RESULT_BYTES,
            max_rows: MAX_REVISION_QUERY_ROWS,
            max_graph_bytes: 64 * 1024 * 1024,
            max_quads: 1_000_000,
            max_workers: 4,
            max_deadline_ms: 5000,
        }
    }
}
#[derive(Debug, thiserror::Error, Serialize, Deserialize)]
#[error("{reason:?}: {detail}")]
pub struct QueryError {
    pub reason: QueryUnavailableReason,
    pub detail: String,
}
impl QueryError {
    pub fn reason(&self) -> QueryUnavailableReason {
        self.reason
    }
}
fn error(reason: QueryUnavailableReason, detail: impl Into<String>) -> QueryError {
    QueryError {
        reason,
        detail: detail.into(),
    }
}
#[derive(Clone)]
pub struct QueryEngine {
    limits: QueryLimits,
    permits: Arc<Semaphore>,
    active: Arc<AtomicUsize>,
    telemetry: Arc<Telemetry>,
    worker_path: PathBuf,
}
#[derive(Default)]
struct Telemetry {
    started: AtomicUsize,
    cancelled: AtomicUsize,
    joined: AtomicUsize,
    lazy: AtomicUsize,
    killed: AtomicUsize,
}
struct CancelOnDrop(Option<tokio::sync::oneshot::Sender<()>>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        if let Some(sender) = self.0.take() {
            let _ = sender.send(());
        }
    }
}
struct Active(Arc<AtomicUsize>);
impl Drop for Active {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}
impl QueryEngine {
    pub fn new(limits: QueryLimits) -> Result<Self, QueryError> {
        let defaults = QueryLimits::default();
        if limits.max_workers == 0
            || limits.max_workers > 4
            || limits.max_ast_depth == 0
            || limits.max_ast_depth > 128
            || limits.max_ast_nodes == 0
            || limits.max_ast_nodes > 10_000
            || limits.max_query_bytes == 0
            || limits.max_query_bytes > defaults.max_query_bytes
            || limits.max_result_bytes == 0
            || limits.max_result_bytes > defaults.max_result_bytes
            || limits.max_rows == 0
            || limits.max_rows > defaults.max_rows
            || limits.max_graph_bytes == 0
            || limits.max_graph_bytes > defaults.max_graph_bytes
            || limits.max_quads == 0
            || limits.max_quads > defaults.max_quads
            || limits.max_deadline_ms == 0
            || limits.max_deadline_ms > 5000
        {
            return Err(error(
                QueryUnavailableReason::CapacityExceeded,
                "invalid service caps",
            ));
        }
        Ok(Self {
            permits: Arc::new(Semaphore::new(limits.max_workers)),
            active: Arc::new(AtomicUsize::new(0)),
            telemetry: Arc::new(Telemetry::default()),
            limits,
            worker_path: std::env::current_exe()
                .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?,
        })
    }
    /// Configure the same revision-owner binary for embedded hosts and tests.
    pub fn with_worker_path(mut self, path: impl AsRef<Path>) -> Self {
        self.worker_path = path.as_ref().to_path_buf();
        self
    }
    pub fn killed_workers(&self) -> usize {
        self.telemetry.killed.load(Ordering::SeqCst)
    }
    pub fn active_workers(&self) -> usize {
        self.active.load(Ordering::SeqCst)
    }
    pub fn evaluations_started(&self) -> usize {
        self.telemetry.started.load(Ordering::SeqCst)
    }
    pub fn evaluations_cancelled(&self) -> usize {
        self.telemetry.cancelled.load(Ordering::SeqCst)
    }
    pub fn lazy_evaluations_started(&self) -> usize {
        self.telemetry.lazy.load(Ordering::SeqCst)
    }
    pub fn joined_workers(&self) -> usize {
        self.telemetry.joined.load(Ordering::SeqCst)
    }
    /// Deadline includes queue, graph verification/loading, parsing and all lazy
    /// results. The worker requests cooperative token cancellation shortly before
    /// the hard deadline. Deadline, disconnect and pipe overflow kill any remaining
    /// process; the supervisor reaps and joins pipes before releasing capacity.
    pub async fn evaluate(
        &self,
        request: &RevisionQueryRequest,
        descriptor: &RevisionGraphDescriptor,
        bytes: &[u8],
    ) -> Result<RevisionQueryResults, QueryError> {
        let started = Instant::now();
        request
            .validate()
            .map_err(|e| error(QueryUnavailableReason::InvalidQuery, e.to_string()))?;
        if request.deadline_ms > self.limits.max_deadline_ms {
            return Err(error(
                QueryUnavailableReason::DeadlineExceeded,
                "deadline exceeds service cap",
            ));
        }
        if request.query.len() > self.limits.max_query_bytes
            || bytes.len() > self.limits.max_graph_bytes
            || descriptor.quad_count > self.limits.max_quads as u64
        {
            return Err(error(QueryUnavailableReason::CapacityExceeded, "input cap"));
        }
        if descriptor.checkpoint.project != request.project {
            return Err(error(
                QueryUnavailableReason::MissingCheckpoint,
                "project mismatch",
            ));
        }
        match &request.selector {
            RevisionQuerySelector::Exact { revision }
                if revision != &descriptor.checkpoint.revision =>
            {
                return Err(error(
                    QueryUnavailableReason::MissingCheckpoint,
                    "exact revision mismatch",
                ))
            }
            RevisionQuerySelector::Minimum { checkpoint, .. }
                if checkpoint.revision == descriptor.checkpoint.revision
                    && checkpoint != &descriptor.checkpoint =>
            {
                return Err(error(
                    QueryUnavailableReason::MissingCheckpoint,
                    "minimum checkpoint mismatch",
                ))
            }
            _ => {}
        }
        let budget = Duration::from_millis(request.deadline_ms);
        let deadline = tokio::time::Instant::from_std(started) + budget;
        let permit = tokio::time::timeout_at(deadline, self.permits.clone().acquire_owned())
            .await
            .map_err(|_| {
                error(
                    QueryUnavailableReason::DeadlineExceeded,
                    "worker queue deadline",
                )
            })?
            .map_err(|_| {
                error(
                    QueryUnavailableReason::CapacityExceeded,
                    "worker queue closed",
                )
            })?;
        let (disconnect_tx, disconnect_rx) = tokio::sync::oneshot::channel();
        let _cancel_guard = CancelOnDrop(Some(disconnect_tx));
        let input = WorkerInput {
            query: request.query.clone(),
            descriptor: descriptor.clone(),
            graph: String::from_utf8(bytes.to_vec())
                .map_err(|e| error(QueryUnavailableReason::CorruptArtifact, e.to_string()))?,
            limits: self.limits.clone(),
            deadline_epoch_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
                .as_millis()
                + budget.saturating_sub(started.elapsed()).as_millis(),
        };
        let path = self.worker_path.clone();
        let telemetry = self.telemetry.clone();
        let active = self.active.clone();
        // This supervisor survives caller cancellation and owns both the permit
        // and the child until wait() has reaped it and every pipe task has joined.
        tokio::spawn(async move {
            let _permit = permit;
            supervise(&path, input, deadline, disconnect_rx, active, telemetry).await
        })
        .await
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
    }
}
#[derive(Serialize, Deserialize)]
struct WorkerInput {
    query: String,
    descriptor: RevisionGraphDescriptor,
    #[serde(skip)]
    graph: String,
    limits: QueryLimits,
    deadline_epoch_ms: u128,
}
const WORKER_INPUT_CAP: u64 = 65 * 1024 * 1024;
fn event(name: &str) {
    let _ = writeln!(std::io::stderr(), "{name}");
}
/// Hidden binary entry, before creating the owner Tokio runtime. Safe rlimit
/// calls bound this expendable process, never the long-lived owner.
pub fn worker_main() -> Result<(), String> {
    rlimit::setrlimit(rlimit::Resource::AS, 1024 * 1024 * 1024, 1024 * 1024 * 1024)
        .map_err(|e| e.to_string())?;
    rlimit::setrlimit(rlimit::Resource::CPU, 6, 6).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(WORKER_INPUT_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > WORKER_INPUT_CAP {
        return Err("worker input cap".into());
    }
    let split = bytes
        .iter()
        .position(|b| *b == b'\n')
        .ok_or("missing worker header")?;
    if split > 1024 * 1024 {
        return Err("worker header cap".into());
    }
    let mut input: WorkerInput =
        serde_json::from_slice(&bytes[..split]).map_err(|e| e.to_string())?;
    input.graph = String::from_utf8(bytes.split_off(split + 1)).map_err(|e| e.to_string())?;
    let token = CancellationToken::new();
    let timer_token = token.clone();
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let budget =
        Duration::from_millis(input.deadline_epoch_ms.saturating_sub(now_ms).min(5000) as u64);
    std::thread::spawn(move || {
        std::thread::sleep(budget.saturating_sub(Duration::from_millis(25)));
        timer_token.cancel();
    });
    let telemetry = Telemetry::default();
    let result = evaluate_worker(&input, &token, Instant::now(), budget, &telemetry);
    serde_json::to_writer(std::io::stdout(), &result).map_err(|e| e.to_string())?;
    Ok(())
}
async fn supervise(
    path: &Path,
    input: WorkerInput,
    deadline: tokio::time::Instant,
    mut disconnect: tokio::sync::oneshot::Receiver<()>,
    active: Arc<AtomicUsize>,
    telemetry: Arc<Telemetry>,
) -> Result<RevisionQueryResults, QueryError> {
    let encoded = serde_json::to_vec(&input)
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?;
    let mut child = tokio::process::Command::new(path)
        .arg("query-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?;
    active.fetch_add(1, Ordering::SeqCst);
    let _active = Active(active);
    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let cap = input.limits.max_result_bytes.saturating_add(4096);
    let graph = input.graph;
    let writer = tokio::spawn(async move {
        if let Some(mut pipe) = stdin {
            pipe.write_all(&encoded).await?;
            pipe.write_all(b"\n").await?;
            pipe.write_all(graph.as_bytes()).await?;
            pipe.shutdown().await?;
        }
        Ok::<_, std::io::Error>(())
    });
    let (overflow_tx, mut overflow_rx) = tokio::sync::mpsc::channel(2);
    let stderr_overflow = overflow_tx.clone();
    let reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        if let Some(pipe) = stdout {
            pipe.take(cap as u64 + 1).read_to_end(&mut bytes).await?;
        }
        if bytes.len() > cap {
            let _ = overflow_tx.try_send(());
        } else {
            drop(overflow_tx);
        }
        Ok::<_, std::io::Error>(bytes)
    });
    let stats = telemetry.clone();
    let errors = tokio::spawn(async move {
        if let Some(pipe) = stderr {
            let mut reader = tokio::io::BufReader::new(pipe).take(4097);
            let mut total = 0;
            loop {
                let mut line = Vec::new();
                let n = reader.read_until(b'\n', &mut line).await?;
                if n == 0 {
                    break;
                }
                total += n;
                if total > 4096 {
                    let _ = stderr_overflow.try_send(());
                    return Err(std::io::Error::other("stderr cap"));
                }
                match line.as_slice() {
                    b"started\n" => {
                        stats.started.fetch_add(1, Ordering::SeqCst);
                    }
                    b"lazy\n" => {
                        stats.lazy.fetch_add(1, Ordering::SeqCst);
                    }
                    b"cancelled\n" => {
                        stats.cancelled.fetch_add(1, Ordering::SeqCst);
                    }
                    _ => {}
                }
            }
        }
        Ok::<_, std::io::Error>(())
    });
    let mut forced = None;
    let status = tokio::select! {
        status=child.wait()=>status,
        _=tokio::time::sleep_until(deadline)=>{forced=Some(QueryUnavailableReason::DeadlineExceeded);let _=child.start_kill();child.wait().await},
        _=&mut disconnect=>{forced=Some(QueryUnavailableReason::DeadlineExceeded);let _=child.start_kill();child.wait().await},
        Some(())=overflow_rx.recv()=>{forced=Some(QueryUnavailableReason::CapacityExceeded);let _=child.start_kill();child.wait().await}
    };
    // Always reap first, then join bounded pipes, then release capacity.
    let written = writer.await;
    let output = reader.await;
    let diagnostics = errors.await;
    telemetry.joined.fetch_add(1, Ordering::SeqCst);
    if let Some(reason) = forced {
        telemetry.killed.fetch_add(1, Ordering::SeqCst);
        return Err(error(reason, "worker killed and reaped"));
    }
    let status =
        status.map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?;
    written
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?;
    diagnostics
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
        .map_err(|e| error(QueryUnavailableReason::CapacityExceeded, e.to_string()))?;
    let output = output
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?;
    if !status.success() {
        return Err(error(
            QueryUnavailableReason::CapacityExceeded,
            "worker resource limit or failure",
        ));
    }
    serde_json::from_slice(&output)
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
}
impl From<std::io::Error> for QueryError {
    fn from(e: std::io::Error) -> Self {
        error(QueryUnavailableReason::EvaluationFailed, e.to_string())
    }
}
fn check(token: &CancellationToken, started: Instant, budget: Duration) -> Result<(), QueryError> {
    if token.is_cancelled() || started.elapsed() >= budget {
        token.cancel();
        Err(error(
            QueryUnavailableReason::DeadlineExceeded,
            "evaluation deadline",
        ))
    } else {
        Ok(())
    }
}
struct Ast<'a> {
    count: usize,
    limits: &'a QueryLimits,
    graph: &'a str,
}
impl Ast<'_> {
    fn enter(&mut self, depth: usize, n: usize) -> Result<(), QueryError> {
        self.count = self.count.saturating_add(n);
        if depth > self.limits.max_ast_depth || self.count > self.limits.max_ast_nodes {
            Err(error(QueryUnavailableReason::CapacityExceeded, "AST cap"))
        } else {
            Ok(())
        }
    }
    fn path(&mut self, p: &PropertyPathExpression, d: usize) -> Result<(), QueryError> {
        self.enter(d, 1)?;
        match p {
            PropertyPathExpression::Reverse(p)
            | PropertyPathExpression::ZeroOrMore(p)
            | PropertyPathExpression::OneOrMore(p)
            | PropertyPathExpression::ZeroOrOne(p) => self.path(p, d + 1)?,
            PropertyPathExpression::Sequence(a, b) | PropertyPathExpression::Alternative(a, b) => {
                self.path(a, d + 1)?;
                self.path(b, d + 1)?;
            }
            PropertyPathExpression::NegatedPropertySet(v) => self.enter(d + 1, v.len())?,
            PropertyPathExpression::NamedNode(_) => {}
        }
        Ok(())
    }
    fn expression(&mut self, e: &Expression, d: usize) -> Result<(), QueryError> {
        self.enter(d, 1)?;
        use Expression::*;
        match e {
            Or(a, b)
            | And(a, b)
            | Equal(a, b)
            | SameTerm(a, b)
            | Greater(a, b)
            | GreaterOrEqual(a, b)
            | Less(a, b)
            | LessOrEqual(a, b)
            | Add(a, b)
            | Subtract(a, b)
            | Multiply(a, b)
            | Divide(a, b) => {
                self.expression(a, d + 1)?;
                self.expression(b, d + 1)?;
            }
            In(a, values) => {
                self.expression(a, d + 1)?;
                for v in values {
                    self.expression(v, d + 1)?;
                }
            }
            UnaryPlus(a) | UnaryMinus(a) | Not(a) => self.expression(a, d + 1)?,
            Exists(p) => self.pattern(p, d + 1)?,
            If(a, b, c) => {
                self.expression(a, d + 1)?;
                self.expression(b, d + 1)?;
                self.expression(c, d + 1)?;
            }
            Coalesce(v) => {
                for e in v {
                    self.expression(e, d + 1)?;
                }
            }
            FunctionCall(f, v) => {
                if matches!(f, Function::Custom(_)) {
                    return Err(error(
                        QueryUnavailableReason::UnsupportedQuery,
                        "custom function",
                    ));
                }
                for e in v {
                    self.expression(e, d + 1)?;
                }
            }
            NamedNode(_) | Literal(_) | Variable(_) | Bound(_) => {}
        }
        Ok(())
    }
    fn pattern(&mut self, p: &GraphPattern, d: usize) -> Result<(), QueryError> {
        self.enter(d, 1)?;
        use GraphPattern::*;
        match p {
            Service { .. } => {
                return Err(error(
                    QueryUnavailableReason::UnsupportedQuery,
                    "SERVICE forbidden",
                ))
            }
            Graph { name, inner } => {
                if !matches!(name,NamedNodePattern::NamedNode(n) if n.as_str()==self.graph) {
                    return Err(error(
                        QueryUnavailableReason::UnsupportedQuery,
                        "graph outside selected revision",
                    ));
                }
                self.pattern(inner, d + 1)?;
            }
            Join { left, right }
            | Union { left, right }
            | Minus { left, right }
            | Lateral { left, right } => {
                self.pattern(left, d + 1)?;
                self.pattern(right, d + 1)?;
            }
            LeftJoin {
                left,
                right,
                expression,
            } => {
                self.pattern(left, d + 1)?;
                self.pattern(right, d + 1)?;
                if let Some(e) = expression {
                    self.expression(e, d + 1)?;
                }
            }
            Filter { expr, inner } => {
                self.expression(expr, d + 1)?;
                self.pattern(inner, d + 1)?;
            }
            Extend {
                inner, expression, ..
            } => {
                self.expression(expression, d + 1)?;
                self.pattern(inner, d + 1)?;
            }
            OrderBy { inner, expression } => {
                for e in expression {
                    let (OrderExpression::Asc(e) | OrderExpression::Desc(e)) = e;
                    self.expression(e, d + 1)?;
                }
                self.pattern(inner, d + 1)?;
            }
            Group {
                inner,
                variables,
                aggregates,
            } => {
                self.enter(d + 1, variables.len() + aggregates.len())?;
                for (_, a) in aggregates {
                    if let AggregateExpression::FunctionCall { expr, .. } = a {
                        self.expression(expr, d + 1)?;
                    }
                }
                self.pattern(inner, d + 1)?;
            }
            Project { inner, variables } => {
                self.enter(d + 1, variables.len())?;
                self.pattern(inner, d + 1)?;
            }
            Distinct { inner } | Reduced { inner } | Slice { inner, .. } => {
                self.pattern(inner, d + 1)?
            }
            Bgp { patterns } => self.enter(d + 1, patterns.len().saturating_mul(3))?,
            Values {
                variables,
                bindings,
            } => self.enter(
                d + 1,
                variables.len() + bindings.len() + bindings.iter().map(Vec::len).sum::<usize>(),
            )?,
            Path { path, .. } => self.path(path, d + 1)?,
        }
        Ok(())
    }
}
fn term(value: &Term) -> Result<RdfTerm, QueryError> {
    Ok(match value {
        Term::NamedNode(n) => RdfTerm::Iri {
            value: n.as_str().into(),
        },
        Term::BlankNode(n) => RdfTerm::BlankNode {
            label: n.as_str().into(),
        },
        Term::Literal(l) => match l.language() {
            Some(language) => RdfTerm::LanguageLiteral {
                value: l.value().into(),
                language: language.into(),
            },
            None => RdfTerm::Literal {
                value: l.value().into(),
                datatype: l.datatype().as_str().into(),
            },
        },
    })
}
// Resource preflight only, never a substitute SPARQL parser. Skip comments,
// quoted strings and IRI references so data containing brackets is not syntax.
fn lexical_depth(query: &str, max: usize) -> Result<(), QueryError> {
    let bytes = query.as_bytes();
    let (mut i, mut depth) = (0usize, 0usize);
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            quote @ (b'\'' | b'"') => {
                let long = bytes.get(i + 1) == Some(&quote) && bytes.get(i + 2) == Some(&quote);
                i += if long { 3 } else { 1 };
                while i < bytes.len() {
                    if bytes[i] == b'\\' {
                        i = (i + 2).min(bytes.len());
                        continue;
                    }
                    if bytes[i] == quote
                        && (!long
                            || (bytes.get(i + 1) == Some(&quote)
                                && bytes.get(i + 2) == Some(&quote)))
                    {
                        i += if long { 3 } else { 1 };
                        break;
                    }
                    i += 1;
                }
            }
            b'<' => {
                let mut end = i + 1;
                while end < bytes.len() && bytes[end] != b'>' && !bytes[end].is_ascii_whitespace() {
                    end += 1;
                }
                let candidate = &bytes[i + 1..end];
                let absolute = candidate
                    .iter()
                    .position(|b| *b == b':')
                    .is_some_and(|colon| {
                        colon > 0
                            && candidate[0].is_ascii_alphabetic()
                            && candidate[..colon].iter().all(|b| {
                                b.is_ascii_alphanumeric() || matches!(*b, b'+' | b'-' | b'.')
                            })
                    });
                if bytes.get(end) == Some(&b'>') && absolute {
                    i = end + 1;
                } else {
                    i += 1;
                }
            }
            b'(' | b'{' | b'[' => {
                depth += 1;
                if depth > max {
                    return Err(error(
                        QueryUnavailableReason::CapacityExceeded,
                        "syntax depth cap",
                    ));
                }
                i += 1;
            }
            b')' | b'}' | b']' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    Ok(())
}
fn evaluate_worker(
    input: &WorkerInput,
    token: &CancellationToken,
    started: Instant,
    budget: Duration,
    telemetry: &Telemetry,
) -> Result<RevisionQueryResults, QueryError> {
    let query = &input.query;
    let descriptor = &input.descriptor;
    let bytes = input.graph.as_bytes();
    let limits = &input.limits;
    check(token, started, budget)?;
    lexical_depth(query, limits.max_ast_depth)?;
    let mut parsed = SparqlParser::new()
        .parse_query(query)
        .map_err(|e| error(QueryUnavailableReason::InvalidQuery, e.to_string()))?;
    let pattern = match &parsed {
        Query::Select {
            dataset: None,
            pattern,
            ..
        }
        | Query::Ask {
            dataset: None,
            pattern,
            ..
        } => pattern,
        Query::Select { .. } | Query::Ask { .. } => {
            return Err(error(
                QueryUnavailableReason::UnsupportedQuery,
                "FROM forbidden",
            ))
        }
        _ => {
            return Err(error(
                QueryUnavailableReason::UnsupportedQuery,
                "only SELECT/ASK",
            ))
        }
    };
    let graph = graph_iri(
        &descriptor.checkpoint.project,
        &descriptor.checkpoint.revision,
    );
    Ast {
        count: 0,
        limits,
        graph: &graph,
    }
    .pattern(pattern, 0)?;
    check(token, started, budget)?;
    let store = load_graph_with_check(descriptor, bytes, || {
        check(token, started, budget).map_err(|_| ProjectionError::Limit("deadline"))
    })
    .map_err(|e| {
        if token.is_cancelled() {
            error(QueryUnavailableReason::DeadlineExceeded, e.to_string())
        } else {
            error(QueryUnavailableReason::CorruptArtifact, e.to_string())
        }
    })?;
    check(token, started, budget)?;
    // Owner-selected named graph is the sole default and named dataset. Caller
    // dataset clauses have already been refused, so they cannot override this.
    let selected = QueryDataset {
        default: vec![oxigraph::model::NamedNode::new_unchecked(&graph)],
        named: Some(vec![oxigraph::model::NamedNode::new_unchecked(&graph)]),
    };
    match &mut parsed {
        Query::Select { dataset, .. } | Query::Ask { dataset, .. } => *dataset = Some(selected),
        _ => unreachable!(),
    }
    telemetry.started.fetch_add(1, Ordering::SeqCst);
    event("started");
    let evaluated = SparqlEvaluator::new()
        .with_cancellation_token(token.clone())
        .for_query(parsed)
        .on_store(&store)
        .execute()
        .map_err(|e| evaluation_error(e, telemetry))?;
    let results = match evaluated {
        QueryResults::Boolean(value) => RevisionQueryResults::Ask { value },
        QueryResults::Solutions(solutions) => {
            telemetry.lazy.fetch_add(1, Ordering::SeqCst);
            event("lazy");
            let variables: Vec<String> = solutions
                .variables()
                .iter()
                .map(|v| v.as_str().into())
                .collect();
            if variables.len() > MAX_REVISION_QUERY_VARIABLES {
                token.cancel();
                return Err(error(
                    QueryUnavailableReason::CapacityExceeded,
                    "projected variable cap",
                ));
            }
            let mut size = serde_json::to_vec(&variables)
                .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
                .len()
                + 128;
            let mut rows = Vec::new();
            for solution in solutions {
                let solution = solution.map_err(|e| evaluation_error(e, telemetry))?;
                check(token, started, budget)?;
                if rows.len() >= limits.max_rows {
                    token.cancel();
                    return Err(error(QueryUnavailableReason::CapacityExceeded, "row cap"));
                }
                let row: BTreeMap<String, RdfTerm> = solution
                    .iter()
                    .map(|(v, t)| Ok((v.as_str().into(), term(t)?)))
                    .collect::<Result<_, QueryError>>()?;
                size = size.saturating_add(
                    serde_json::to_vec(&row)
                        .map_err(|e| {
                            error(QueryUnavailableReason::EvaluationFailed, e.to_string())
                        })?
                        .len()
                        + 1,
                );
                if size > limits.max_result_bytes {
                    token.cancel();
                    return Err(error(
                        QueryUnavailableReason::CapacityExceeded,
                        "result byte cap",
                    ));
                }
                rows.push(row);
            }
            RevisionQueryResults::Select { variables, rows }
        }
        _ => {
            return Err(error(
                QueryUnavailableReason::UnsupportedQuery,
                "unexpected result kind",
            ))
        }
    };
    check(token, started, budget)?;
    results
        .validate()
        .map_err(|e| error(QueryUnavailableReason::CapacityExceeded, e.to_string()))?;
    if serde_json::to_vec(&results)
        .map_err(|e| error(QueryUnavailableReason::EvaluationFailed, e.to_string()))?
        .len()
        > limits.max_result_bytes
    {
        token.cancel();
        return Err(error(
            QueryUnavailableReason::CapacityExceeded,
            "result byte cap",
        ));
    }
    Ok(results)
}

fn evaluation_error(e: QueryEvaluationError, telemetry: &Telemetry) -> QueryError {
    if matches!(e, QueryEvaluationError::Cancelled) {
        telemetry.cancelled.fetch_add(1, Ordering::SeqCst);
        event("cancelled");
        error(QueryUnavailableReason::DeadlineExceeded, e.to_string())
    } else {
        error(QueryUnavailableReason::EvaluationFailed, e.to_string())
    }
}
