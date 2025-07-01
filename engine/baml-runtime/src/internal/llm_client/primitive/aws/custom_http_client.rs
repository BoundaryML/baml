// See https://github.com/awslabs/aws-sdk-rust/issues/169
use std::time::Duration;

use aws_smithy_runtime_api::{
    client::{
        http::{
            HttpClient, HttpConnector, HttpConnectorFuture, HttpConnectorSettings,
            SharedHttpConnector,
        },
        result::ConnectorError,
        runtime_components::RuntimeComponents,
    },
    http::Request,
};
use aws_smithy_types::body::SdkBody;
// --- WASM specific imports ---
#[cfg(target_arch = "wasm32")]
use {futures::channel::oneshot, wasm_bindgen_futures::spawn_local};

use crate::request::create_client;

/// Returns a wrapper around the global reqwest client.
/// [HttpClient].
#[cfg(not(target_arch = "wasm32"))] // Keep function non-WASM for now
pub fn client() -> anyhow::Result<Client> {
    let client = crate::request::create_client()
        .map_err(|e| anyhow::anyhow!("failed to create base http client: {}", e))?;
    Ok(Client::new(client.clone()))
}

#[cfg(target_arch = "wasm32")] // Define WASM client function
pub fn client() -> anyhow::Result<Client> {
    let client = crate::request::create_client()
        .map_err(|e| anyhow::anyhow!("failed to create base http client for WASM: {}", e))?;
    Ok(Client::new(client.clone()))
}

/// A wrapper around [reqwest::Client] that implements [HttpClient].
///
/// This is required to support using proxy servers with the AWS SDK.
#[derive(Debug, Clone)]
pub struct Client {
    inner: reqwest::Client,
}

impl Client {
    pub fn new(client: reqwest::Client) -> Self {
        Self { inner: client }
    }
}

#[derive(Debug)]
struct CallError {
    kind: CallErrorKind,
    message: &'static str,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl CallError {
    fn user(message: &'static str) -> Self {
        Self {
            kind: CallErrorKind::User,
            message,
            source: None,
        }
    }

    fn user_with_source<E>(message: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::User,
            message,
            source: Some(Box::new(source)),
        }
    }

    fn timeout<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Timeout,
            message: "request timed out",
            source: Some(Box::new(source)),
        }
    }

    fn io<E>(source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Io,
            message: "an i/o error occurred",
            source: Some(Box::new(source)),
        }
    }

    fn other<E>(message: &'static str, source: E) -> Self
    where
        E: std::error::Error + Send + Sync + 'static,
    {
        Self {
            kind: CallErrorKind::Other,
            message,
            source: Some(Box::new(source)),
        }
    }
}

impl std::error::Error for CallError {}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)?;
        if let Some(err) = self.source.as_ref() {
            write!(f, ": {err}")?;
        }
        Ok(())
    }
}

impl From<CallError> for ConnectorError {
    fn from(value: CallError) -> Self {
        match &value.kind {
            CallErrorKind::User => Self::user(Box::new(value)),
            CallErrorKind::Timeout => Self::timeout(Box::new(value)),
            CallErrorKind::Io => Self::io(Box::new(value)),
            CallErrorKind::Other => Self::other(Box::new(value), None),
        }
    }
}

impl From<reqwest::Error> for CallError {
    fn from(err: reqwest::Error) -> Self {
        if err.is_timeout() {
            return CallError::timeout(err);
        }

        // Conditionally check for connect error only on non-WASM targets.
        #[cfg(not(target_arch = "wasm32"))]
        {
            if err.is_connect() {
                return CallError::io(err);
            }
        }

        // If it's not a timeout or (on non-WASM) a connect error, treat as other.
        CallError::other("an unknown error occurred", err)
    }
}

#[derive(Debug, Clone)]
enum CallErrorKind {
    User,
    Timeout,
    Io,
    Other,
}

#[derive(Debug)]
struct ReqwestConnector {
    client: reqwest::Client,
    timeout: Option<Duration>,
}

// See https://github.com/aws/amazon-q-developer-cli/pull/1199
impl HttpConnector for ReqwestConnector {
    fn call(&self, request: Request) -> HttpConnectorFuture {
        let client = self.client.clone();
        let timeout = self.timeout;

        #[cfg(not(target_arch = "wasm32"))]
        let future = async move {
            // Non-WASM logic (direct send)
            let mut req_builder = client.request(
                reqwest::Method::from_bytes(request.method().as_bytes()).map_err(|err| {
                    CallError::user_with_source("failed to create method name", err)
                })?,
                request.uri().to_owned(),
            );
            let parts = request.into_parts();
            for (name, value) in parts.headers.iter() {
                req_builder = req_builder.header(name, value.as_bytes());
            }
            let body_bytes = parts
                .body
                .bytes()
                .ok_or(CallError::user("streaming request body is not supported"))?
                .to_owned();
            req_builder = req_builder.body(body_bytes);

            if let Some(timeout) = timeout {
                req_builder = req_builder.timeout(timeout);
            }

            let reqwest_response = req_builder.send().await.map_err(CallError::from)?;

            let http_response = {
                let (parts, body) = http::Response::from(reqwest_response).into_parts();
                http::Response::from_parts(parts, SdkBody::from_body_1_x(body))
            };

            Ok(
                aws_smithy_runtime_api::http::Response::try_from(http_response).map_err(|err| {
                    CallError::other("failed to convert to a proper response", err)
                })?,
            )
        };

        #[cfg(target_arch = "wasm32")]
        let future = async move {
            // WASM logic (spawn_local)
            let (tx, rx) = oneshot::channel();

            spawn_local(async move {
                // Use a closure to handle errors
                let result = (async {
                    let mut req_builder = client.request(
                        reqwest::Method::from_bytes(request.method().as_bytes()).map_err(
                            |err| CallError::user_with_source("failed to create method name", err),
                        )?,
                        request.uri().to_owned(),
                    );
                    let parts = request.into_parts();
                    for (name, value) in parts.headers.iter() {
                        req_builder = req_builder.header(name, value.as_bytes());
                    }
                    let body_bytes = parts
                        .body
                        .bytes()
                        .ok_or(CallError::user("streaming request body is not supported"))?
                        .to_owned();
                    req_builder = req_builder.body(body_bytes);

                    let reqwest_response = req_builder.send().await.map_err(CallError::from)?;

                    // Use manual construction for WASM response conversion
                    let http_response = {
                        let status = reqwest_response.status();
                        let headers = reqwest_response.headers().clone();
                        let body_bytes = reqwest_response
                            .bytes()
                            .await
                            .map_err(|e| CallError::other("failed to read response body", e))?;

                        let mut response_builder = http::Response::builder().status(status);

                        for (name, value) in headers.iter() {
                            response_builder = response_builder.header(name, value);
                        }

                        response_builder
                            .body(SdkBody::from(body_bytes))
                            .map_err(|e| CallError::other("failed to build http::Response", e))?
                    };

                    aws_smithy_runtime_api::http::Response::try_from(http_response).map_err(|err| {
                        CallError::other("failed to convert to a proper response", err)
                    })
                })
                .await;

                // Convert the inner Result<_, CallError> to Result<_, ConnectorError>
                let final_result = result.map_err(ConnectorError::from);

                let _ = tx.send(final_result);
            });

            rx.await.map_err(|_| {
                ConnectorError::other(
                    Box::new(CallError::user("WASM future channel cancelled")),
                    None,
                )
            })?
        };

        HttpConnectorFuture::new(future)
    }
}

impl HttpClient for Client {
    fn http_connector(
        &self,
        settings: &HttpConnectorSettings,
        _components: &RuntimeComponents,
    ) -> SharedHttpConnector {
        let timeout = if cfg!(target_arch = "wasm32") {
            None // Timeout not directly supported via reqwest on wasm
        } else {
            settings.read_timeout()
        };
        let connector = ReqwestConnector {
            client: self.inner.clone(),
            timeout,
        };
        SharedHttpConnector::new(connector)
    }
}

// --- Non-WASM Implementation using Reqwest ---
#[cfg(not(target_arch = "wasm32"))]
mod reqwest_impl {
    use std::time::Duration;
}
