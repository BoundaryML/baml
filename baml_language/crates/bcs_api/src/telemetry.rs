//! Telemetry HTTP operations with process-local credential renewal. Callers own response bounds,
//! upload-plan validation, retries, scheduling and cancellation.
use std::time::Duration;

use bytes::Bytes;
use reqwest::{Body, Client, Response, Url, header};

use crate::{
    credentials::{RenewalError, RequestAuthorization, Targeted},
    wire::Liveness,
};

/// Send an already bounded JSON prepare body with its stable idempotency key.
pub async fn prepare(
    client: &Client,
    endpoint: &Url,
    bearer_token: Option<&str>,
    authorization: Option<&RequestAuthorization>,
    idempotency_key: &str,
    body: Bytes,
) -> Result<Response, ControlError> {
    send_control(endpoint, bearer_token, authorization, || {
        client
            .post(endpoint.clone())
            .header(header::CONTENT_TYPE, "application/json")
            .header("idempotency-key", idempotency_key)
            .body(body.clone())
    })
    .await
}

/// Report process liveness using the same authorization as upload preparation.
pub async fn heartbeat(
    client: &Client,
    endpoint: &Url,
    bearer_token: Option<&str>,
    authorization: Option<&RequestAuthorization>,
    body: &Liveness,
) -> Result<Response, ControlError> {
    let targeted = Targeted {
        build_id: authorization.and_then(|auth| auth.authentication.build_id()),
        target: authorization.and_then(|auth| auth.target.as_ref()),
        payload: body,
    };
    send_control(endpoint, bearer_token, authorization, || {
        client.post(endpoint.clone()).json(&targeted)
    })
    .await
}

/// A cached grant rejected before admission can be retried once with its original session.
/// Uploads use their separate signed capabilities and never enter this renewal path.
async fn send_control(
    endpoint: &Url,
    bearer_token: Option<&str>,
    authorization: Option<&RequestAuthorization>,
    request: impl Fn() -> reqwest::RequestBuilder,
) -> Result<Response, ControlError> {
    if authorization.is_some_and(|auth| !auth.authentication.accepts_url(endpoint)) {
        return Err(ControlError::EndpointMismatch);
    }
    for attempt in 0..2 {
        let sent = match authorization {
            Some(auth) => Some(
                auth.authentication
                    .acquire_async()
                    .await
                    .map_err(ControlError::Renewal)?,
            ),
            None => None,
        };
        let mut request = request();
        if let Some(token) = sent
            .as_ref()
            .map(|attempt| attempt.token().expose())
            .or(bearer_token)
        {
            request = request.bearer_auth(token);
        }
        let response = request.send().await?;
        let retry = if let (Some(auth), Some(sent)) = (authorization, sent.as_ref()) {
            attempt == 0
                && response.status() == reqwest::StatusCode::UNAUTHORIZED
                && auth.authentication.rejected(sent.token())
        } else {
            false
        };
        if retry {
            if let Some(sent) = sent {
                sent.complete(response.status(), response.headers());
            }
            continue;
        }
        if !response.status().is_success() {
            let secrets: Vec<_> = sent
                .as_ref()
                .map(|attempt| attempt.token().expose())
                .into_iter()
                .chain(bearer_token)
                .chain(authorization.map(|auth| auth.authentication.original_credential()))
                .collect();
            let failure = crate::HttpFailure::asynchronous(response, &secrets).await;
            if let Some(sent) = sent {
                sent.complete_failure(failure.clone());
            }
            return Err(ControlError::Api(failure));
        }
        if let Some(sent) = sent {
            sent.complete(response.status(), response.headers());
        }
        return Ok(response);
    }
    unreachable!("the second attempt returns its response")
}

/// PUT a protobuf envelope to a validated upload-plan URL. The URL supplies
/// upload authorization; the login or API-key credential belongs on control calls.
pub async fn upload(
    client: &Client,
    endpoint: &str,
    mut headers: header::HeaderMap,
    body: Body,
    body_length: usize,
    timeout: Duration,
) -> reqwest::Result<Response> {
    headers
        .entry(header::CONTENT_TYPE)
        .or_insert(header::HeaderValue::from_static("application/x-protobuf"));
    headers.insert(
        header::CONTENT_LENGTH,
        header::HeaderValue::from(body_length as u64),
    );
    client
        .put(endpoint)
        .timeout(timeout)
        .headers(headers)
        .body(body)
        .send()
        .await
}

#[derive(Debug)]
pub enum ControlError {
    EndpointMismatch,
    Renewal(RenewalError),
    Http(reqwest::Error),
    Api(crate::HttpFailure),
}
impl ControlError {
    /// Preserve a shared renewal's refusal so delivery/heartbeat can stop on revoked access.
    pub fn status(&self) -> Option<reqwest::StatusCode> {
        match self {
            Self::Api(failure) | Self::Renewal(RenewalError::Rejected(failure)) => {
                Some(failure.status)
            }
            Self::Http(error) => error.status(),
            Self::EndpointMismatch | Self::Renewal(_) => None,
        }
    }
    pub fn http_failure(&self) -> Option<&crate::HttpFailure> {
        match self {
            Self::Api(failure) | Self::Renewal(RenewalError::Rejected(failure)) => Some(failure),
            _ => None,
        }
    }
}
impl From<reqwest::Error> for ControlError {
    fn from(error: reqwest::Error) -> Self {
        Self::Http(error)
    }
}
impl std::fmt::Display for ControlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Api(failure) => failure.fmt(f),
            Self::Renewal(error) => error.fmt(f),
            _ => f.write_str("Boundary telemetry control request failed"),
        }
    }
}
impl std::error::Error for ControlError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Http(error) => Some(error),
            Self::Renewal(error) => Some(error),
            Self::Api(failure) => Some(failure),
            Self::EndpointMismatch => None,
        }
    }
}
