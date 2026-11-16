//! Read-only API-key identity and permission introspection.
use serde::Deserialize;

use crate::{
    Client, Secret,
    auth::HttpFailure,
    error::{Error, Result, require},
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiedApiKey {
    pub id: String,
    pub name: String,
    pub api_key_type: ApiKeyType,
    pub organization: ApiKeyOrganization,
    pub project: ApiKeyProject,
    pub permissions: ApiKeyPermissions,
    pub ingest_environments: Vec<ApiKeyEnvironment>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ApiKeyType {
    Public,
    Secret,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiKeyOrganization {
    pub id: String,
    pub slug: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ApiKeyProject {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ApiKeyPermissions {
    pub ingest: Vec<PermissionTarget>,
    pub query: Vec<PermissionTarget>,
    pub personal_environments: PersonalEnvironments,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PersonalEnvironments {
    CreatorOnly,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "lowercase",
    rename_all_fields = "camelCase"
)]
pub enum PermissionTarget {
    Organization { org_id: String },
    Project { project_id: String },
    Environment { environment_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiKeyEnvironment {
    pub id: String,
    pub name: String,
    pub project_id: String,
    pub organization_id: String,
}

impl Client {
    /// Inspect only this credential; no destination, query, or ingestion is requested.
    pub fn inspect_api_key(&self, key: &Secret) -> Result<VerifiedApiKey> {
        let response = self
            .http
            .post(format!("{}/v1/auth/verify", self.endpoint.as_str()))
            .bearer_auth(key.expose())
            .json(&serde_json::json!({"fullResponse": true}))
            .send()
            .map_err(Error::transport)?;
        if !response.status().is_success() {
            return Err(HttpFailure::blocking(response, &[key.expose()]).into());
        }
        require(
            response.status() == reqwest::StatusCode::OK,
            "Boundary API-key verification requires the 200 JSON introspection response",
        )?;
        Self::read_json(response)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{header, method, path},
    };

    use super::*;
    use crate::Endpoint;

    #[tokio::test]
    async fn introspection_accepts_public_write_only_and_empty_policy_keys() {
        let server = MockServer::start().await;
        for (key_type, ingest, query, slug) in [
            (
                "public",
                json!([{"kind":"environment","environmentId":"production"}]),
                json!([]),
                json!("acme"),
            ),
            ("secret", json!([]), json!([]), json!(null)),
            (
                "secret",
                json!([{"kind":"project","projectId":"app"}]),
                json!([{"kind":"organization","orgId":"org_acme"}]),
                json!("acme"),
            ),
        ] {
            server.reset().await;
            Mock::given(method("POST")).and(path("/v1/auth/verify"))
                .and(header("authorization", "Bearer bml_key"))
                .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                    "id":"key_1","name":"Telemetry","apiKeyType":key_type,
                    "organization":{"id":"org_acme","slug":slug},
                    "project":{"id":"app","name":"App"},
                    "ingestEnvironments":[],
                    "permissions":{"ingest":ingest,"query":query,"personalEnvironments":"creator_only"}
                }))).mount(&server).await;
            let endpoint = Endpoint::parse(&server.uri()).unwrap();
            let metadata = tokio::task::spawn_blocking(move || {
                Client::new(endpoint)
                    .unwrap()
                    .inspect_api_key(&Secret::new("bml_key".into()))
                    .unwrap()
            })
            .await
            .unwrap();
            assert_eq!(metadata.project.name, "App");
            assert_eq!(
                metadata.permissions.personal_environments,
                PersonalEnvironments::CreatorOnly
            );
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(
                    &server.received_requests().await.unwrap()[0].body
                )
                .unwrap(),
                json!({"fullResponse":true})
            );
        }
    }

    #[tokio::test]
    async fn introspection_rejects_empty_legacy_success_and_redacts_refused_credentials() {
        let server = MockServer::start().await;
        for (status, body) in [
            (204, json!(null)),
            (
                401,
                json!({"code":"UNAUTHORIZED","retryable":false,"message":"Refused bml_key"}),
            ),
        ] {
            server.reset().await;
            let response = if status == 204 {
                ResponseTemplate::new(status)
            } else {
                ResponseTemplate::new(status).set_body_json(body)
            };
            Mock::given(method("POST"))
                .and(path("/v1/auth/verify"))
                .respond_with(response)
                .mount(&server)
                .await;
            let endpoint = Endpoint::parse(&server.uri()).unwrap();
            let error = tokio::task::spawn_blocking(move || {
                Client::new(endpoint)
                    .unwrap()
                    .inspect_api_key(&Secret::new("bml_key".into()))
                    .unwrap_err()
            })
            .await
            .unwrap();
            assert!(!error.to_string().contains("bml_key"));
            if status == 204 {
                assert!(error.to_string().contains("200 JSON"));
            }
        }
    }
}
