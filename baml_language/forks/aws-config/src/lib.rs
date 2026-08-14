// Docs reference AWS env-var and JSON field names (AWS_REGION, AccessKeyId,
// credential_process, etc.) that read fine without backticks.
#![allow(clippy::doc_markdown)]

//! A slim AWS credential and region resolver.
//!
//! Replaces `aws-config` for BAML's use case. All IO (environment variables,
//! file reads, HTTP, and subprocess execution for `credential_process`) is
//! routed through the [`CredentialIo`] trait so the host can sandbox it.
//!
//! [`resolve_credentials`] walks the standard AWS provider chain in order:
//! environment → shared profile (static creds / `credential_process` / SSO) →
//! ECS container endpoint → EC2 IMDS. [`resolve_region`] checks `AWS_REGION`,
//! then `AWS_DEFAULT_REGION`, then the active profile's `region` key.

use std::collections::HashMap;

use async_trait::async_trait;
pub use aws_sigv4::Credentials;

mod ini;
mod profile;
mod providers;

pub use ini::IniFile;

// ---------------------------------------------------------------------------
// IO abstraction
// ---------------------------------------------------------------------------

/// Result of an HTTP request made during credential resolution.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Output of a subprocess run for `credential_process`.
#[derive(Debug, Clone)]
pub struct CommandOutput {
    pub status: i32,
    pub stdout: String,
}

/// Errors surfaced by the resolver.
#[derive(Debug, Clone)]
pub enum ConfigError {
    /// No provider in the chain produced credentials.
    NoCredentials(String),
    /// An IO operation failed.
    Io(String),
    /// A credential payload could not be parsed.
    Parse(String),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::NoCredentials(m) => write!(f, "no AWS credentials found: {m}"),
            ConfigError::Io(m) => write!(f, "AWS config IO error: {m}"),
            ConfigError::Parse(m) => write!(f, "AWS credential parse error: {m}"),
        }
    }
}

impl std::error::Error for ConfigError {}

/// Async IO operations the resolver needs. Implemented by the host (e.g. by
/// delegating to a sandboxed runtime). Method semantics mirror the AWS SDK's
/// default environment/filesystem/HTTP behavior.
#[async_trait]
pub trait CredentialIo: Send + Sync {
    /// Read an environment variable. Returns `None` if unset.
    async fn env(&self, key: &str) -> Option<String>;

    /// Read a file to a string. Returns `None` if it cannot be read.
    async fn read_file(&self, path: &str) -> Option<String>;

    /// Perform an HTTP request. `headers` are extra request headers.
    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<HttpResponse, ConfigError>;

    /// Run a subprocess for `credential_process`. The default returns an error,
    /// disabling `credential_process` support unless the host overrides it.
    async fn run_command(&self, _command: &str) -> Result<CommandOutput, ConfigError> {
        Err(ConfigError::Io(
            "credential_process is not supported by this runtime".into(),
        ))
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Resolve the AWS region: `AWS_REGION`, then `AWS_DEFAULT_REGION`, then the
/// active profile's `region` key. `profile_override` corresponds to the
/// client's explicit `profile` option (else `AWS_PROFILE`, else `default`).
pub async fn resolve_region(
    io: &dyn CredentialIo,
    profile_override: Option<&str>,
) -> Option<String> {
    if let Some(r) = io.env("AWS_REGION").await {
        return Some(r);
    }
    if let Some(r) = io.env("AWS_DEFAULT_REGION").await {
        return Some(r);
    }

    let profiles = profile::load_profiles(io).await;
    let name = profile::active_profile_name(io, profile_override).await;
    profile::resolve_region_from_profiles(&profiles, &name)
}

/// Resolve AWS credentials by walking the default provider chain.
pub async fn resolve_credentials(
    io: &dyn CredentialIo,
    profile_override: Option<&str>,
) -> Result<Credentials, ConfigError> {
    let mut tried: Vec<String> = Vec::new();

    // 1. Environment variables.
    if let Some(creds) = providers::from_env(io).await {
        return Ok(creds);
    }
    tried.push("environment".into());

    // 2. Shared profile: static creds, then credential_process, then SSO.
    match providers::from_profile(io, profile_override).await {
        Ok(Some(creds)) => return Ok(creds),
        Ok(None) => {}
        Err(e) => return Err(e),
    }
    tried.push("profile".into());

    // 3. ECS / container endpoint.
    match providers::from_container(io).await {
        Ok(Some(creds)) => return Ok(creds),
        Ok(None) => {}
        Err(e) => return Err(e),
    }
    tried.push("container".into());

    // 4. EC2 instance metadata (IMDS).
    match providers::from_imds(io).await {
        Ok(Some(creds)) => return Ok(creds),
        Ok(None) => {}
        Err(e) => return Err(e),
    }
    tried.push("imds".into());

    Err(ConfigError::NoCredentials(format!(
        "tried: {}",
        tried.join(", ")
    )))
}

/// Parse a JSON credential payload shared by the container and IMDS providers.
/// Keys are matched case-insensitively. Like upstream `aws-config`'s
/// refreshable JSON parser, this requires a session token and an expiration
/// field, even though the slim `Credentials` type does not store expiration.
pub(crate) fn credentials_from_json(body: &str) -> Result<Credentials, ConfigError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| ConfigError::Parse(format!("invalid credential JSON: {e}")))?;
    refreshable_credentials_from_value(&value)
}

/// Credential object parser for providers where the session token is optional
/// (`credential_process`) or where the containing service already defined the
/// token contract (SSO).
pub(crate) fn credentials_from_value(
    value: &serde_json::Value,
) -> Result<Credentials, ConfigError> {
    let access_key_id = string_field(value, "AccessKeyId")
        .ok_or_else(|| ConfigError::Parse("missing AccessKeyId".into()))?;
    let secret_access_key = string_field(value, "SecretAccessKey")
        .ok_or_else(|| ConfigError::Parse("missing SecretAccessKey".into()))?;
    let token = string_field(value, "SessionToken").or_else(|| string_field(value, "Token"));

    Ok(Credentials::new(access_key_id, secret_access_key, token))
}

pub(crate) fn credential_process_credentials_from_value(
    value: &serde_json::Value,
) -> Result<Credentials, ConfigError> {
    let access_key_id = string_field(value, "AccessKeyId")
        .ok_or_else(|| ConfigError::Parse("missing AccessKeyId".into()))?;
    let secret_access_key = string_field(value, "SecretAccessKey")
        .ok_or_else(|| ConfigError::Parse("missing SecretAccessKey".into()))?;
    let token = string_field(value, "SessionToken");

    Ok(Credentials::new(access_key_id, secret_access_key, token))
}

fn refreshable_credentials_from_value(
    value: &serde_json::Value,
) -> Result<Credentials, ConfigError> {
    if let Some(code) = string_field(value, "Code") {
        if !code.eq_ignore_ascii_case("Success") {
            let message = string_field(value, "Message").unwrap_or_else(|| "no message".into());
            return Err(ConfigError::Io(format!(
                "credential endpoint returned error [{code}]: {message}"
            )));
        }
    }

    let access_key_id = string_field(value, "AccessKeyId")
        .ok_or_else(|| ConfigError::Parse("missing AccessKeyId".into()))?;
    let secret_access_key = string_field(value, "SecretAccessKey")
        .ok_or_else(|| ConfigError::Parse("missing SecretAccessKey".into()))?;
    let token = string_field(value, "Token")
        .or_else(|| string_field(value, "SessionToken"))
        .ok_or_else(|| ConfigError::Parse("missing Token".into()))?;
    string_field(value, "Expiration")
        .or_else(|| string_field(value, "ExpiresAt"))
        .ok_or_else(|| ConfigError::Parse("missing Expiration".into()))?;

    Ok(Credentials::new(
        access_key_id,
        secret_access_key,
        Some(token),
    ))
}

fn string_field(value: &serde_json::Value, field: &str) -> Option<String> {
    value.as_object()?.iter().find_map(|(key, value)| {
        if key.eq_ignore_ascii_case(field) {
            value.as_str().map(ToString::to_string)
        } else {
            None
        }
    })
}

// Shared profile-map type used across modules.
pub(crate) type Profiles = HashMap<String, HashMap<String, String>>;
