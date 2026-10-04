//! Build-time provisioning of public ingestion credentials. These credentials
//! are opaque server registrations, with independent revocation and no expiry.
use serde::{Deserialize, Serialize};

use crate::{
    Client, Secret,
    credentials::RequestAuthorization,
    error::{Result, require},
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BytecodeDigest {
    pub version: u32,
    pub algorithm: String,
    pub value: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Destination {
    pub org_id: String,
    pub project_id: String,
    pub environment_id: String,
}
#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BuildTarget<'a> {
    Handle {
        project: &'a str,
        environment: &'a str,
    },
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MintBuildToken<'a> {
    pub target: BuildTarget<'a>,
    pub bytecode_digest: &'a BytecodeDigest,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MintedBuildToken {
    pub build_id: String,
    pub token_id: String,
    pub token: Secret,
    pub product_id: String,
    pub destination: Destination,
    pub bytecode_digest: BytecodeDigest,
}

impl Client {
    /// A fresh idempotency key belongs to each explicit invocation. Retrying this
    /// call with that same key recovers the original credential rather than minting another.
    pub fn mint_build_token(
        &self,
        authorization: &RequestAuthorization,
        idempotency_key: uuid::Uuid,
        request: &MintBuildToken<'_>,
    ) -> Result<MintedBuildToken> {
        let response = self.send_authorized(authorization, || {
            self.http
                .post(format!("{}/v1/build-tokens", self.endpoint.as_str()))
                .header("Idempotency-Key", idempotency_key.to_string())
                .json(request)
        })?;
        let minted: MintedBuildToken = Self::read_json(response)?;
        let suffix = minted
            .token
            .expose()
            .strip_prefix(crate::credentials::PUBLIC_PREFIX);
        require(
            suffix.is_some_and(|value| {
                (value.len() == 43 || value.len() == 64)
                    && value
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
            }) && !minted.build_id.is_empty()
                && !minted.token_id.is_empty()
                && !minted.product_id.is_empty()
                && !minted.destination.org_id.is_empty()
                && !minted.destination.project_id.is_empty()
                && !minted.destination.environment_id.is_empty()
                && &minted.bytecode_digest == request.bytecode_digest,
            "Boundary returned an invalid build-token registration",
        )?;
        Ok(minted)
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_json, header, method, path},
    };

    use super::*;
    use crate::{
        Endpoint,
        credentials::{ACCESS_EXPIRY_HEADER, ACCESS_TOKEN_HEADER, Authentication},
    };

    fn registration(digest: &BytecodeDigest) -> serde_json::Value {
        serde_json::json!({"buildId":"build", "tokenId":"token", "token":format!("bdry_public_{}", "a".repeat(64)),
            "productId":"product", "destination":{"orgId":"org","projectId":"project","environmentId":"env"}, "bytecodeDigest":digest})
    }
    #[tokio::test]
    async fn mint_uses_operation_auth_and_preserves_the_invocations_idempotency_key() {
        let server = MockServer::start().await;
        let digest = BytecodeDigest {
            version: 1,
            algorithm: "sha256".into(),
            value: "d".repeat(64),
        };
        let key = uuid::Uuid::new_v4();
        let body = serde_json::json!({"target":{"kind":"handle","project":"publisher/app","environment":"staging"},"bytecodeDigest":digest});
        let expires = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 250;
        Mock::given(method("POST"))
            .and(path("/v1/build-tokens"))
            .and(header("Authorization", "Bearer bdry_session_admin"))
            .and(header("Idempotency-Key", key.to_string()))
            .and(body_json(body.clone()))
            .respond_with(
                ResponseTemplate::new(201)
                    .insert_header(ACCESS_TOKEN_HEADER, "bdry_access_admin")
                    .insert_header(ACCESS_EXPIRY_HEADER, expires.to_string())
                    .set_body_json(registration(&digest)),
            )
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/build-tokens"))
            .and(header("Authorization", "Bearer bdry_access_admin"))
            .and(header("Idempotency-Key", key.to_string()))
            .and(body_json(body))
            .respond_with(ResponseTemplate::new(201).set_body_json(registration(&digest)))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = Endpoint::parse(&server.uri()).unwrap();
        tokio::task::spawn_blocking(move || {
            let authorization = RequestAuthorization {
                authentication: Authentication::shared(
                    endpoint.as_str(),
                    Secret::new("bdry_session_admin".into()),
                ),
                target: None,
            };
            let client = Client::new(endpoint).unwrap();
            let request = MintBuildToken {
                target: BuildTarget::Handle {
                    project: "publisher/app",
                    environment: "staging",
                },
                bytecode_digest: &digest,
            };
            let first = client
                .mint_build_token(&authorization, key, &request)
                .unwrap();
            let retry = client
                .mint_build_token(&authorization, key, &request)
                .unwrap();
            assert_eq!(first.token.expose(), retry.token.expose());
            assert_eq!(first.build_id, retry.build_id);
            assert_eq!(format!("{:?}", first.token), "[REDACTED]");
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn mint_rejects_mismatched_or_nonpublic_registration_without_exposing_tokens() {
        let server = MockServer::start().await;
        let digest = BytecodeDigest {
            version: 1,
            algorithm: "sha256".into(),
            value: "d".repeat(64),
        };
        let mut invalid = registration(&digest);
        invalid["token"] = "bdry_secret_private-never-embed".into();
        Mock::given(method("POST"))
            .and(path("/v1/build-tokens"))
            .respond_with(ResponseTemplate::new(201).set_body_json(invalid))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = Endpoint::parse(&server.uri()).unwrap();
        tokio::task::spawn_blocking(move || {
            let authorization = RequestAuthorization {
                authentication: Authentication::shared(
                    endpoint.as_str(),
                    Secret::new("bdry_secret_builder".into()),
                ),
                target: None,
            };
            let client = Client::new(endpoint).unwrap();
            let error = client
                .mint_build_token(
                    &authorization,
                    uuid::Uuid::new_v4(),
                    &MintBuildToken {
                        target: BuildTarget::Handle {
                            project: "publisher/app",
                            environment: "staging",
                        },
                        bytecode_digest: &digest,
                    },
                )
                .unwrap_err();
            assert_eq!(
                error.to_string(),
                r#"Boundary returned an invalid build-token registration"#
            );
        })
        .await
        .unwrap();
    }
    #[tokio::test]
    async fn public_authorization_sends_build_context_on_heartbeat() {
        let server = MockServer::start().await;
        let token = format!("bdry_public_{}", "a".repeat(64));
        let authorization = RequestAuthorization {
            authentication: Authentication::for_build(
                &server.uri(),
                Secret::new(token.clone()),
                "build".into(),
            ),
            target: Some(crate::credentials::Target {
                environment_id: Some("env".into()),
                ..Default::default()
            }),
        };
        Mock::given(method("POST"))
            .and(path("/heartbeat"))
            .and(header("Authorization", format!("Bearer {token}")))
            .and(wiremock::matchers::body_partial_json(
                serde_json::json!({"buildId":"build","target":{"environmentId":"env"}}),
            ))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        crate::telemetry::heartbeat(
            &reqwest::Client::new(),
            &format!("{}/heartbeat", server.uri()).parse().unwrap(),
            None,
            Some(&authorization),
            &crate::wire::Liveness {
                producer_session_id: uuid::Uuid::new_v4(),
                liveness_sequence: 1,
                state: crate::wire::ProducerState::Running,
            },
        )
        .await
        .unwrap();
    }
}
