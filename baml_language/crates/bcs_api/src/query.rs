//! Native cloud query transport with incremental NDJSON results.
use std::{
    io::{BufRead, BufReader, Read},
    time::Duration,
};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use crate::credentials::Target;
use crate::{
    auth::{Client, HttpFailure},
    credentials::RequestAuthorization,
    error::{Error, Result, require},
};

#[derive(Debug, Clone, Copy, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Budgets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_rows: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wall_ms: Option<u64>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Record {
    Columns(Vec<Value>),
    Row(Vec<Value>),
    QueryOutcome(Value),
}

impl Client {
    pub fn query(
        &self,
        authorization: &RequestAuthorization,
        query_id: &str,
        sql: &str,
        budgets: Budgets,
        mut consume: impl FnMut(Record) -> Result<()>,
    ) -> Result<()> {
        let response = self.send_authorized(authorization, || {
            self.http.post(format!("{}/v1/query", self.endpoint.as_str()))
                .timeout(Duration::from_millis(budgets.max_wall_ms.unwrap_or(60_000)).saturating_add(Duration::from_secs(30)))
                .json(&json!({"queryId":query_id,"sql":sql,"budgets":budgets,"target":authorization.target}))
        })?;
        require(
            response
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.split(';').next() == Some("application/x-ndjson")),
            "Boundary returned an unexpected query format",
        )?;
        let mut input = BufReader::new(response);
        let mut width = None;
        let mut ended = false;
        loop {
            let mut bytes = Vec::new();
            let size = (&mut input)
                .take((64 << 20) + 1)
                .read_until(b'\n', &mut bytes)?;
            if size == 0 {
                break;
            }
            require(
                size <= 64 << 20,
                "Cloud query frame exceeded its size limit",
            )?;
            require(!ended, "Cloud query sent data after its outcome")?;
            let record: Record = serde_json::from_slice(&bytes)
                .map_err(|_| Error::Protocol("Invalid cloud query frame"))?;
            match &record {
                Record::Columns(columns) => {
                    require(width.is_none(), "Cloud query repeated its columns")?;
                    width = Some(columns.len());
                }
                Record::Row(row) => require(
                    width == Some(row.len()),
                    "Cloud query row does not match its columns",
                )?,
                Record::QueryOutcome(outcome) => {
                    require(
                        width.is_some() && outcome["queryId"] == query_id,
                        "Cloud query outcome does not match the request",
                    )?;
                    ended = true;
                }
            }
            consume(record)?;
        }
        require(ended, "Cloud query ended without an outcome")?;
        Ok(())
    }

    pub fn cancel_query(&self, authorization: &RequestAuthorization, query_id: &str) -> Result<()> {
        if !(query_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
            && !query_id.is_empty()
            && query_id.len() <= 128)
        {
            return Err(Error::QueryId);
        }
        self.send_authorized(authorization, || {
            self.http
                .post(format!(
                    "{}/v1/queries/{query_id}/cancel",
                    self.endpoint.as_str()
                ))
                .json(&json!({"target":authorization.target}))
        })?;
        Ok(())
    }
    /// The same credential lifecycle applies to query and cancellation; targets stay in bodies.
    fn send_authorized(
        &self,
        authorization: &RequestAuthorization,
        request: impl Fn() -> reqwest::blocking::RequestBuilder,
    ) -> Result<reqwest::blocking::Response> {
        if authorization.authentication.endpoint() != self.endpoint.as_str() {
            return Err(Error::EndpointMismatch);
        }
        for attempt in 0..2 {
            let sent = authorization.authentication.acquire_blocking()?;
            let response = request()
                .bearer_auth(sent.token().expose())
                .send()
                .map_err(Error::transport)?;
            let retry = attempt == 0
                && response.status() == reqwest::StatusCode::UNAUTHORIZED
                && authorization.authentication.rejected(sent.token());
            sent.complete(response.status(), response.headers());
            if retry {
                continue;
            }
            if !response.status().is_success() {
                return Err(HttpFailure(response.status()).into());
            }
            return Ok(response);
        }
        unreachable!("the second attempt returns its response")
    }
}

#[cfg(test)]
mod tests {
    use wiremock::{
        Mock, MockServer, ResponseTemplate,
        matchers::{body_partial_json, header, method, path},
    };

    use super::*;
    use crate::{
        Endpoint, Secret,
        credentials::{ACCESS_EXPIRY_HEADER, ACCESS_TOKEN_HEADER, Authentication},
    };

    #[tokio::test]
    async fn rejection_of_an_older_request_retries_with_concurrently_renewed_access() {
        let server = MockServer::start().await;
        let auth =
            Authentication::shared(&server.uri(), Secret::new("bdry_session_concurrent".into()));
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 250;
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(ACCESS_TOKEN_HEADER, "bdry_access_before".parse().unwrap());
        headers.insert(ACCESS_EXPIRY_HEADER, expiry.into());
        auth.observe(reqwest::StatusCode::OK, &headers);
        headers.insert(ACCESS_TOKEN_HEADER, "bdry_access_after".parse().unwrap());
        let concurrent = auth.clone();
        Mock::given(method("POST"))
            .and(path("/v1/queries/concurrent/cancel"))
            .and(header("authorization", "Bearer bdry_access_before"))
            .respond_with(move |_: &wiremock::Request| {
                concurrent.observe(reqwest::StatusCode::OK, &headers);
                ResponseTemplate::new(401)
            })
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/queries/concurrent/cancel"))
            .and(header("authorization", "Bearer bdry_access_after"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = Endpoint::parse(&server.uri()).unwrap();
        tokio::task::spawn_blocking(move || {
            let client = Client::new(endpoint).unwrap();
            client
                .cancel_query(
                    &RequestAuthorization {
                        authentication: auth,
                        target: None,
                    },
                    "concurrent",
                )
                .unwrap();
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_rejected_cached_token_renews_on_the_operation_and_does_not_retry_forbidden() {
        let server = MockServer::start().await;
        let auth = Authentication::shared(&server.uri(), Secret::new("bdry_session_renew".into()));
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert(ACCESS_TOKEN_HEADER, "bdry_access_old".parse().unwrap());
        headers.insert(
            ACCESS_EXPIRY_HEADER,
            (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
                + 250)
                .into(),
        );
        auth.observe(reqwest::StatusCode::OK, &headers);
        Mock::given(method("POST"))
            .and(path("/v1/queries/renew/cancel"))
            .and(header("authorization", "Bearer bdry_access_old"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/queries/renew/cancel"))
            .and(header("authorization", "Bearer bdry_session_renew"))
            .and(body_partial_json(
                json!({"target":{"environmentId":"prod"}}),
            ))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/queries/forbidden/cancel"))
            .respond_with(ResponseTemplate::new(403))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = Endpoint::parse(&server.uri()).unwrap();
        tokio::task::spawn_blocking(move || {
            let client = Client::new(endpoint).unwrap();
            let request = RequestAuthorization {
                authentication: auth,
                target: Some(Target {
                    environment_id: Some("prod".into()),
                    ..Default::default()
                }),
            };
            client.cancel_query(&request, "renew").unwrap();
            let error = client.cancel_query(&request, "forbidden").unwrap_err();
            assert_eq!(error.status().unwrap(), reqwest::StatusCode::FORBIDDEN);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn operation_bootstrap_reuses_one_token_for_query_and_telemetry() {
        let server = MockServer::start().await;
        let expiry = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 300;
        Mock::given(method("POST")).and(path("/v1/query")).and(header("authorization","Bearer bdry_session_original"))
            .and(body_partial_json(json!({"target":{"project":"acme/support","environment":"production"}})))
            .respond_with(ResponseTemplate::new(200)
                .insert_header(ACCESS_TOKEN_HEADER,"bdry_access_shared").insert_header(ACCESS_EXPIRY_HEADER,expiry.to_string())
                .set_body_raw("{\"columns\":[]}\n{\"queryOutcome\":{\"queryId\":\"first\",\"status\":\"complete\"}}\n", "application/x-ndjson"))
            .expect(1).mount(&server).await;
        Mock::given(method("POST")).and(path("/v1/query")).and(header("authorization","Bearer bdry_access_shared"))
            .and(body_partial_json(json!({"target":{"project":"beta/research","environment":"staging"}})))
            .respond_with(ResponseTemplate::new(200)
                .set_body_raw("{\"columns\":[]}\n{\"queryOutcome\":{\"queryId\":\"second\",\"status\":\"complete\"}}\n", "application/x-ndjson"))
            .expect(1).mount(&server).await;
        Mock::given(method("POST"))
            .and(path("/v1/recordings/r/uploads:prepare"))
            .and(header("authorization", "Bearer bdry_access_shared"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({})))
            .expect(1)
            .mount(&server)
            .await;
        let endpoint = Endpoint::parse(&server.uri()).unwrap();
        let auth = Authentication::shared(
            endpoint.as_str(),
            Secret::new("bdry_session_original".into()),
        );
        let clone = auth.clone();
        let client_endpoint = endpoint.clone();
        tokio::task::spawn_blocking(move || {
            let client = Client::new(client_endpoint).unwrap();
            let mut request = RequestAuthorization {
                authentication: clone,
                target: Some(Target {
                    project: Some("acme/support".into()),
                    environment: Some("production".into()),
                    ..Default::default()
                }),
            };
            client
                .query(
                    &request,
                    "first",
                    "SELECT 1",
                    Budgets::default(),
                    |_| Ok(()),
                )
                .unwrap();
            request.target = Some(Target {
                project: Some("beta/research".into()),
                environment: Some("staging".into()),
                ..Default::default()
            });
            client
                .query(&request, "second", "SELECT 1", Budgets::default(), |_| {
                    Ok(())
                })
                .unwrap();
        })
        .await
        .unwrap();
        let request = RequestAuthorization {
            authentication: auth,
            target: None,
        };
        let url = format!("{}/v1/recordings/r/uploads:prepare", server.uri())
            .parse()
            .unwrap();
        let http = reqwest::Client::new();
        let response = crate::telemetry::prepare(
            &http,
            &url,
            None,
            Some(&request),
            "r:1",
            bytes::Bytes::from_static(b"{}"),
        )
        .await
        .unwrap();
        assert!(response.status().is_success());
        // Mocks count exactly the operation calls; no explicit refresh or grant request exists.
    }
}
