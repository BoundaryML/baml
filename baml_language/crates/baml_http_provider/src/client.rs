//! HTTP requests, on reqwest.

use baml_http_types::{BoxFuture, Error, Redirects, Request, Response};
use futures::StreamExt;

/// A reqwest error as a transport [`Error`]: a deadline is a timeout, the rest
/// is I/O.
fn transport_error(e: &reqwest::Error) -> Error {
    if e.is_timeout() {
        Error::timeout(e.to_string())
    } else {
        Error::io(e.to_string())
    }
}

pub(crate) fn send(request: Request) -> BoxFuture<Result<Response, Error>> {
    Box::pin(async move {
        crate::ensure_crypto_provider()?;
        let method = reqwest::Method::from_bytes(request.method.as_bytes()).map_err(|e| {
            Error::invalid_request(format!("Invalid HTTP method '{}': {e}", request.method))
        })?;

        let mut client = reqwest::Client::builder();
        if request.redirects == Redirects::None {
            client = client.redirect(reqwest::redirect::Policy::none());
        }
        if !request.use_env_proxy {
            client = client.no_proxy();
        }
        if let Some(timeout) = request.connect_timeout {
            client = client.connect_timeout(timeout);
        }
        let client = client.build().map_err(|e| transport_error(&e))?;

        let mut builder = client.request(method, &request.url);
        for (name, value) in &request.headers {
            builder = builder.header(name.as_str(), value.as_str());
        }
        if !request.body.is_empty() {
            builder = builder.body(request.body);
        }
        if let Some(timeout) = request.timeout {
            builder = builder.timeout(timeout);
        }

        let response = builder.send().await.map_err(|e| transport_error(&e))?;
        let status = response.status().as_u16();
        let url = response.url().to_string();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response
            .bytes_stream()
            .map(|chunk| chunk.map_err(|e| transport_error(&e)));
        Ok(Response {
            status,
            headers,
            url,
            body: Box::pin(body),
        })
    })
}
