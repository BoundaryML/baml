//! A builder-style [`RuntimeIo`] stub for the credential-resolution tests.
//!
//! Every method not configured behaves like an empty environment: no env vars,
//! no readable files, and an HTTP endpoint that 404s. That is deliberately the
//! shape of "a machine with no AWS/GCP credentials at all", so a test that
//! forgets to configure a source fails closed.

use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use indexmap::IndexMap;
use sys_types::{
    BexExternalValue,
    runtime_io::{HttpResponseHandle, RuntimeIo, RuntimeIoError},
};

/// Counts the IO a resolution actually performed, so a test can assert that
/// explicit credentials short-circuit the chain entirely.
#[derive(Default)]
pub(crate) struct Calls {
    env: AtomicUsize,
    fs: AtomicUsize,
    http: AtomicUsize,
}

impl Calls {
    pub(crate) fn total(&self) -> usize {
        self.env.load(Ordering::SeqCst)
            + self.fs.load(Ordering::SeqCst)
            + self.http.load(Ordering::SeqCst)
    }
}

pub(crate) struct StubIo {
    env: HashMap<String, String>,
    files: HashMap<String, String>,
    http_status: i64,
    http_body: String,
    calls: Arc<Calls>,
}

impl StubIo {
    pub(crate) fn new() -> Self {
        Self {
            env: HashMap::new(),
            files: HashMap::new(),
            http_status: 404,
            http_body: String::new(),
            calls: Arc::new(Calls::default()),
        }
    }

    pub(crate) fn env(mut self, key: &str, value: &str) -> Self {
        self.env.insert(key.to_string(), value.to_string());
        self
    }

    pub(crate) fn file(mut self, path: &str, contents: &str) -> Self {
        self.files.insert(path.to_string(), contents.to_string());
        self
    }

    pub(crate) fn http(mut self, status: i64, body: &str) -> Self {
        self.http_status = status;
        self.http_body = body.to_string();
        self
    }

    pub(crate) fn call_counter(&self) -> Arc<Calls> {
        self.calls.clone()
    }

    pub(crate) fn arc(self) -> Arc<dyn RuntimeIo> {
        Arc::new(self)
    }
}

impl RuntimeIo for StubIo {
    fn env_get(
        &self,
        key: String,
    ) -> Pin<Box<dyn Future<Output = Result<Option<String>, RuntimeIoError>> + Send + '_>> {
        self.calls.env.fetch_add(1, Ordering::SeqCst);
        let value = self.env.get(&key).cloned();
        Box::pin(async move { Ok(value) })
    }

    fn fs_read(
        &self,
        path: String,
    ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
        self.calls.fs.fetch_add(1, Ordering::SeqCst);
        let contents = self.files.get(&path).cloned();
        Box::pin(async move {
            contents.ok_or_else(|| RuntimeIoError::Other(format!("no such file: {path}")))
        })
    }

    fn http__send(
        &self,
        _request: sys_types::generated::owned::http::Request,
        _timeout_nanos: Arc<num_bigint::BigInt>,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponseHandle, RuntimeIoError>> + Send + '_>> {
        self.calls.http.fetch_add(1, Ordering::SeqCst);
        let status_code = self.http_status;
        Box::pin(async move {
            Ok(HttpResponseHandle {
                raw: BexExternalValue::Null,
                status_code,
                headers: IndexMap::new(),
                url: String::new(),
            })
        })
    }

    fn http_response_text(
        &self,
        _response: &HttpResponseHandle,
    ) -> Pin<Box<dyn Future<Output = Result<String, RuntimeIoError>> + Send + '_>> {
        let body = self.http_body.clone();
        Box::pin(async move { Ok(body) })
    }
}

/// An `OAuth2` token endpoint success body.
pub(crate) fn token_response(access_token: &str) -> String {
    serde_json::json!({
        "access_token": access_token,
        "token_type": "Bearer",
        "expires_in": 3600,
    })
    .to_string()
}

/// An `authorized_user` (refresh-grant) credential document. `tag` makes the
/// credential material unique per test, so the fork's process-wide token cache
/// can never serve one test's token to another.
pub(crate) fn authorized_user_json(tag: &str) -> String {
    serde_json::json!({
        "type": "authorized_user",
        "client_id": format!("{tag}-client-id"),
        "client_secret": format!("{tag}-client-secret"),
        "refresh_token": format!("{tag}-refresh-token"),
        "token_uri": "https://fake-oauth.example.com/token",
    })
    .to_string()
}
