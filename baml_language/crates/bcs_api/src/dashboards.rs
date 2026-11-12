//! Native dashboard publication through the selected gateway and its scoped upload capability.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{
    Client, Endpoint, HttpFailure, Secret,
    credentials::Target,
    error::{Error, Result, require},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Publication {
    pub target: Target,
    pub key: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_version: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Prepare<'a> {
    #[serde(flatten)]
    publication: &'a Publication,
    sha256: String,
    size: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Upload {
    dashboard_id: String,
    version_id: String,
    data_plane_url: String,
    #[serde(rename = "uploadToken")]
    token: Secret,
    expires_at: u64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedDashboard {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub latest: PublishedVersion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedVersion {
    pub version: u64,
    pub sha256: String,
    pub size: u64,
}

impl Client {
    /// Prepare metadata with a saved login, then send the exact HTML using only the upload token.
    pub fn publish_dashboard(
        &self,
        session: &Secret,
        publication: &Publication,
        html: Vec<u8>,
    ) -> Result<PublishedDashboard> {
        require(
            session
                .expose()
                .starts_with(crate::credentials::SESSION_PREFIX),
            "Dashboard publication requires a saved login; run baml auth login",
        )?;
        require(!html.is_empty(), "Dashboard HTML is empty")?;
        let request = Prepare {
            publication,
            sha256: hex::encode(Sha256::digest(&html)),
            size: html.len() as u64,
        };
        let response = self
            .http
            .post(format!("{}/v1/dashboards/prepare", self.endpoint.as_str()))
            .bearer_auth(session.expose())
            .json(&request)
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[session.expose()]).into());
        }
        let upload: Upload = Self::read_json(response)?;
        require(
            version_id(&upload.version_id),
            "Boundary returned an invalid dashboard version ID",
        )?;
        let storage = Endpoint::parse(&upload.data_plane_url)?;
        require(
            !self.endpoint.as_str().starts_with("https:") || storage.as_str().starts_with("https:"),
            "Boundary dashboard upload cannot downgrade HTTPS",
        )?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |time| time.as_secs());
        require(
            upload.expires_at > now,
            "Dashboard upload expired; prepare the upload again",
        )?;
        let response = self
            .http
            .put(format!(
                "{}/v1/dashboard-versions/{}/bundle",
                storage.as_str(),
                upload.version_id
            ))
            .bearer_auth(upload.token.expose())
            .header(reqwest::header::CONTENT_TYPE, "text/html")
            .body(html)
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(
                response,
                &[session.expose(), upload.token.expose()],
            )
            .into());
        }
        let published: PublishedDashboard = Self::read_json(response)?;
        require(
            published.id == upload.dashboard_id
                && published.key == publication.key
                && published.latest.sha256 == request.sha256
                && published.latest.size == request.size,
            "Boundary published a different dashboard bundle",
        )?;
        if let Some(expected) = publication.expected_version {
            require(
                expected.checked_add(1) == Some(published.latest.version),
                "Boundary published an unexpected dashboard version",
            )?;
        } else {
            require(
                published.latest.version == 1,
                "Boundary did not publish a new dashboard",
            )?;
        }
        Ok(published)
    }
}

fn version_id(value: &str) -> bool {
    value.strip_prefix("dsv_").is_some_and(|id| {
        id.len() == 32
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}

#[cfg(test)]
mod tests;
