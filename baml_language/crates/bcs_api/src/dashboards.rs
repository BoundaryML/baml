//! Native dashboard publication through the selected gateway and its scoped upload capability.
use std::{
    io::Read as _,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedDashboard {
    pub id: String,
    pub project_id: String,
    pub key: String,
    pub latest: PublishedVersion,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublishedVersion {
    pub id: String,
    pub name: String,
    pub created_by: String,
    pub created_at: u64,
    pub version: u64,
    pub sha256: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardLookup {
    pub target: Target,
    pub key: String,
}

pub struct RetrievedDashboard {
    pub dashboard: PublishedDashboard,
    pub html: Option<Vec<u8>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadAccess {
    data_plane_url: String,
    bundle_id: String,
    access_token: Secret,
    expires_at: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct NativeDashboard {
    dashboard: PublishedDashboard,
    bundle_access: ReadAccess,
}

impl Client {
    /// Prepare metadata with a saved login, then send the exact HTML using only the upload token.
    pub fn publish_dashboard(
        &self,
        session: &Secret,
        publication: &Publication,
        html: Vec<u8>,
    ) -> Result<PublishedDashboard> {
        require_session(session)?;
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
        let (storage, remaining) =
            self.dashboard_storage(&upload.data_plane_url, upload.expires_at)?;
        let timeout = upload_timeout(request.size, remaining);
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
            .timeout(timeout)
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
    fn dashboard_storage(&self, url: &str, expires_at: u64) -> Result<(Endpoint, Duration)> {
        let storage = Endpoint::parse(url)?;
        require(
            !self.endpoint.as_str().starts_with("https:") || storage.as_str().starts_with("https:"),
            "Boundary dashboard transfer cannot downgrade HTTPS",
        )?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| Error::Protocol("System clock is before the Unix epoch"))?;
        let remaining = Duration::from_secs(expires_at)
            .checked_sub(now)
            .filter(|time| !time.is_zero())
            .ok_or(Error::Protocol(
                "Dashboard capability expired; request it again",
            ))?;
        Ok((storage, remaining))
    }

    /// Get the current metadata, optionally downloading and verifying its exact HTML bundle.
    pub fn get_dashboard(
        &self,
        session: &Secret,
        lookup: &DashboardLookup,
        download: bool,
    ) -> Result<RetrievedDashboard> {
        require_session(session)?;
        let response = self
            .http
            .post(format!("{}/v1/dashboards/get", self.endpoint.as_str()))
            .bearer_auth(session.expose())
            .json(lookup)
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[session.expose()]).into());
        }
        let read: NativeDashboard = Self::read_json(response)?;
        require(
            read.dashboard.key == lookup.key,
            "Boundary returned a different dashboard",
        )?;
        let html = if download {
            let access = read.bundle_access;
            require(
                version_id(&access.bundle_id),
                "Boundary returned an invalid dashboard bundle ID",
            )?;
            let (storage, remaining) =
                self.dashboard_storage(&access.data_plane_url, access.expires_at)?;
            let response = self
                .http
                .get(format!(
                    "{}/v1/dashboard-versions/{}/bundle",
                    storage.as_str(),
                    access.bundle_id
                ))
                .bearer_auth(access.access_token.expose())
                .timeout(upload_timeout(read.dashboard.latest.size, remaining))
                .send()
                .map_err(Error::transport)?;
            if !response.status().is_success() {
                return Err(HttpFailure::blocking(
                    response,
                    &[session.expose(), access.access_token.expose()],
                )
                .into());
            }
            let mut html = Vec::new();
            response
                .take(read.dashboard.latest.size.saturating_add(1))
                .read_to_end(&mut html)
                .map_err(Error::Read)?;
            require(
                html.len() as u64 == read.dashboard.latest.size
                    && hex::encode(Sha256::digest(&html)) == read.dashboard.latest.sha256,
                "Boundary returned a different dashboard bundle",
            )?;
            Some(html)
        } else {
            None
        };
        Ok(RetrievedDashboard {
            dashboard: read.dashboard,
            html,
        })
    }

    /// Delete by stable key; the server checks dashboard-delete permission independently of read.
    pub fn delete_dashboard(&self, session: &Secret, lookup: &DashboardLookup) -> Result<()> {
        require_session(session)?;
        let response = self
            .http
            .post(format!("{}/v1/dashboards/delete", self.endpoint.as_str()))
            .bearer_auth(session.expose())
            .json(lookup)
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[session.expose()]).into());
        }
        Ok(())
    }
}

// Allow slow uplinks at 64 KiB/s plus response time, bounded by ten minutes and the capability.
fn upload_timeout(size: u64, remaining: Duration) -> Duration {
    const MIN_BYTES_PER_SECOND: u64 = 64 * 1024;
    let seconds = 30_u64.saturating_add(size.div_ceil(MIN_BYTES_PER_SECOND));
    Duration::from_secs(seconds.min(600)).min(remaining)
}

fn require_session(session: &Secret) -> Result<()> {
    require(
        session
            .expose()
            .starts_with(crate::credentials::SESSION_PREFIX),
        "Dashboard operations require a saved login; run baml auth login",
    )
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
