//! Function hot swapping in Boundary Cloud (BEP-087, stage 2): the publisher's
//! routes under `/v1/hotswaps`, and a process's under `/v1/processes`.
//!
//! BCS owns this contract (its `shared/api` crate generates the OpenAPI
//! document); these types mirror it for BAML's clients.
use std::{
    io::{BufRead, BufReader, Read},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    auth::Client,
    credentials::{RequestAuthorization, Target},
    error::{Error, Result, require},
};

/// A swap as BCS reports it to its publisher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Hotswap {
    pub id: String,
    /// `matching`, `matched`, `pending_approval`, `approved`, `installed`,
    /// `rejected`, `disabled` or `deleted`.
    pub status: String,
    pub revision: u64,
    pub baseline: String,
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub process_ids: Option<Vec<String>>,
    #[serde(default)]
    pub replaces: Option<String>,
    pub require_approval: bool,
    #[serde(default)]
    pub functions: Vec<HotswapFunctionSummary>,
    #[serde(default)]
    pub published_by: Option<String>,
    #[serde(default)]
    pub published_at: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    #[serde(default)]
    pub approved_at: Option<String>,
    #[serde(default)]
    pub renewed_at: Option<String>,
    pub processes: ProcessCounts,
    /// Why BCS refused the swap before delivering it, such as an overlap.
    #[serde(default)]
    pub rejection: Option<Rejection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotswapFunctionSummary {
    pub name: String,
    pub rule: String,
    /// The function as built and as swapped, when BCS returns one swap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub old_source: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub new_source: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCounts {
    pub matched: u64,
    pub skipped: u64,
    pub installed: u64,
    pub rejected: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rejection {
    pub reason: String,
    #[serde(default)]
    pub conflicts: Vec<Conflict>,
}

/// An active swap that already changes a function this one changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Conflict {
    pub function: String,
    pub hotswap_id: String,
    pub processes: u64,
}

/// `POST /v1/hotswaps`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateHotswap {
    pub baseline: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub process_ids: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replaces: Option<String>,
    pub require_approval: bool,
}

/// One stored status event of a swap, in order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotswapEvent {
    pub sequence: u64,
    pub at: String,
    pub status: String,
    /// The process an outcome is for; absent for the swap's own events.
    #[serde(default)]
    pub process: Option<String>,
    #[serde(default)]
    pub by: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
    #[serde(default)]
    pub baseline: Option<String>,
    #[serde(default)]
    pub processes: Option<u64>,
    #[serde(default)]
    pub skipped: Option<u64>,
    #[serde(default)]
    pub installed: Option<u64>,
    #[serde(default)]
    pub rejected: Option<u64>,
}

/// A changed file, by content, for `uploads:prepare`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceFile {
    pub path: String,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSource {
    pub sha256: String,
    /// `available` when BCS already has it, else `upload`, with where.
    pub disposition: String,
    #[serde(default)]
    pub upload_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedSources {
    pub files: Vec<PreparedSource>,
}

/// One changed function, as built at the baseline and as replaced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapFunction {
    /// As `hotswap_changes` names it, such as `support.Summarize`.
    pub name: String,
    /// The project-relative file it is declared in.
    pub file: String,
    /// The rule its mark names, for approval; the process runs its own.
    pub rule: String,
    pub old_source: String,
    pub new_source: String,
}

/// `POST /v1/hotswaps/{id}/payload`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotswapPayload {
    pub functions: Vec<SwapFunction>,
    /// The changed files uploaded, which BCS keeps under the swap.
    pub files: Vec<SourceFile>,
    /// Functions the swap changes through a changed client, beside those in
    /// `functions`, so no other swap may change them either.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub affects: Vec<String>,
}

/// An installed swap, as every request a process makes lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledSwap {
    pub id: String,
    pub revision: u64,
}

/// What a process receives for one swap: only the changed functions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Swap {
    pub id: String,
    pub revision: u64,
    pub baseline: String,
    #[serde(default)]
    pub replaces: Option<String>,
    #[serde(default)]
    pub published_by: Option<String>,
    #[serde(default)]
    pub approved_by: Option<String>,
    pub functions: Vec<SwapFunction>,
}

/// A notice from BCS on any response to a process. For a swap, `active`
/// both offers a revision the process lacks and renews one it has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Notification {
    #[serde(rename = "type")]
    pub kind: String,
    pub id: String,
    pub revision: u64,
    /// `active`, `disabled` or `deleted`.
    pub status: String,
}

/// `POST /v1/processes`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegisterProcess {
    pub process_id: String,
    #[serde(default)]
    pub commit: Option<String>,
    pub baml_version: String,
    pub host: String,
    pub installed: Vec<InstalledSwap>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    #[serde(default)]
    pub swaps: Vec<Swap>,
    #[serde(default)]
    pub notifications: Vec<Notification>,
    /// How long BCS holds an idle process's connection.
    #[serde(default)]
    pub hold_seconds: Option<u64>,
}

/// `POST /v1/processes/{id}/hotswaps/{hotswap}/status`: `installed`,
/// `rejected`, `removed` or `lapsed`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SwapOutcome {
    pub revision: u64,
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// A path segment BCS accepts as an ID.
fn checked_id(id: &str) -> Result<&str> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-');
    if valid {
        Ok(id)
    } else {
        Err(Error::Protocol("Invalid hot swap or process ID"))
    }
}

/// Whether `url` is on `endpoint`'s scheme, host and port, so a credential sent
/// to it goes nowhere else. A string prefix is not enough:
/// `https://api.example.com.evil.example` starts with `https://api.example.com`.
fn same_origin(url: &str, endpoint: &str) -> bool {
    match (url::Url::parse(url), url::Url::parse(endpoint)) {
        (Ok(url), Ok(endpoint)) => url.origin() == endpoint.origin(),
        _ => false,
    }
}

/// The target as query parameters, for `GET` routes.
fn target_query(target: Option<&Target>) -> Vec<(&'static str, String)> {
    let Some(target) = target else {
        return Vec::new();
    };
    [
        ("project", &target.project),
        ("environment", &target.environment),
        ("orgId", &target.org_id),
        ("projectId", &target.project_id),
        ("environmentId", &target.environment_id),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.clone().map(|value| (name, value)))
    .collect()
}

/// The body of an authorized `POST`: `fields` and the target.
fn with_target(authorization: &RequestAuthorization, fields: impl Serialize) -> serde_json::Value {
    let mut body = serde_json::to_value(fields).unwrap_or_else(|_| json!({}));
    if let serde_json::Value::Object(map) = &mut body {
        map.insert("target".into(), json!(authorization.target));
    }
    body
}

impl Client {
    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.endpoint.as_str())
    }

    fn url_with(&self, path: &str, query: &[(&'static str, String)]) -> String {
        let base = self.url(path);
        match url::Url::parse_with_params(&base, query) {
            Ok(url) => url.to_string(),
            Err(_) => base,
        }
    }

    fn post_hotswap<T: serde::de::DeserializeOwned>(
        &self,
        authorization: &RequestAuthorization,
        path: &str,
        fields: impl Serialize,
        timeout: Option<Duration>,
    ) -> Result<T> {
        let body = with_target(authorization, fields);
        let response = self.send_authorized(authorization, || {
            let request = self.http.post(self.url(path)).json(&body);
            match timeout {
                Some(timeout) => request.timeout(timeout),
                None => request,
            }
        })?;
        Self::read_json(response)
    }

    pub fn create_hotswap(
        &self,
        authorization: &RequestAuthorization,
        request: &CreateHotswap,
    ) -> Result<Hotswap> {
        self.post_hotswap(authorization, "/v1/hotswaps", request, None)
    }

    pub fn list_hotswaps(&self, authorization: &RequestAuthorization) -> Result<Vec<Hotswap>> {
        #[derive(Deserialize)]
        struct Listed {
            hotswaps: Vec<Hotswap>,
        }
        let query = target_query(authorization.target.as_ref());
        let response = self.send_authorized(authorization, || {
            self.http.get(self.url_with("/v1/hotswaps", &query))
        })?;
        Ok(Self::read_json::<Listed>(response)?.hotswaps)
    }

    pub fn hotswap(&self, authorization: &RequestAuthorization, id: &str) -> Result<Hotswap> {
        let path = format!("/v1/hotswaps/{}", checked_id(id)?);
        let query = target_query(authorization.target.as_ref());
        let response = self.send_authorized(authorization, || {
            self.http.get(self.url_with(&path, &query))
        })?;
        Self::read_json(response)
    }

    /// The swap's stored events after `after`, then, with `follow`, each new
    /// one as BCS stores it, until `consume` returns `false` or BCS ends the
    /// stream.
    pub fn hotswap_events(
        &self,
        authorization: &RequestAuthorization,
        id: &str,
        after: u64,
        follow: bool,
        mut consume: impl FnMut(HotswapEvent) -> Result<bool>,
    ) -> Result<()> {
        let path = format!("/v1/hotswaps/{}/events", checked_id(id)?);
        let mut query = target_query(authorization.target.as_ref());
        query.push(("after", after.to_string()));
        if follow {
            query.push(("follow", "true".to_string()));
        }
        let response = self.send_authorized(authorization, || {
            let request = self.http.get(self.url_with(&path, &query));
            // A followed stream stays open while the swap rolls out.
            if follow {
                request.timeout(Duration::from_secs(24 * 3600))
            } else {
                request
            }
        })?;
        let mut input = BufReader::new(response);
        loop {
            let mut line = Vec::new();
            let size = (&mut input)
                .take((1 << 20) + 1)
                .read_until(b'\n', &mut line)?;
            if size == 0 {
                return Ok(());
            }
            require(size <= 1 << 20, "Hot swap event exceeded its size limit")?;
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let event: HotswapEvent = serde_json::from_slice(&line)
                .map_err(|_| Error::Protocol("Invalid hot swap event"))?;
            if !consume(event)? {
                return Ok(());
            }
        }
    }

    pub fn prepare_hotswap_sources(
        &self,
        authorization: &RequestAuthorization,
        id: &str,
        files: &[SourceFile],
    ) -> Result<PreparedSources> {
        let path = format!("/v1/hotswaps/{}/uploads:prepare", checked_id(id)?);
        self.post_hotswap(authorization, &path, json!({ "files": files }), None)
    }

    /// PUT one changed file where `uploads:prepare` said to.
    pub fn upload_hotswap_source(
        &self,
        authorization: &RequestAuthorization,
        url: &str,
        bytes: Vec<u8>,
    ) -> Result<()> {
        // A path is under the endpoint; a full URL must be.
        let url = if url.starts_with('/') {
            self.url(url)
        } else {
            url.to_string()
        };
        require(
            same_origin(&url, self.endpoint.as_str()),
            "Boundary named an upload outside its endpoint",
        )?;
        self.send_authorized(authorization, || {
            self.http
                .put(&url)
                .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(bytes.clone())
        })?;
        Ok(())
    }

    pub fn submit_hotswap_payload(
        &self,
        authorization: &RequestAuthorization,
        id: &str,
        payload: &HotswapPayload,
    ) -> Result<Hotswap> {
        let path = format!("/v1/hotswaps/{}/payload", checked_id(id)?);
        self.post_hotswap(authorization, &path, payload, None)
    }

    /// `approve`, `disable` or `enable`.
    pub fn update_hotswap(
        &self,
        authorization: &RequestAuthorization,
        id: &str,
        action: &str,
    ) -> Result<Hotswap> {
        let path = format!("/v1/hotswaps/{}/{action}", checked_id(id)?);
        self.post_hotswap(authorization, &path, json!({}), None)
    }

    pub fn delete_hotswap(
        &self,
        authorization: &RequestAuthorization,
        id: &str,
    ) -> Result<Hotswap> {
        let path = format!("/v1/hotswaps/{}", checked_id(id)?);
        let body = with_target(authorization, json!({}));
        let response = self.send_authorized(authorization, || {
            self.http.delete(self.url(&path)).json(&body)
        })?;
        Self::read_json(response)
    }

    // ----- a process's routes -----

    pub fn register_process(
        &self,
        authorization: &RequestAuthorization,
        request: &RegisterProcess,
    ) -> Result<Delivery> {
        self.post_hotswap(authorization, "/v1/processes", request, None)
    }

    pub fn fetch_swaps(
        &self,
        authorization: &RequestAuthorization,
        process_id: &str,
        installed: &[InstalledSwap],
        ids: &[String],
    ) -> Result<Delivery> {
        let path = format!("/v1/processes/{}/hotswaps/fetch", checked_id(process_id)?);
        self.post_hotswap(
            authorization,
            &path,
            json!({ "installed": installed, "ids": ids }),
            None,
        )
    }

    pub fn report_swap(
        &self,
        authorization: &RequestAuthorization,
        process_id: &str,
        hotswap_id: &str,
        installed: &[InstalledSwap],
        outcome: &SwapOutcome,
    ) -> Result<Delivery> {
        let path = format!(
            "/v1/processes/{}/hotswaps/{}/status",
            checked_id(process_id)?,
            checked_id(hotswap_id)?
        );
        let mut body = serde_json::to_value(outcome).unwrap_or_else(|_| json!({}));
        body["installed"] = json!(installed);
        self.post_hotswap(authorization, &path, body, None)
    }

    /// Say the process is stopping, so no rollout waits on it.
    pub fn process_stopped(
        &self,
        authorization: &RequestAuthorization,
        process_id: &str,
    ) -> Result<()> {
        let path = format!("/v1/processes/{}/stop", checked_id(process_id)?);
        let body = with_target(authorization, json!({}));
        self.send_authorized(authorization, || {
            self.http
                .post(self.url(&path))
                .timeout(Duration::from_secs(2))
                .json(&body)
        })?;
        Ok(())
    }

    /// Hold the connection until BCS has a notification for the process, or
    /// for about `hold`.
    pub fn wait_for_notifications(
        &self,
        authorization: &RequestAuthorization,
        process_id: &str,
        installed: &[InstalledSwap],
        hold: Duration,
    ) -> Result<Delivery> {
        let path = format!(
            "/v1/processes/{}/notifications/wait",
            checked_id(process_id)?
        );
        self.post_hotswap(
            authorization,
            &path,
            json!({ "installed": installed }),
            Some(hold + Duration::from_secs(30)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::same_origin;

    #[test]
    fn an_upload_must_be_on_the_endpoints_origin() {
        let endpoint = "https://api.example.com";
        assert!(same_origin(
            "https://api.example.com/v1/hotswaps/x/sources/y",
            endpoint
        ));
        assert!(!same_origin(
            "https://api.example.com.evil.example/x",
            endpoint
        ));
        assert!(!same_origin("http://api.example.com/x", endpoint));
        assert!(!same_origin("https://api.example.com:8443/x", endpoint));
        assert!(!same_origin("not a url", endpoint));
    }
}
