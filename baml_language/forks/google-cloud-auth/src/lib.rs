//! A slim, pure-Rust Google Cloud OAuth2 access-token resolver.
//!
//! Replaces `google-cloud-auth` for BAML's use case: minting Google Cloud
//! access tokens for Vertex AI. All IO (env, file, HTTP) is routed through the
//! [`TokenIo`] trait so the host can sandbox it; JWT signing is pure Rust
//! (`rsa` + `sha2`), so a single code path works on both native and wasm.
//!
//! Supported credential sources:
//! - **Service account** JSON — RS256 JWT bearer assertion exchanged for a
//!   token at the account's `token_uri`.
//! - **Application Default Credentials** — `authorized_user` JSON (OAuth2
//!   refresh-token grant), `service_account` JSON, or the GCE metadata server,
//!   discovered from `GOOGLE_APPLICATION_CREDENTIALS` (a file path only; a
//!   set-but-unreadable path is a hard error, not a fallthrough — matching
//!   google-auth) or the well-known ADC config path (`$CLOUDSDK_CONFIG`,
//!   `$HOME/.config/gcloud`, or `%APPDATA%\gcloud`).
//!
//! Minted tokens are cached process-wide and reused until shortly before
//! expiry (google-auth's 3m45s refresh threshold), so per-request callers do
//! not re-mint on every call.

#![allow(clippy::doc_markdown)]

use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

use async_trait::async_trait;
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use rsa::{
    RsaPrivateKey,
    pkcs8::DecodePrivateKey,
    signature::{SignatureEncoding, Signer},
};

/// The default OAuth2 cloud-platform scope used for Vertex AI.
pub const CLOUD_PLATFORM_SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";

const DEFAULT_TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const METADATA_TOKEN_URL: &str =
    "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/token";

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
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Parse(m)
            | AuthError::Signing(m)
            | AuthError::Io(m)
            | AuthError::TokenEndpoint(m)
            | AuthError::NoCredentials(m) => write!(f, "Google Cloud: {m}"),
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
        return token_from_adc_json(io, &contents, scope).await;
    }

    // 2. Well-known ADC config file.
    if let Some(path) = adc_config_path(io).await {
        if let Some(contents) = io.read_file(&path).await {
            return token_from_adc_json(io, &contents, scope).await;
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

// ---------------------------------------------------------------------------
// ADC JSON dispatch
// ---------------------------------------------------------------------------

async fn token_from_adc_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<String, AuthError> {
    let key = cache_key(json_str, scope);
    if let Some(token) = cached_token(&key) {
        return Ok(token);
    }
    let token = mint_from_adc_json(io, json_str, scope).await?;
    store_token(key, &token);
    Ok(token.access_token)
}

async fn mint_from_adc_json(
    io: &dyn TokenIo,
    json_str: &str,
    scope: &str,
) -> Result<Token, AuthError> {
    let value: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| AuthError::Parse(format!("failed to parse ADC JSON: {e}")))?;
    match value.get("type").and_then(serde_json::Value::as_str) {
        Some("authorized_user") => mint_authorized_user(io, &value).await,
        Some("service_account") => mint_service_account(io, json_str, scope).await,
        Some(other) => Err(AuthError::NoCredentials(format!(
            "unsupported ADC credential type '{other}' (only service_account and authorized_user are supported)"
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

// ---------------------------------------------------------------------------
// Token endpoint helpers
// ---------------------------------------------------------------------------

/// POST a form-encoded body to an OAuth2 token endpoint.
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
// ADC config path discovery
// ---------------------------------------------------------------------------

/// A non-empty `GOOGLE_APPLICATION_CREDENTIALS` value (always a file path;
/// inline JSON is deliberately not supported).
async fn gac_path(io: &dyn TokenIo) -> Option<String> {
    io.env("GOOGLE_APPLICATION_CREDENTIALS")
        .await
        .filter(|s| !s.is_empty())
}

/// The gcloud config directory: `$CLOUDSDK_CONFIG`, `$HOME/.config/gcloud`
/// (Unix), or `%APPDATA%\gcloud` (Windows).
async fn gcloud_config_dir(io: &dyn TokenIo) -> Option<String> {
    if let Some(dir) = io.env("CLOUDSDK_CONFIG").await.filter(|s| !s.is_empty()) {
        return Some(dir);
    }
    if let Some(home) = io.env("HOME").await.filter(|s| !s.is_empty()) {
        return Some(format!("{home}/.config/gcloud"));
    }
    if let Some(appdata) = io.env("APPDATA").await.filter(|s| !s.is_empty()) {
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
