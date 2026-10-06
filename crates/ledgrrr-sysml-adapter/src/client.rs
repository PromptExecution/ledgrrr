//! Bounded native reads. No generic request or public mutation entry point exists.
use crate::native::{decode_json, NativeElement};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct Bounds {
    pub deadline: Duration,
    pub max_bytes: usize,
    pub max_pages: usize,
    pub max_ids: usize,
    pub page_size: usize,
}
impl Default for Bounds {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(20),
            max_bytes: 32 * 1024 * 1024,
            max_pages: 100,
            max_ids: 10000,
            page_size: 100,
        }
    }
}
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("incomplete native fetch: {0}")]
    Incomplete(String),
    #[error("invalid native protocol: {0}")]
    Protocol(String),
    #[error("native HTTP status {0}")]
    Status(u16),
    #[error(transparent)]
    Http(#[from] reqwest::Error),
    #[error(transparent)]
    Codec(#[from] crate::native::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NativeCommit {
    pub id: String,
    pub parent: Option<String>,
}
#[derive(Clone)]
pub struct NativeClient {
    origin: Url,
    http: Client,
    bounds: Bounds,
}
struct Budget {
    until: Instant,
    bytes: usize,
    requests: usize,
}
impl Budget {
    fn new(b: &Bounds) -> Self {
        Self {
            until: Instant::now() + b.deadline,
            bytes: 0,
            requests: 0,
        }
    }
}
impl NativeClient {
    pub fn new(origin: &str, bounds: Bounds) -> Result<Self> {
        let origin =
            Url::parse(origin).map_err(|_| Error::Protocol("invalid configured origin".into()))?;
        if !matches!(origin.scheme(), "http" | "https")
            || origin.host_str().is_none()
            || !origin.username().is_empty()
            || origin.password().is_some()
            || origin.query().is_some()
            || origin.fragment().is_some()
            || origin.path() != "/"
        {
            return Err(Error::Protocol(
                "origin must be a credential-free HTTP authority".into(),
            ));
        }
        if bounds.deadline.is_zero()
            || bounds.max_bytes == 0
            || bounds.max_pages == 0
            || bounds.max_ids == 0
            || bounds.page_size == 0
            || bounds.page_size > 100
        {
            return Err(Error::Protocol("invalid fetch bounds".into()));
        }
        let http = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .no_proxy()
            .timeout(bounds.deadline)
            .build()?;
        Ok(Self {
            origin,
            http,
            bounds,
        })
    }
    fn url(&self, segments: &[&str]) -> Result<Url> {
        let mut url = self.origin.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|_| Error::Protocol("invalid origin".into()))?;
            path.clear();
            for id in segments {
                if id.trim().is_empty()
                    || id.chars().any(char::is_control)
                    || id.contains(['/', '\\', '?', '#', '%'])
                    || matches!(*id, "." | "..")
                {
                    return Err(Error::Protocol("invalid route identity".into()));
                }
                path.push(id);
            }
        }
        Ok(url)
    }
    async fn response(
        &self,
        request: reqwest::RequestBuilder,
        budget: &mut Budget,
    ) -> Result<(Value, Option<String>)> {
        if budget.requests >= self.bounds.max_pages {
            return Err(Error::Incomplete("request/page bound reached".into()));
        }
        budget.requests += 1;
        let remaining = budget
            .until
            .checked_duration_since(Instant::now())
            .ok_or_else(|| Error::Incomplete("deadline exceeded".into()))?;
        let response = tokio::time::timeout(remaining, async {
            let mut response = request.send().await?;
            let status = response.status();
            if !status.is_success() {
                return Err(Error::Status(status.as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|n| n > self.bounds.max_bytes.saturating_sub(budget.bytes) as u64)
            {
                return Err(Error::Incomplete("byte bound reached".into()));
            }
            let links = response
                .headers()
                .get_all(reqwest::header::LINK)
                .iter()
                .map(|h| {
                    h.to_str()
                        .map(str::to_owned)
                        .map_err(|_| Error::Protocol("invalid Link encoding".into()))
                })
                .collect::<Result<Vec<_>>>()?;
            let link = if links.is_empty() {
                None
            } else {
                Some(links.join(","))
            };
            let mut bytes = vec![];
            while let Some(chunk) = response.chunk().await? {
                if chunk.len() > self.bounds.max_bytes.saturating_sub(budget.bytes) {
                    return Err(Error::Incomplete("byte bound reached".into()));
                }
                budget.bytes += chunk.len();
                bytes.extend_from_slice(&chunk);
            }
            Ok((decode_json(&bytes)?, link))
        })
        .await
        .map_err(|_| Error::Incomplete("deadline exceeded".into()))??;
        Ok(response)
    }
    pub async fn head(&self, project: &str, branch: &str) -> Result<Option<String>> {
        self.head_budget(project, branch, &mut Budget::new(&self.bounds))
            .await
    }
    async fn head_budget(
        &self,
        project: &str,
        branch: &str,
        budget: &mut Budget,
    ) -> Result<Option<String>> {
        let url = self.url(&["projects", project, "branches", branch])?;
        let (v, _) = self.response(self.http.get(url), budget).await?;
        if v["@id"].as_str() != Some(branch) || v["owningProject"]["@id"].as_str() != Some(project)
        {
            return Err(Error::Protocol("branch/project identity mismatch".into()));
        }
        if !v.as_object().is_some_and(|v| v.contains_key("head")) {
            return Err(Error::Protocol("missing branch head evidence".into()));
        }
        reference(&v["head"])
    }
    pub async fn commit(&self, project: &str, revision: &str) -> Result<NativeCommit> {
        self.commit_budget(project, revision, &mut Budget::new(&self.bounds))
            .await
    }
    async fn commit_budget(
        &self,
        project: &str,
        revision: &str,
        budget: &mut Budget,
    ) -> Result<NativeCommit> {
        let (v, _) = self
            .response(
                self.http
                    .get(self.url(&["projects", project, "commits", revision])?),
                budget,
            )
            .await?;
        let c = parse_commit(&v)?;
        if c.id != revision
            || v.get("owningProject")
                .is_some_and(|p| !p.is_null() && p["@id"].as_str() != Some(project))
        {
            return Err(Error::Protocol("exact commit/project mismatch".into()));
        }
        Ok(c)
    }
    pub async fn snapshot(
        &self,
        project: &str,
        revision: &str,
        exclude_used: bool,
    ) -> Result<Vec<NativeElement>> {
        let mut url = self.url(&["projects", project, "commits", revision, "elements"])?;
        url.query_pairs_mut()
            .append_pair("excludeUsed", if exclude_used { "true" } else { "false" })
            .append_pair("page[size]", &self.bounds.page_size.to_string());
        let values = self.pages(url, &mut Budget::new(&self.bounds)).await?;
        values
            .into_iter()
            .map(|v| serde_json::from_value(v).map_err(Error::from))
            .collect()
    }
    pub async fn changes(&self, project: &str, revision: &str) -> Result<Vec<Value>> {
        let mut url = self.url(&["projects", project, "commits", revision, "changes"])?;
        url.query_pairs_mut()
            .append_pair("page[size]", &self.bounds.page_size.to_string());
        self.pages(url, &mut Budget::new(&self.bounds)).await
    }
    async fn pages(&self, initial: Url, budget: &mut Budget) -> Result<Vec<Value>> {
        let mut current = initial.clone();
        let mut seen_cursors = BTreeSet::new();
        let mut ids = BTreeSet::new();
        let mut out = vec![];
        loop {
            let (value, link) = self
                .response(self.http.get(current.clone()), budget)
                .await?;
            let page = value
                .as_array()
                .ok_or_else(|| Error::Protocol("inventory must be an array".into()))?;
            if page.len() > self.bounds.page_size {
                return Err(Error::Protocol("oversized inventory page".into()));
            }
            for row in page {
                let id = row["@id"]
                    .as_str()
                    .ok_or_else(|| Error::Protocol("missing inventory identity".into()))?;
                if id.trim().is_empty() || !ids.insert(id.to_owned()) {
                    return Err(Error::Protocol("duplicate/empty inventory identity".into()));
                }
                if ids.len() > self.bounds.max_ids {
                    return Err(Error::Incomplete("identity bound reached".into()));
                }
                out.push(row.clone());
            }
            match link
                .as_deref()
                .map(|l| self.next_cursor(&initial, l))
                .transpose()?
                .flatten()
            {
                Some(cursor) => {
                    if page.is_empty() || !seen_cursors.insert(cursor.clone()) {
                        return Err(Error::Protocol(
                            "empty page with next link or repeated cursor".into(),
                        ));
                    }
                    current = initial.clone();
                    current
                        .query_pairs_mut()
                        .append_pair("page[after]", &cursor);
                }
                None => {
                    // A full page without next cannot prove complete inventory.
                    if page.len() == self.bounds.page_size {
                        return Err(Error::Incomplete(
                            "full final page lacks continuation".into(),
                        ));
                    }
                    return Ok(out);
                }
            }
        }
    }
    fn next_cursor(&self, initial: &Url, link: &str) -> Result<Option<String>> {
        let mut next = None;
        for part in link.split(',') {
            let part = part.trim();
            let end = part
                .find('>')
                .ok_or_else(|| Error::Protocol("malformed Link".into()))?;
            if !part.starts_with('<') {
                return Err(Error::Protocol("malformed Link".into()));
            }
            let relations: Vec<_> = part[end + 1..]
                .split(';')
                .map(str::trim)
                .filter_map(|p| p.strip_prefix("rel="))
                .collect();
            if relations.len() != 1 {
                return Err(Error::Protocol("malformed Link relation".into()));
            }
            let rel = relations[0].trim_matches('"');
            if rel != "next" && rel != "prev" {
                return Err(Error::Protocol("unsupported Link relation".into()));
            }
            let url = Url::parse(&part[1..end])
                .map_err(|_| Error::Protocol("malformed Link URL".into()))?;
            // The pinned provider hardcodes http. Reconstruct configured scheme;
            // never follow supplied URL. Host, effective port and exact path must match.
            if url.host_str() != initial.host_str()
                || url.port_or_known_default() != initial.port_or_known_default()
                || url.path() != initial.path()
                || !url.username().is_empty()
                || url.password().is_some()
                || url.fragment().is_some()
                || !(url.scheme() == initial.scheme()
                    || (initial.scheme() == "https"
                        && url.scheme() == "http"
                        && url.port().is_some()))
            {
                return Err(Error::Protocol(
                    "cross-origin or cross-resource Link".into(),
                ));
            }
            let mut cursor = None;
            let mut keys = BTreeSet::new();
            for (key, value) in url.query_pairs() {
                if !keys.insert(key.to_string()) {
                    return Err(Error::Protocol("duplicate Link query key".into()));
                }
                match key.as_ref() {
                    "page[after]" if rel == "next" => cursor = Some(value.to_string()),
                    "page[before]" if rel == "prev" => {}
                    "page[size]" if value.parse::<usize>().ok() == Some(self.bounds.page_size) => {}
                    "excludeUsed" if initial.query_pairs().any(|(k, v)| k == key && v == value) => {
                    }
                    _ => return Err(Error::Protocol("altered Link query".into())),
                }
            }
            if rel == "next" {
                let cursor =
                    cursor.ok_or_else(|| Error::Protocol("missing after cursor".into()))?;
                validate_cursor(&cursor)?;
                if next.replace(cursor).is_some() {
                    return Err(Error::Protocol("multiple next cursors".into()));
                }
            }
        }
        Ok(next)
    }
    /// Only exact commits reachable from this branch qualify as reconciliation evidence.
    pub async fn history(&self, project: &str, branch: &str) -> Result<Vec<NativeCommit>> {
        let mut budget = Budget::new(&self.bounds);
        let mut next = self.head_budget(project, branch, &mut budget).await?;
        let mut seen = BTreeSet::new();
        let mut result = vec![];
        while let Some(id) = next {
            if !seen.insert(id.clone()) {
                return Err(Error::Protocol("history cycle".into()));
            }
            if seen.len() > self.bounds.max_ids {
                return Err(Error::Incomplete("history identity bound".into()));
            }
            let commit = self.commit_budget(project, &id, &mut budget).await?;
            next = commit.parent.clone();
            result.push(commit);
        }
        // Head changes during a bounded traversal make its observation inconsistent.
        if self.head_budget(project, branch, &mut budget).await?
            != result.first().map(|c| c.id.clone())
        {
            return Err(Error::Incomplete(
                "branch changed during history fetch".into(),
            ));
        }
        Ok(result)
    }
    #[allow(dead_code)] // Used only by the owner promotion worker, never an external API.
    pub(crate) async fn create_commit(
        &self,
        project: &str,
        branch: &str,
        previous: Option<&str>,
        elements: &[NativeElement],
        deleted: &[String],
    ) -> Result<NativeCommit> {
        let mut url = self.url(&["projects", project, "commits"])?;
        url.query_pairs_mut().append_pair("branchId", branch);
        let mut changes:Vec<_>=elements.iter().map(|e|json!({"@type":"DataVersion","identity":{"@id":e.id,"@type":"DataIdentity"},"payload":e})).collect();
        changes.extend(deleted.iter().map(|id|json!({"@type":"DataVersion","identity":{"@id":id,"@type":"DataIdentity"},"payload":null})));
        let body = json!({"@type":"Commit","previousCommit":previous.map(|id|json!({"@id":id})),"change":changes});
        let bytes = serde_json::to_vec(&body)?;
        if bytes.len() > self.bounds.max_bytes {
            return Err(Error::Incomplete("commit request byte bound".into()));
        }
        let (value, _) = self
            .response(
                self.http
                    .post(url)
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(bytes),
                &mut Budget::new(&self.bounds),
            )
            .await?;
        parse_commit(&value)
    }
}
fn reference(v: &Value) -> Result<Option<String>> {
    if v.is_null() {
        return Ok(None);
    }
    let id = v["@id"]
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Error::Protocol("malformed reference".into()))?;
    Ok(Some(id.into()))
}
fn parse_commit(value: &Value) -> Result<NativeCommit> {
    let id = value["@id"]
        .as_str()
        .filter(|id| !id.trim().is_empty())
        .ok_or_else(|| Error::Protocol("missing commit id".into()))?
        .to_owned();
    if !value
        .as_object()
        .is_some_and(|v| v.contains_key("previousCommit"))
    {
        return Err(Error::Protocol("missing commit parent evidence".into()));
    }
    let parent = reference(&value["previousCommit"])?;
    if parent.as_deref() == Some(&id) {
        return Err(Error::Protocol("self-parent commit".into()));
    }
    Ok(NativeCommit { id, parent })
}
fn validate_cursor(cursor: &str) -> Result<()> {
    // URL-safe unpadded Base64 of milliseconds|uuid; decode without dependencies.
    if cursor.is_empty() || cursor.len() > 256 {
        return Err(Error::Protocol("invalid cursor".into()));
    }
    let mut bits = 0u32;
    let mut count = 0;
    let mut out = vec![];
    for byte in cursor.bytes() {
        let digit = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'-' => 62,
            b'_' => 63,
            _ => return Err(Error::Protocol("invalid cursor alphabet".into())),
        };
        bits = (bits << 6) | u32::from(digit);
        count += 6;
        if count >= 8 {
            count -= 8;
            out.push((bits >> count) as u8);
            bits &= (1 << count) - 1;
        }
    }
    if count >= 6 || bits != 0 {
        return Err(Error::Protocol("invalid cursor encoding".into()));
    }
    let text =
        String::from_utf8(out).map_err(|_| Error::Protocol("invalid cursor bytes".into()))?;
    let (time, id) = text
        .split_once('|')
        .ok_or_else(|| Error::Protocol("invalid cursor shape".into()))?;
    if time.parse::<u64>().is_err()
        || id.len() != 36
        || id.bytes().enumerate().any(|(i, c)| {
            if [8, 13, 18, 23].contains(&i) {
                c != b'-'
            } else {
                !c.is_ascii_hexdigit()
            }
        })
    {
        return Err(Error::Protocol("invalid cursor payload".into()));
    }
    Ok(())
}
