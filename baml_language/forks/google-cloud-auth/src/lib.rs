//! A slim, pure-Rust Google Cloud OAuth2 access-token resolver.
//!
//! Replaces `google-cloud-auth` for BAML's use case: minting Google Cloud
//! access tokens for Vertex AI. All IO (env, file, HTTP) is routed through the
//! [`TokenIo`] trait so the host can sandbox it; JWT signing is pure Rust
//! (`rsa` + `sha2`), so a single code path works on both native and wasm.
//!
//! Mirrors `google-auth` (Python/Node) Application Default Credentials as
//! closely as the `TokenIo` surface allows:
//!
//! - **Discovery order**: `GOOGLE_APPLICATION_CREDENTIALS` (file path only;
//!   a set-but-unreadable path is a hard error, not a fallthrough) → the
//!   well-known ADC config file (`$CLOUDSDK_CONFIG`, `$HOME/.config/gcloud`,
//!   or `%APPDATA%\gcloud`) → the GCE metadata server.
//! - **Credential types**: `service_account` (RS256 JWT bearer),
//!   `authorized_user` (refresh-token grant), `external_account` (workload
//!   identity federation with file- or url-sourced subject tokens, optional
//!   service-account impersonation), `external_account_authorized_user`,
//!   and `impersonated_service_account`. AWS- and executable-sourced
//!   federation and GDCH are rejected with [`AuthError::Unsupported`].
//! - **Token caching**: minted tokens are cached process-wide and reused
//!   until shortly before expiry (google-auth's 3m45s refresh threshold).
//! - **Project resolution**: [`project_id`] follows `GOOGLE_CLOUD_PROJECT` /
//!   legacy `GCLOUD_PROJECT`, the credential file, the active gcloud
//!   configuration's `core.project`, the ADC file's quota project, then the
//!   metadata server. [`quota_project_id`] backs the `x-goog-user-project`
//!   header (google-auth's `Credentials.apply`).
//!
//! Deliberately NOT supported (BAML policy): inline-JSON env vars and
//! `gcloud` CLI shell-outs.
//!
//! Env vars that are set — even to the empty string — are honored verbatim
//! (a diverge from google-auth, which treats some empty vars as unset): a
//! misconfigured `GCLOUD_PROJECT=""` produces a visibly broken request or
//! error instead of being silently skipped.

#![allow(clippy::doc_markdown)]

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use async_trait::async_trait;
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use rsa::{
    RsaPrivateKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};

/// The default OAuth2 cloud-platform scope used for Vertex AI.
pub const CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const DEFAULT_STS_TOKEN_URL: &str = "https://sts.googleapis.com/v1/token";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";
const METADATA_PROJECT_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/project/project-id";

/// Re-mint when a cached token is within this window of its expiry. Matches
/// google-auth's `REFRESH_THRESHOLD` (3m45s).
const REFRESH_THRESHOLD_SECS: u64 = 225;

// ---------------------------------------------------------------------------
// IO abstraction
// ---------------------------------------------------------------------------

/// A minimal HTTP response.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub body: String,
}

/// Async IO operations the resolver needs. Implemented by the host, typically
/// by delegating to a sandboxed runtime.
#[async_trait]
pub trait TokenIo: Send + Sync {
    /// Read an environment variable. Returns `None` if unset.
    async fn env(&self, key: &str) -> Option<String>;

    /// Read a file to a string. Returns `None` if it cannot be read.
    async fn read_file(&self, path: &str) -> Option<String>;

    /// Perform an HTTP request with the given extra headers and body.
    async fn http(
        &self,
        method: &str,
        url: &str,
        headers: &[(String, String)],
        body: &str,
    ) -> Result<HttpResponse, AuthError>;
}

/// Errors produced while resolving a Google Cloud access token.
#[derive(Debug, Clone)]
pub enum AuthError {
    /// A credential payload could not be parsed.
    Parse(String),
    /// JWT signing failed (e.g. an invalid private key).
    Signing(String),
    /// An IO operation failed.
    Io(String),
    /// The token endpoint returned a non-success status or an unexpected body.
    TokenEndpoint(String),
    /// No credential source could be discovered.
    NoCredentials(String),
    /// A credential type or source google-auth supports but this fork does not.
    Unsupported(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Parse(m)
            | AuthError::Signing(m)
            | AuthError::Io(m)
            | AuthError::TokenEndpoint(m)
            | AuthError::NoCredentials(m)
            | AuthError::Unsupported(m) => write!(f, "Google Cloud: {m}"),
        }
    }
}

impl std::error::Error for AuthError {}

// ---------------------------------------------------------------------------
// Token cache
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Token {
    access_token: String,
    /// Unix seconds when the token expires; `None` means no expiry was
    /// reported (google-auth treats such tokens as never expiring).
    expires_at: Option<u64>,
}

static TOKEN_CACHE: OnceLock<Mutex<HashMap<[u8; 32], Token>>> = OnceLock::new();

fn now_unix() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Cache keys are SHA-256 over the credential material + scope so the map
/// never holds raw credentials and distinct identities can never collide.
fn cache_key(credential_material: &str, scope: &str) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(credential_material.as_bytes());
    hasher.update([0]);
    hasher.update(scope.as_bytes());
    hasher.finalize().into()
}

fn cached_token(key: &[u8; 32]) -> Option<String> {
    let map = TOKEN_CACHE.get_or_init(Mutex::default).lock().unwrap();
    let token = map.get(key)?;
    match token.expires_at {
        None => Some(token.access_token.clone()),
        Some(exp) if now_unix() + REFRESH_THRESHOLD_SECS < exp => Some(token.access_token.clone()),
        Some(_) => None,
    }
}

fn store_token(key: [u8; 32], token: &Token) {
    TOKEN_CACHE
        .get_or_init(Mutex::default)
        .lock()
        .unwrap()
        .insert(key, token.clone());
}

/// Drop all cached tokens. Test hook; production code never needs this.
#[doc(hidden)]
pub fn clear_token_cache() {
    if let Some(map) = TOKEN_CACHE.get() {
        map.lock().unwrap().clear();
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Mint an access token from service-account JSON via the RS256 JWT-bearer flow.
pub async fn token_from_service_account_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<String, AuthError> {
    let key = cache_key(json_str, scope);
    if let Some(token) = cached_token(&key) {
        return Ok(token);
    }
    let token = mint_service_account(io, json_str, scope).await?;
    store_token(key, &token);
    Ok(token.access_token)
}

/// Mint an access token from any supported credential JSON document,
/// dispatching on its `type` field (the same dispatch `google-auth` performs
/// on a `GOOGLE_APPLICATION_CREDENTIALS` file).
pub async fn token_from_credentials_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<String, AuthError> {
    let key = cache_key(json_str, scope);
    if let Some(token) = cached_token(&key) {
        return Ok(token);
    }
    let token = mint_from_credentials_json(io, json_str, scope).await?;
    store_token(key, &token);
    Ok(token.access_token)
}

/// Returns `true` when Application Default Credentials look discoverable (a
/// `GOOGLE_APPLICATION_CREDENTIALS` file or the well-known ADC config file).
/// Does not perform any network IO. Mirrors the resolution-time probe the AWS
/// SDK performs.
pub async fn adc_available(io: &dyn TokenIo) -> bool {
    if let Some(path) = gac_path(io).await {
        if io.read_file(&path).await.is_some() {
            return true;
        }
    }
    if let Some(path) = adc_config_path(io).await {
        if io.read_file(&path).await.is_some() {
            return true;
        }
    }
    false
}

/// Mint an access token via Application Default Credentials: the
/// `GOOGLE_APPLICATION_CREDENTIALS` file, the well-known ADC config file, then
/// the GCE metadata server.
pub async fn token_from_adc(io: &dyn TokenIo, scope: &str) -> Result<String, AuthError> {
    // 1. GOOGLE_APPLICATION_CREDENTIALS file path. Like google-auth, a set
    //    var whose file cannot be read is an error, not a fallthrough.
    if let Some(path) = gac_path(io).await {
        let Some(contents) = io.read_file(&path).await else {
            return Err(AuthError::NoCredentials(format!(
                "GOOGLE_APPLICATION_CREDENTIALS points to '{path}' but the file could not be read"
            )));
        };
        return token_from_credentials_json(io, &contents, scope).await;
    }

    // 2. Well-known ADC config file.
    if let Some(path) = adc_config_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            return token_from_credentials_json(io, &contents, scope).await;
        }
    }

    // 3. GCE metadata server.
    let key = cache_key("\u{0}gce-metadata", scope);
    if let Some(token) = cached_token(&key) {
        return Ok(token);
    }
    let token = mint_metadata(io, scope).await?;
    store_token(key, &token);
    Ok(token.access_token)
}

/// Resolve the active project id the way `google-auth` does:
/// `GOOGLE_CLOUD_PROJECT` (legacy `GCLOUD_PROJECT`) → the
/// `GOOGLE_APPLICATION_CREDENTIALS` file → the active gcloud configuration's
/// `core.project` → the well-known ADC file's quota/project id → the GCE
/// metadata server.
pub async fn project_id(io: &dyn TokenIo) -> Option<String> {
    for env_key in ["GOOGLE_CLOUD_PROJECT", "GCLOUD_PROJECT"] {
        if let Some(val) = io.env(env_key).await {
            let val = val.trim().to_string();
            // Ignore unexpanded `$VAR` placeholders from .env files. A set-but-
            // empty var is honored so the misconfiguration is visible.
            if !val.starts_with('$') {
                return Some(val);
            }
        }
    }

    if let Some(path) = gac_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            if let Some(pid) = project_id_from_json(&contents) {
                return Some(pid);
            }
        }
    }

    if let Some(pid) = gcloud_config_project(io).await {
        return Some(pid);
    }

    if let Some(path) = adc_config_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            if let Some(pid) = project_id_from_json(&contents) {
                return Some(pid);
            }
        }
    }

    metadata_project_id(io).await
}

/// Extract a project id from a credential JSON document (`project_id`, falling
/// back to `quota_project_id` for `authorized_user` documents).
pub fn project_id_from_json(json_str: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(json_str).ok()?;
    ["project_id", "quota_project_id"]
        .iter()
        .find_map(|k| value.get(k)?.as_str())
        .map(String::from)
}

/// Resolve the quota project (billing/quota attribution for user credentials):
/// `GOOGLE_CLOUD_QUOTA_PROJECT` → the `GOOGLE_APPLICATION_CREDENTIALS` file →
/// the well-known ADC config file. Backs the `x-goog-user-project` header,
/// like google-auth's `Credentials.apply`.
pub async fn quota_project_id(io: &dyn TokenIo) -> Option<String> {
    if let Some(val) = io.env("GOOGLE_CLOUD_QUOTA_PROJECT").await {
        return Some(val.trim().to_string());
    }
    if let Some(path) = gac_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            if let Some(qp) = quota_project_id_from_json(&contents) {
                return Some(qp);
            }
        }
    }
    if let Some(path) = adc_config_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            if let Some(qp) = quota_project_id_from_json(&contents) {
                return Some(qp);
            }
        }
    }
    None
}

/// Extract `quota_project_id` from a credential JSON document.
pub fn quota_project_id_from_json(json_str: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(json_str)
        .ok()
        .and_then(|v| v.get("quota_project_id")?.as_str().map(String::from))
}

// ---------------------------------------------------------------------------
// Credential-type dispatch
// ---------------------------------------------------------------------------

async fn mint_from_credentials_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<Token, AuthError> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| AuthError::Parse(format!("failed to parse credential JSON: {e}")))?;
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("authorized_user") => mint_authorized_user(io, &value).await,
        Some("service_account") => mint_service_account(io, json_str, scope).await,
        Some("external_account") => mint_external_account(io, &value, scope).await,
        Some("external_account_authorized_user") => {
            mint_external_account_authorized_user(io, &value).await
        }
        Some("impersonated_service_account") => mint_impersonated(io, &value, scope).await,
        Some("gdch_service_account") => Err(AuthError::Unsupported(
            "GDCH service-account credentials are not supported".to_string(),
        )),
        Some(other) => Err(AuthError::NoCredentials(format!(
            "unsupported ADC credential type '{other}' (supported: service_account, \
             authorized_user, external_account, external_account_authorized_user, \
             impersonated_service_account)"
        ))),
        None => Err(AuthError::Parse(
            "ADC JSON missing 'type' field".to_string(),
        )),
    }
}

// ---------------------------------------------------------------------------
// Flows: service account (RS256 JWT-bearer)
// ---------------------------------------------------------------------------

#[derive(serde::Deserialize)]
struct ServiceAccount {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

async fn mint_service_account(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<Token, AuthError> {
    let sa: ServiceAccount = serde_json::from_str(json_str)
        .map_err(|e| AuthError::Parse(format!("failed to parse service account JSON: {e}")))?;
    let jwt = sign_service_account_jwt(&sa, scope)?;
    let token_uri = sa.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let body = format!(
        "grant_type={}&assertion={}",
        encode("urn:ietf:params:oauth:grant-type:jwt-bearer"),
        encode(&jwt),
    );
    post_token_form(io, token_uri, Vec::new(), &body).await
}

/// Sign a service-account JWT using RSASSA-PKCS1-v1_5 with SHA-256.
fn sign_service_account_jwt(sa: &ServiceAccount, scope: &str) -> Result<String, AuthError> {
    #[allow(clippy::cast_possible_wrap)]
    let now = now_unix() as i64;

    let token_uri = sa.token_uri.as_deref().unwrap_or(DEFAULT_TOKEN_URI);
    let header = serde_json::json!({ "alg": "RS256", "typ": "JWT" });
    let claims = serde_json::json!({
        "iss": sa.client_email,
        "scope": scope,
        "aud": token_uri,
        "iat": now,
        "exp": now + 3600,
    });

    let header_b64 = URL_SAFE_NO_PAD.encode(header.to_string());
    let claims_b64 = URL_SAFE_NO_PAD.encode(claims.to_string());
    let signing_input = format!("{header_b64}.{claims_b64}");

    let private_key = RsaPrivateKey::from_pkcs8_pem(&sa.private_key)
        .map_err(|e| AuthError::Signing(format!("failed to parse PKCS8 private key: {e}")))?;
    let signing_key = rsa::pkcs1v15::SigningKey::<sha2::Sha256>::new(private_key);

    let signature = signing_key.sign(signing_input.as_bytes());
    let sig_b64 = URL_SAFE_NO_PAD.encode(signature.to_bytes());

    Ok(format!("{signing_input}.{sig_b64}"))
}

// ---------------------------------------------------------------------------
// Flows: authorized user (OAuth2 refresh-token grant)
// ---------------------------------------------------------------------------

async fn mint_authorized_user(
    io: &dyn TokenIo,
    value: &serde_json::Value,
) -> Result<Token, AuthError> {
    let client_id = str_field(value, "client_id", "authorized_user")?;
    let client_secret = str_field(value, "client_secret", "authorized_user")?;
    let refresh_token = str_field(value, "refresh_token", "authorized_user")?;
    let token_uri = value
        .get("token_uri")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_TOKEN_URI);

    let body = format!(
        "grant_type=refresh_token&client_id={}&client_secret={}&refresh_token={}",
        encode(client_id),
        encode(client_secret),
        encode(refresh_token),
    );
    post_token_form(io, token_uri, Vec::new(), &body).await
}

// ---------------------------------------------------------------------------
// Flows: external account (workload identity federation)
// ---------------------------------------------------------------------------

async fn mint_external_account(
    io: &dyn TokenIo,
    value: &serde_json::Value,
    scope: &str,
) -> Result<Token, AuthError> {
    let audience = str_field(value, "audience", "external_account")?;
    let subject_token_type = str_field(value, "subject_token_type", "external_account")?;
    let token_url = value
        .get("token_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_STS_TOKEN_URL);
    let source = value.get("credential_source").ok_or_else(|| {
        AuthError::Parse("external_account missing 'credential_source'".to_string())
    })?;

    if let Some(env_id) = source
        .get("environment_id")
        .and_then(serde_json::Value::as_str)
    {
        if env_id.starts_with("aws") {
            return Err(AuthError::Unsupported(
                "AWS-sourced workload identity federation is not supported".to_string(),
            ));
        }
    }
    if source.get("executable").is_some() {
        return Err(AuthError::Unsupported(
            "executable-sourced workload identity federation is not supported".to_string(),
        ));
    }

    let subject_token = fetch_subject_token(io, source).await?;

    let impersonation_url = value
        .get("service_account_impersonation_url")
        .and_then(serde_json::Value::as_str);
    // With impersonation the STS leg always uses cloud-platform; the caller's
    // scope is applied at the generateAccessToken step (matches google-auth).
    let sts_scope = if impersonation_url.is_some() {
        CLOUD_PLATFORM_SCOPE
    } else {
        scope
    };

    let mut body = format!(
        "grant_type={}&audience={}&scope={}&requested_token_type={}&subject_token={}&subject_token_type={}",
        encode("urn:ietf:params:oauth:grant-type:token-exchange"),
        encode(audience),
        encode(sts_scope),
        encode("urn:ietf:params:oauth:token-type:access_token"),
        encode(&subject_token),
        encode(subject_token_type),
    );

    let mut headers = Vec::new();
    let client_id = value.get("client_id").and_then(serde_json::Value::as_str);
    let client_secret = value
        .get("client_secret")
        .and_then(serde_json::Value::as_str);
    if let (Some(id), Some(secret)) = (client_id, client_secret) {
        headers.push((
            "authorization".to_string(),
            format!("Basic {}", STANDARD.encode(format!("{id}:{secret}"))),
        ));
    } else if let Some(user_project) = value
        .get("workforce_pool_user_project")
        .and_then(serde_json::Value::as_str)
    {
        // Workforce pools need a user project when no client auth is given.
        use std::fmt::Write as _;
        let options = serde_json::json!({ "userProject": user_project }).to_string();
        let _ = write!(body, "&options={}", encode(&options));
    }

    let sts_token = post_token_form(io, token_url, headers, &body).await?;

    match impersonation_url {
        None => Ok(sts_token),
        Some(url) => impersonate(io, url, &sts_token.access_token, scope, None).await,
    }
}

/// Fetch the third-party subject token from a `file` or `url` credential
/// source, applying the optional `format` (text or JSON-field extraction).
async fn fetch_subject_token(
    io: &dyn TokenIo,
    source: &serde_json::Value,
) -> Result<String, AuthError> {
    let raw = if let Some(file) = source.get("file").and_then(serde_json::Value::as_str) {
        io.read_file(file).await.ok_or_else(|| {
            AuthError::Io(format!("failed to read WIF subject-token file '{file}'"))
        })?
    } else if let Some(url) = source.get("url").and_then(serde_json::Value::as_str) {
        let mut headers = Vec::new();
        if let Some(hs) = source.get("headers").and_then(serde_json::Value::as_object) {
            for (k, v) in hs {
                if let Some(v) = v.as_str() {
                    headers.push((k.clone(), v.to_string()));
                }
            }
        }
        let resp = io.http("GET", url, &headers, "").await?;
        if !(200..300).contains(&resp.status) {
            return Err(AuthError::TokenEndpoint(format!(
                "WIF subject-token URL '{url}' returned status {}",
                resp.status
            )));
        }
        resp.body
    } else {
        return Err(AuthError::Unsupported(
            "external_account credential_source must provide 'file' or 'url'".to_string(),
        ));
    };

    let format = source.get("format");
    let format_type = format
        .and_then(|f| f.get("type"))
        .and_then(serde_json::Value::as_str)
        .unwrap_or("text");
    if format_type == "json" {
        let field = format
            .and_then(|f| f.get("subject_token_field_name"))
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| {
                AuthError::Parse(
                    "external_account JSON format missing 'subject_token_field_name'".to_string(),
                )
            })?;
        let parsed: serde_json::Value = serde_json::from_str(&raw)
            .map_err(|e| AuthError::Parse(format!("failed to parse subject token JSON: {e}")))?;
        parsed
            .get(field)
            .and_then(serde_json::Value::as_str)
            .map(String::from)
            .ok_or_else(|| AuthError::Parse(format!("subject token JSON missing '{field}'")))
    } else {
        Ok(raw.trim().to_string())
    }
}

// ---------------------------------------------------------------------------
// Flows: external account authorized user (workforce refresh grant)
// ---------------------------------------------------------------------------

async fn mint_external_account_authorized_user(
    io: &dyn TokenIo,
    value: &serde_json::Value,
) -> Result<Token, AuthError> {
    let refresh_token = str_field(value, "refresh_token", "external_account_authorized_user")?;
    let client_id = str_field(value, "client_id", "external_account_authorized_user")?;
    let client_secret = str_field(value, "client_secret", "external_account_authorized_user")?;
    let token_url = value
        .get("token_url")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(DEFAULT_STS_TOKEN_URL);

    let headers = vec![(
        "authorization".to_string(),
        format!(
            "Basic {}",
            STANDARD.encode(format!("{client_id}:{client_secret}"))
        ),
    )];
    let body = format!(
        "grant_type=refresh_token&refresh_token={}",
        encode(refresh_token),
    );
    // The response may carry a rotated refresh_token; with no store to persist
    // it, the on-disk credential keeps working until Google expires it.
    post_token_form(io, token_url, headers, &body).await
}

// ---------------------------------------------------------------------------
// Flows: impersonated service account
// ---------------------------------------------------------------------------

async fn mint_impersonated(
    io: &dyn TokenIo,
    value: &serde_json::Value,
    scope: &str,
) -> Result<Token, AuthError> {
    let url = str_field(
        value,
        "service_account_impersonation_url",
        "impersonated_service_account",
    )?;
    let source = value.get("source_credentials").ok_or_else(|| {
        AuthError::Parse("impersonated_service_account missing 'source_credentials'".to_string())
    })?;

    let source_token = match source.get("type").and_then(serde_json::Value::as_str) {
        Some("authorized_user") => mint_authorized_user(io, source).await?,
        Some("service_account") => {
            mint_service_account(io, &source.to_string(), CLOUD_PLATFORM_SCOPE).await?
        }
        other => {
            return Err(AuthError::Unsupported(format!(
                "impersonated_service_account source_credentials type {other:?} is not \
                 supported (only service_account and authorized_user)"
            )));
        }
    };

    impersonate(
        io,
        url,
        &source_token.access_token,
        scope,
        value.get("delegates"),
    )
    .await
}

/// Exchange a source token for an impersonated service-account token via the
/// IAM Credentials `generateAccessToken` endpoint.
async fn impersonate(
    io: &dyn TokenIo,
    url: &str,
    source_bearer: &str,
    scope: &str,
    delegates: Option<&serde_json::Value>,
) -> Result<Token, AuthError> {
    let mut body = serde_json::json!({ "scope": [scope], "lifetime": "3600s" });
    if let Some(delegates) = delegates {
        body["delegates"] = delegates.clone();
    }
    let headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        (
            "authorization".to_string(),
            format!("Bearer {source_bearer}"),
        ),
    ];
    let resp = io.http("POST", url, &headers, &body.to_string()).await?;
    if !(200..300).contains(&resp.status) {
        return Err(AuthError::TokenEndpoint(format!(
            "service-account impersonation endpoint returned status {}: {}",
            resp.status, resp.body
        )));
    }
    let value: serde_json::Value = serde_json::from_str(&resp.body).map_err(|e| {
        AuthError::TokenEndpoint(format!("failed to parse impersonation response: {e}"))
    })?;
    let access_token = value
        .get("accessToken")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| {
            AuthError::TokenEndpoint("impersonation response missing 'accessToken'".to_string())
        })?;
    let expires_at = value
        .get("expireTime")
        .and_then(serde_json::Value::as_str)
        .and_then(rfc3339_to_unix);
    Ok(Token {
        access_token,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Flows: GCE metadata server
// ---------------------------------------------------------------------------

async fn mint_metadata(io: &dyn TokenIo, scope: &str) -> Result<Token, AuthError> {
    let url = format!("{METADATA_TOKEN_URL}?scopes={}", encode(scope));
    let headers = vec![("Metadata-Flavor".to_string(), "Google".to_string())];
    let resp = io.http("GET", &url, &headers, "").await?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(AuthError::NoCredentials(format!(
            "no ADC credentials found (checked GOOGLE_APPLICATION_CREDENTIALS, the gcloud ADC \
             file, and the GCE metadata server, which returned status {}). To set up ADC, run \
             `gcloud auth application-default login` or see \
             https://cloud.google.com/docs/authentication/external/set-up-adc",
            resp.status
        )));
    }
    parse_token_response(&resp.body)
}

async fn metadata_project_id(io: &dyn TokenIo) -> Option<String> {
    let headers = vec![("Metadata-Flavor".to_string(), "Google".to_string())];
    let resp = io
        .http("GET", METADATA_PROJECT_URL, &headers, "")
        .await
        .ok()?;
    if !(200..300).contains(&resp.status) {
        return None;
    }
    let pid = resp.body.trim().to_string();
    (!pid.is_empty()).then_some(pid)
}

// ---------------------------------------------------------------------------
// Token endpoint helpers
// ---------------------------------------------------------------------------

/// POST a form-encoded body to an OAuth2/STS token endpoint.
async fn post_token_form(
    io: &dyn TokenIo,
    url: &str,
    mut headers: Vec<(String, String)>,
    body: &str,
) -> Result<Token, AuthError> {
    headers.push((
        "content-type".to_string(),
        "application/x-www-form-urlencoded".to_string(),
    ));
    let resp = io.http("POST", url, &headers, body).await?;
    if resp.status < 200 || resp.status >= 300 {
        return Err(AuthError::TokenEndpoint(format!(
            "token endpoint returned status {}: {}",
            resp.status, resp.body
        )));
    }
    parse_token_response(&resp.body)
}

fn parse_token_response(body: &str) -> Result<Token, AuthError> {
    let value: serde_json::Value = serde_json::from_str(body)
        .map_err(|e| AuthError::TokenEndpoint(format!("failed to parse token response: {e}")))?;
    let access_token = value
        .get("access_token")
        .and_then(serde_json::Value::as_str)
        .map(String::from)
        .ok_or_else(|| {
            AuthError::TokenEndpoint("token response missing 'access_token'".to_string())
        })?;
    let expires_at = value
        .get("expires_in")
        .and_then(serde_json::Value::as_u64)
        .map(|secs| now_unix() + secs);
    Ok(Token {
        access_token,
        expires_at,
    })
}

// ---------------------------------------------------------------------------
// Discovery: ADC paths, gcloud config, project id
// ---------------------------------------------------------------------------

/// The `GOOGLE_APPLICATION_CREDENTIALS` value (always a file path; inline
/// JSON is deliberately not supported). A set-but-empty value is returned
/// as-is so it fails visibly instead of being silently skipped.
async fn gac_path(io: &dyn TokenIo) -> Option<String> {
    io.env("GOOGLE_APPLICATION_CREDENTIALS").await
}

/// The gcloud config directory: `$CLOUDSDK_CONFIG`, `$HOME/.config/gcloud`
/// (Unix), or `%APPDATA%\gcloud` (Windows). Set-but-empty vars are honored.
async fn gcloud_config_dir(io: &dyn TokenIo) -> Option<String> {
    if let Some(dir) = io.env("CLOUDSDK_CONFIG").await {
        return Some(dir);
    }
    if let Some(home) = io.env("HOME").await {
        return Some(format!("{home}/.config/gcloud"));
    }
    if let Some(appdata) = io.env("APPDATA").await {
        return Some(format!("{appdata}/gcloud"));
    }
    None
}

/// The well-known ADC config file path.
async fn adc_config_path(io: &dyn TokenIo) -> Option<String> {
    Some(format!(
        "{}/application_default_credentials.json",
        gcloud_config_dir(io).await?
    ))
}

/// `core.project` from the active gcloud configuration file — the same value
/// google-auth reports for gcloud-user ADC, but read from disk instead of
/// shelling out to `gcloud config get-value project`.
async fn gcloud_config_project(io: &dyn TokenIo) -> Option<String> {
    let dir = gcloud_config_dir(io).await?;
    let config_name = match io.env("CLOUDSDK_ACTIVE_CONFIG_NAME").await {
        Some(name) => name,
        None => io
            .read_file(&format!("{dir}/active_config"))
            .await
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "default".to_string()),
    };
    let contents = io
        .read_file(&format!("{dir}/configurations/config_{config_name}"))
        .await?;
    parse_gcloud_config_project(&contents)
}

/// Extract `project` from the `[core]` section of a gcloud config file.
fn parse_gcloud_config_project(contents: &str) -> Option<String> {
    let mut in_core = false;
    for line in contents.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_core = line == "[core]";
            continue;
        }
        if !in_core {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            if key.trim() == "project" {
                let val = val.trim();
                if !val.is_empty() {
                    return Some(val.to_string());
                }
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Small helpers
// ---------------------------------------------------------------------------

fn encode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

fn str_field<'a>(
    value: &'a serde_json::Value,
    key: &str,
    credential_type: &str,
) -> Result<&'a str, AuthError> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| AuthError::Parse(format!("{credential_type} ADC missing '{key}'")))
}

/// Parse an RFC 3339 UTC timestamp (`2026-07-09T12:34:56Z`, optionally with
/// fractional seconds) to unix seconds. Only the `Z` offset is supported —
/// that is what Google's APIs emit.
fn rfc3339_to_unix(s: &str) -> Option<u64> {
    let s = s.strip_suffix('Z')?;
    let (date, time) = s.split_once('T')?;

    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    let time = time.split('.').next()?;
    let mut parts = time.split(':');
    let hour: u64 = parts.next()?.parse().ok()?;
    let minute: u64 = parts.next()?.parse().ok()?;
    let second: u64 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || hour > 23 || minute > 59 || second > 60 {
        return None;
    }

    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    #[allow(clippy::cast_sign_loss)]
    Some(days as u64 * 86_400 + hour * 3_600 + minute * 60 + second)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400; // [0, 399]
    let day_of_year = i64::from((153 * ((month + 9) % 12) + 2) / 5 + day - 1); // [0, 365]
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

// ---------------------------------------------------------------------------
// Tests (pure functions; flow coverage lives in tests/)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_private_key() {
        let sa = ServiceAccount {
            client_email: "x@y.iam.gserviceaccount.com".into(),
            private_key: "not-a-pem".into(),
            token_uri: None,
        };
        assert!(sign_service_account_jwt(&sa, CLOUD_PLATFORM_SCOPE).is_err());
    }

    #[test]
    fn rfc3339_parses_google_timestamps() {
        // Cross-checked with Python `datetime.timestamp()`.
        assert_eq!(rfc3339_to_unix("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_to_unix("2026-07-09T00:00:00Z"), Some(1_783_555_200));
        assert_eq!(rfc3339_to_unix("2024-02-29T23:59:59Z"), Some(1_709_251_199));
        // Fractional seconds (IAM Credentials emits these).
        assert_eq!(
            rfc3339_to_unix("2026-07-09T00:00:00.123456Z"),
            Some(1_783_555_200)
        );
        // Non-UTC offsets are not Google API output; refuse rather than guess.
        assert_eq!(rfc3339_to_unix("2026-07-09T00:00:00+02:00"), None);
        assert_eq!(rfc3339_to_unix("garbage"), None);
    }

    #[test]
    fn gcloud_config_project_parses_core_section_only() {
        let config = "\
[compute]
zone = us-central1-c
project = wrong-project

[core]
account = dev@example.com
project = right-project
";
        assert_eq!(
            parse_gcloud_config_project(config),
            Some("right-project".to_string())
        );
        assert_eq!(parse_gcloud_config_project("[core]\naccount = x\n"), None);
    }

    #[test]
    fn project_id_from_json_prefers_project_id() {
        let sa = r#"{"type":"service_account","project_id":"p1","quota_project_id":"q1"}"#;
        assert_eq!(project_id_from_json(sa), Some("p1".to_string()));
        let user = r#"{"type":"authorized_user","quota_project_id":"q1"}"#;
        assert_eq!(project_id_from_json(user), Some("q1".to_string()));
        assert_eq!(project_id_from_json("{}"), None);
        assert_eq!(quota_project_id_from_json(sa), Some("q1".to_string()));
    }

    #[test]
    fn cache_respects_refresh_threshold() {
        let key = cache_key("cache-threshold-test", CLOUD_PLATFORM_SCOPE);
        // Expiring within the 225s threshold -> treated as stale.
        store_token(
            key,
            &Token {
                access_token: "stale".into(),
                expires_at: Some(now_unix() + 10),
            },
        );
        assert_eq!(cached_token(&key), None);
        // Comfortably before expiry -> served from cache.
        store_token(
            key,
            &Token {
                access_token: "fresh".into(),
                expires_at: Some(now_unix() + 3600),
            },
        );
        assert_eq!(cached_token(&key), Some("fresh".to_string()));
        // No expiry reported -> never expires (google-auth parity).
        store_token(
            key,
            &Token {
                access_token: "eternal".into(),
                expires_at: None,
            },
        );
        assert_eq!(cached_token(&key), Some("eternal".to_string()));
    }
}
