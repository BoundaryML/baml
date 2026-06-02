#![allow(dead_code)]
#![allow(unreachable_pub)]

//! Reusable mock `CredentialIo` shared across integration tests.
//!
//! Other test files depend on this exact public surface — do not change method
//! names/signatures without coordinating. If you need behavior this mock does
//! not support, define a small inline mock in your own test file instead.

use std::{collections::HashMap, sync::Mutex};

use async_trait::async_trait;
use aws_config::{CommandOutput, ConfigError, CredentialIo, HttpResponse};

type HttpHandler =
    Box<dyn Fn(&str, &str, &[(String, String)]) -> Result<HttpResponse, ConfigError> + Send + Sync>;
type CommandHandler = Box<dyn Fn(&str) -> Result<CommandOutput, ConfigError> + Send + Sync>;

/// A configurable mock implementation of [`CredentialIo`].
///
/// Build it with the chainable setters, then pass `&mock` (which is `Send +
/// Sync`) into `resolve_credentials` / `resolve_region`.
pub struct MockIo {
    env: HashMap<String, String>,
    /// Exact-path file matches.
    exact_files: Vec<(String, String)>,
    /// `(substr, contents)`: returned for any `read_file` path containing `substr`.
    contains_files: Vec<(String, String)>,
    http: Option<HttpHandler>,
    command: Option<CommandHandler>,
    /// `(http_call_count, last_http_url)`.
    tracking: Mutex<(usize, Option<String>)>,
}

impl MockIo {
    pub fn new() -> MockIo {
        MockIo {
            env: HashMap::new(),
            exact_files: Vec::new(),
            contains_files: Vec::new(),
            http: None,
            command: None,
            tracking: Mutex::new((0, None)),
        }
    }

    /// Add an environment variable.
    pub fn env(mut self, key: &str, val: &str) -> MockIo {
        self.env.insert(key.to_string(), val.to_string());
        self
    }

    /// Add a file matched by exact path.
    pub fn file(mut self, path: &str, contents: &str) -> MockIo {
        self.exact_files
            .push((path.to_string(), contents.to_string()));
        self
    }

    /// Return `contents` for any `read_file` path containing `substr` (useful
    /// for sha1-hashed SSO cache paths).
    pub fn file_contains(mut self, substr: &str, contents: &str) -> MockIo {
        self.contains_files
            .push((substr.to_string(), contents.to_string()));
        self
    }

    /// Install an HTTP handler: `(method, url, headers) -> Result<HttpResponse, _>`.
    pub fn http<F>(mut self, handler: F) -> MockIo
    where
        F: Fn(&str, &str, &[(String, String)]) -> Result<HttpResponse, ConfigError>
            + Send
            + Sync
            + 'static,
    {
        self.http = Some(Box::new(handler));
        self
    }

    /// Install a command handler for `credential_process`.
    pub fn command<F>(mut self, handler: F) -> MockIo
    where
        F: Fn(&str) -> Result<CommandOutput, ConfigError> + Send + Sync + 'static,
    {
        self.command = Some(Box::new(handler));
        self
    }

    /// Number of `http()` calls made so far.
    pub fn http_calls(&self) -> usize {
        self.tracking.lock().unwrap().0
    }

    /// The URL of the most recent `http()` call, if any.
    pub fn last_http_url(&self) -> Option<String> {
        self.tracking.lock().unwrap().1.clone()
    }
}

impl Default for MockIo {
    fn default() -> Self {
        MockIo::new()
    }
}

#[async_trait]
impl CredentialIo for MockIo {
    async fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }

    async fn read_file(&self, path: &str) -> Option<String> {
        for (p, contents) in &self.exact_files {
            if p == path {
                return Some(contents.clone());
            }
        }
        for (substr, contents) in &self.contains_files {
            if path.contains(substr.as_str()) {
                return Some(contents.clone());
            }
        }
        None
    }

    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, ConfigError> {
        {
            let mut t = self.tracking.lock().unwrap();
            t.0 += 1;
            t.1 = Some(url.to_string());
        }
        match &self.http {
            Some(h) => h(method, url, headers),
            None => Err(ConfigError::Io("no http handler".into())),
        }
    }

    async fn run_command(&self, command: &str) -> Result<CommandOutput, ConfigError> {
        match &self.command {
            Some(h) => h(command),
            None => Err(ConfigError::Io("no command handler".into())),
        }
    }
}
