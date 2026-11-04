//! Boundary alerts: SQL that runs on a schedule in an environment's data plane and posts to
//! Slack or a webhook when it returns rows. The control API keeps the catalogue (names,
//! schedules, versions, mutes, the log of runs); an alert's definition, with its SQL, goes only
//! to the environment's data plane, with a token the control API issues for that one upload.
//!
//! The control API takes a `baml auth login` session's access token as its bearer.
use std::io::Read as _;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use sha2::{Digest, Sha256};

use crate::{
    auth::{Endpoint, HttpFailure, Secret},
    error::{Error, Result, require},
};

/// Boundary's control API, where Studio signs in and alerts are kept.
pub const DEFAULT_CONTROL_API_URL: &str = "https://services.cloud.boundaryml.com";
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const MAX_RESPONSE_BYTES: u64 = 4 * 1024 * 1024;

/// When an alert runs: every `seconds`, counted from when it was saved, or at a local time on
/// some days in an IANA time zone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Schedule {
    Every {
        seconds: u32,
    },
    #[serde(rename_all = "camelCase")]
    At {
        time: String,
        time_zone: String,
        days: Vec<String>,
    },
}

/// What the data plane keeps of an alert: its SQL and where it posts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Definition {
    pub name: String,
    pub sql: String,
    pub destinations: Vec<String>,
    pub message_rows: u32,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub id: String,
    pub name: String,
    pub version: u64,
    pub version_id: String,
    pub schedule: Schedule,
    #[serde(default)]
    pub next_run_at: Option<u64>,
    #[serde(default)]
    pub muted_until: Option<u64>,
    pub updated_by: String,
    /// Unix milliseconds.
    pub updated_at: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    pub name: String,
    /// `slack` or `webhook`.
    pub kind: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Delivery {
    pub destination_id: String,
    #[serde(default)]
    pub destination_name: Option<String>,
    #[serde(default)]
    pub deleted: bool,
    pub delivered: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: u64,
    pub alert_id: String,
    pub alert_name: String,
    pub run_at: u64,
    /// `posted`, `query_failed`, `undeliverable` or `not_run`.
    pub outcome: String,
    pub deliveries: Vec<Delivery>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPage {
    pub runs: Vec<Run>,
    #[serde(default)]
    pub next_before: Option<u64>,
}

/// An environment found by its project's handle and its name.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub organization_id: String,
    pub project_id: String,
    pub environment_id: String,
    pub environment_name: String,
}

#[derive(Deserialize)]
struct AlertList {
    alerts: Vec<Alert>,
}

#[derive(Deserialize)]
struct Destinations {
    destinations: Vec<Destination>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    version_id: String,
    data_plane_url: String,
    upload_token: Secret,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct DefinitionAccess {
    data_plane_url: String,
    version_id: String,
    access_token: Secret,
}

/// A client of the control API's alert routes, as one signed-in user.
pub struct Client {
    control: Endpoint,
    access_token: Secret,
    http: reqwest::blocking::Client,
}

/// The control API's base URL: `BOUNDARY_CONTROL_API_URL`, else `configured`, else Boundary's.
pub fn control_endpoint(configured: Option<&str>) -> Result<Endpoint> {
    let override_url = baml_env::string_var("BOUNDARY_CONTROL_API_URL")?;
    Endpoint::parse(
        override_url
            .as_deref()
            .or(configured)
            .unwrap_or(DEFAULT_CONTROL_API_URL),
    )
}

impl Client {
    pub fn new(control: Endpoint, access_token: Secret) -> Result<Self> {
        Ok(Self {
            control,
            access_token,
            http: reqwest::blocking::Client::builder()
                .timeout(REQUEST_TIMEOUT)
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(Error::transport)?,
        })
    }

    /// The environment `project` (`org_handle/project_name`) names `environment`; the caller's
    /// personal environment of the project without one.
    pub fn environment(&self, project: &str, environment: Option<&str>) -> Result<Environment> {
        let mut query = vec![("project", project)];
        if let Some(environment) = environment {
            query.push(("environment", environment));
        }
        self.get("/v1/environment-lookup", &query)
    }

    pub fn list(&self, environment: &str) -> Result<Vec<Alert>> {
        let listed: AlertList = self.get(&format!("/v1/environments/{environment}/alerts"), &[])?;
        Ok(listed.alerts)
    }

    pub fn destinations(&self, environment: &str) -> Result<Vec<Destination>> {
        let listed: Destinations = self.get(
            &format!("/v1/environments/{environment}/alert-destinations"),
            &[],
        )?;
        Ok(listed.destinations)
    }

    pub fn runs(&self, environment: &str, limit: u32, before: Option<u64>) -> Result<RunPage> {
        let limit = limit.to_string();
        let before = before.map(|before| before.to_string());
        let mut query = vec![("limit", limit.as_str())];
        if let Some(before) = &before {
            query.push(("before", before.as_str()));
        }
        self.get(
            &format!("/v1/environments/{environment}/alert-runs"),
            &query,
        )
    }

    /// Creates an alert in `environment`: the control API prepares it, the data plane stores
    /// the definition and publishes it.
    pub fn create(
        &self,
        environment: &str,
        definition: &Definition,
        schedule: &Schedule,
        run_now: bool,
    ) -> Result<Alert> {
        self.save(
            &format!("/v1/environments/{environment}/alerts"),
            0,
            definition,
            schedule,
            run_now,
        )
    }

    /// Saves the alert's next version, based on `version`, the one it is at.
    pub fn update(
        &self,
        alert: &str,
        version: u64,
        definition: &Definition,
        schedule: &Schedule,
        run_now: bool,
    ) -> Result<Alert> {
        self.save(
            &format!("/v1/alerts/{alert}/versions"),
            version,
            definition,
            schedule,
            run_now,
        )
    }

    /// The alert's current definition, read from its data plane.
    pub fn definition(&self, alert: &str) -> Result<Definition> {
        let access: DefinitionAccess =
            self.get(&format!("/v1/alerts/{alert}/definition-access"), &[])?;
        let plane = Endpoint::parse(&access.data_plane_url)?;
        let response = self
            .http
            .get(format!(
                "{}/v1/alert-versions/{}/definition",
                plane.as_str(),
                access.version_id
            ))
            .bearer_auth(access.access_token.expose())
            .send()
            .map_err(Error::transport)?;
        self.answer(response, &[access.access_token.expose()])
    }

    /// Mutes the alert until `until`, Unix seconds, or ends its mute.
    pub fn mute(&self, alert: &str, until: Option<u64>) -> Result<Alert> {
        self.post(
            &format!("/v1/alerts/{alert}/mute"),
            &serde_json::json!({"until": until}),
        )
    }

    pub fn delete(&self, alert: &str) -> Result<()> {
        let response = self
            .http
            .post(format!(
                "{}/v1/alerts/{alert}/delete",
                self.control.as_str()
            ))
            .bearer_auth(self.access_token.expose())
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[self.access_token.expose()]).into());
        }
        Ok(())
    }

    fn save(
        &self,
        path: &str,
        expected_version: u64,
        definition: &Definition,
        schedule: &Schedule,
        run_now: bool,
    ) -> Result<Alert> {
        // The data plane receives exactly the bytes digested here.
        let bytes = serde_json::to_vec(definition)?;
        let pending: Pending = self.post(
            path,
            &serde_json::json!({
                "name": definition.name,
                "schedule": schedule,
                "runNow": run_now,
                "expectedVersion": expected_version,
                "sha256": hex_digest(&bytes),
                "size": bytes.len(),
            }),
        )?;
        let plane = Endpoint::parse(&pending.data_plane_url)?;
        let response = self
            .http
            .put(format!(
                "{}/v1/alert-versions/{}/definition",
                plane.as_str(),
                pending.version_id
            ))
            .bearer_auth(pending.upload_token.expose())
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(bytes)
            .send()
            .map_err(Error::transport)?;
        self.answer(response, &[pending.upload_token.expose()])
    }

    fn get<T: DeserializeOwned>(&self, path: &str, query: &[(&str, &str)]) -> Result<T> {
        let url = url::Url::parse_with_params(&format!("{}{path}", self.control.as_str()), query)?;
        let response = self
            .http
            .get(url)
            .bearer_auth(self.access_token.expose())
            .send()
            .map_err(Error::transport)?;
        self.answer(response, &[])
    }

    fn post<T: DeserializeOwned>(&self, path: &str, body: &serde_json::Value) -> Result<T> {
        let response = self
            .http
            .post(format!("{}{path}", self.control.as_str()))
            .bearer_auth(self.access_token.expose())
            .json(body)
            .send()
            .map_err(Error::transport)?;
        self.answer(response, &[])
    }

    fn answer<T: DeserializeOwned>(
        &self,
        mut response: reqwest::blocking::Response,
        secrets: &[&str],
    ) -> Result<T> {
        if !response.status().is_success() {
            let mut secrets = secrets.to_vec();
            secrets.push(self.access_token.expose());
            return Err(HttpFailure::blocking(response, &secrets).into());
        }
        let mut bytes = Vec::new();
        response
            .by_ref()
            .take(MAX_RESPONSE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(Error::Read)?;
        require(
            bytes.len() as u64 <= MAX_RESPONSE_BYTES,
            "Boundary response exceeded the size limit",
        )?;
        serde_json::from_slice(&bytes).map_err(|_| Error::Protocol("Invalid Boundary response"))
    }
}

fn hex_digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
#[path = "alerts_tests.rs"]
mod tests;
