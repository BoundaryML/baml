// `baml auth` — a human email login via WorkOS CLI Auth (`baml auth login`).
//
// This replaces the earlier agent-registration (auth.md) design: identity
// for feedback is now PostHog-first (see `feedback_command.rs`) — anonymous
// reporters get a locally generated PostHog distinct id, and logging in
// exists to attach a verified email to that id. `baml auth login` runs the
// OAuth 2.0 device authorization grant (RFC 8628): show a one-time code,
// the user confirms it in a browser, the CLI polls for tokens. On success,
// if an anonymous distinct id exists, a PostHog `$identify` event merges the
// anonymous person into the identified one — retroactively attributing all
// previously sent feedback.
//
// Credentials live in `~/.baml/creds.json` (0600). The PostHog distinct id
// survives logout so anonymous continuity is never lost.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

// WorkOS client id: baked in at build time from BAML_WORKOS_CLIENT_ID (set
// by release CI), overridable at runtime by the same variable. A public
// OAuth identifier, not a secret — but environment-specific, so it doesn't
// belong in source.
const BUILD_CLIENT_ID: Option<&str> = option_env!("BAML_WORKOS_CLIENT_ID");
const DEFAULT_API_DOMAIN: &str = "https://api.workos.com";

/// Hard cap on how long the device-authorization poll loop waits for the
/// user to confirm.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

/// Per-request cap so a black-holed network fails a login step instead of
/// stalling it indefinitely.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// A blocking HTTP client with [`REQUEST_TIMEOUT`] applied.
pub(crate) fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        // Falls back to default settings only if the builder ever fails,
        // which requires a broken TLS backend; a login attempt should still
        // proceed rather than abort here.
        .unwrap_or_default()
}

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// WorkOS API domain: hosts the CLI Auth device-authorization endpoints.
fn api_domain() -> String {
    env_or("BAML_WORKOS_API_DOMAIN", DEFAULT_API_DOMAIN)
}

/// Resolves the WorkOS client id from the runtime environment or the
/// build-time default. Blank values are treated as unset (CI expands a
/// missing repo variable to an empty string).
///
/// Errors:
/// - When neither source provides a non-blank value.
fn client_id() -> Result<String> {
    std::env::var("BAML_WORKOS_CLIENT_ID")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| {
            BUILD_CLIENT_ID
                .filter(|v| !v.trim().is_empty())
                .map(str::to_string)
        })
        .context(
            "This build has no WorkOS environment configured. Set \
             BAML_WORKOS_CLIENT_ID (release builds bake it in at compile time).",
        )
}

#[derive(Subcommand, Debug)]
pub(crate) enum AuthCommands {
    #[command(about = "Log in to Boundary with your email")]
    Login(LoginArgs),
    #[command(about = "Show the current identity")]
    Whoami(WhoamiArgs),
    #[command(about = "Log out (keeps your anonymous feedback id)")]
    Logout(LogoutArgs),
    #[command(about = "Print a valid access token", hide = true)]
    Token(TokenArgs),
}

impl AuthCommands {
    pub fn run(&self) -> Result<crate::ExitCode> {
        match self {
            AuthCommands::Login(args) => args.run(),
            AuthCommands::Whoami(args) => args.run(),
            AuthCommands::Logout(args) => args.run(),
            AuthCommands::Token(args) => args.run(),
        }
    }
}

/// `baml auth login` — device-code email login.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Log in using a browser:
    baml auth login

  Print the verification URL instead:
    baml auth login --no-open")]
pub(crate) struct LoginArgs {
    /// Print the verification URL instead of opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Args, Debug)]
#[command(after_long_help = "Examples:\n  Show the current identity:\n    baml auth whoami")]
pub(crate) struct WhoamiArgs {}

#[derive(Args, Debug)]
#[command(after_long_help = "Examples:\n  Log out:\n    baml auth logout")]
pub(crate) struct LogoutArgs {}

#[derive(Args, Debug)]
pub(crate) struct TokenArgs {}

impl LoginArgs {
    /// Runs `baml auth login`.
    ///
    /// Device-code login via WorkOS CLI Auth, then — when an anonymous
    /// PostHog distinct id exists — sends a `$identify` event so all
    /// feedback previously reported anonymously is merged into the now
    /// identified person.
    ///
    /// Returns:
    /// - `ExitCode::Success` once credentials are persisted.
    ///
    /// Errors:
    /// - When authorization can't be started, the user denies the login,
    ///   the code expires, or [`LOGIN_TIMEOUT`] elapses.
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = crate::reporter::Reporter::new();
        let mut existing = Credentials::read()?.unwrap_or_default();
        // Only skip the flow when the stored session can still produce a
        // token; a session that can't refresh falls through to a fresh
        // login instead of dead-ending against "already logged in".
        if let Some(email) = existing.user_email.clone() {
            if existing.access_token().is_ok() {
                existing.write()?;
                reporter.status("Login", format!("already logged in as {email}"));
                return Ok(crate::ExitCode::Success);
            }
            reporter.status(
                "Login",
                format!("the session for {email} has expired; logging in again"),
            );
        }
        let creds = device_login(self.no_open, existing)?;
        match creds.user_email.as_deref() {
            Some(email) => reporter.finish("Login", format!("logged in as {email}")),
            None => reporter.finish("Login", "logged in"),
        }
        Ok(crate::ExitCode::Success)
    }
}

/// Runs the device authorization flow and persists the resulting session,
/// preserving (and identifying) any anonymous PostHog distinct id carried in
/// `existing`.
///
/// Parameters:
/// - `no_open`: Print the verification URL instead of opening a browser.
/// - `existing`: Prior credential state; the PostHog distinct id (if any)
///   survives into the new session.
///
/// Returns:
/// - The persisted, logged-in credentials.
///
/// Errors:
/// - When authorization can't be started or the poll ends in denial,
///   expiry, or timeout.
pub(crate) fn device_login(no_open: bool, existing: Credentials) -> Result<Credentials> {
    let reporter = crate::reporter::Reporter::new();
    let client_id = client_id()?;
    let device: DeviceAuthResponse = post_form(
        &format!("{}/user_management/authorize/device", api_domain()),
        &[("client_id", client_id.as_str())],
    )
    .context("Failed to start device authorization")?;

    reporter.status(
        "Login",
        format!("first, copy your one-time code: {}", device.user_code),
    );
    let open_uri = device
        .verification_uri_complete
        .as_deref()
        .unwrap_or(&device.verification_uri);
    if no_open {
        reporter.status(
            "Login",
            format!("then log in at: {}", device.verification_uri),
        );
    } else {
        reporter.status("Login", "then confirm the login in your browser...");
        if webbrowser::open(open_uri).is_err() {
            reporter.status(
                "Login",
                format!(
                    "could not open a browser; log in at: {}",
                    device.verification_uri
                ),
            );
        }
    }
    reporter.status(
        "Waiting",
        format!(
            "for confirmation (times out in {} minutes, ctrl-c to cancel)",
            LOGIN_TIMEOUT.as_secs() / 60
        ),
    );

    let tokens = poll_token_endpoint(
        &format!("{}/user_management/authenticate", api_domain()),
        &[
            ("grant_type", DEVICE_GRANT_TYPE),
            ("client_id", &client_id),
            ("device_code", &device.device_code),
        ],
        device.interval,
    )?;

    let user = tokens.user.as_ref();
    let creds = Credentials {
        // Seed the distinct id when missing so the `$identify` below always
        // merges: one PostHog person per machine whether feedback happens
        // before or after the first login.
        posthog_distinct_id: existing
            .posthog_distinct_id
            .clone()
            .or_else(|| Some(uuid::Uuid::new_v4().to_string())),
        user_id: user.and_then(|u| u.id.clone()),
        user_email: user.and_then(|u| u.email.clone()),
        access_token: Some(tokens.access_token),
        expires_at: tokens.expires_in.map(|s| now_unix() + s),
        refresh_token: tokens.refresh_token,
    };
    creds.write()?;

    // Retroactive attribution: merge the anonymous person (all feedback sent
    // before login) into the identified one. Best-effort — a failed identify
    // never fails the login; the next one retries the merge.
    crate::feedback_command::identify(&creds);

    Ok(creds)
}

impl WhoamiArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        match Credentials::read()? {
            Some(creds) if creds.user_email.is_some() => {
                println!(
                    "logged in as {}",
                    creds.user_email.as_deref().unwrap_or("<unknown>")
                );
                Ok(crate::ExitCode::Success)
            }
            Some(creds) if creds.posthog_distinct_id.is_some() => {
                println!(
                    "anonymous (feedback id {}); run `baml auth login` to attach your email",
                    creds.posthog_distinct_id.as_deref().unwrap_or("<unknown>")
                );
                Ok(crate::ExitCode::Success)
            }
            _ => {
                println!("not logged in");
                Ok(crate::ExitCode::Other)
            }
        }
    }
}

impl LogoutArgs {
    /// Removes the login session. The PostHog distinct id is preserved so
    /// anonymous feedback continuity — and a future re-login's retroactive
    /// attribution — still work.
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = crate::reporter::Reporter::new();
        match Credentials::read()? {
            None => reporter.status("Logout", "not logged in"),
            Some(creds) if creds.user_email.is_none() && creds.access_token.is_none() => {
                reporter.status("Logout", "not logged in");
            }
            Some(creds) => {
                let anonymous = Credentials {
                    posthog_distinct_id: creds.posthog_distinct_id,
                    ..Credentials::default()
                };
                anonymous.write()?;
                reporter.status("Logout", "logged out");
            }
        }
        Ok(crate::ExitCode::Success)
    }
}

impl TokenArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = crate::reporter::Reporter::new();
        let mut creds = match Credentials::read()? {
            Some(creds) => creds,
            None => return Ok(reporter.fatal("not logged in; run `baml auth login`")),
        };
        // The token itself is this command's data output (stdout); auth
        // failures are fatal diagnostics.
        match creds.access_token() {
            Ok(token) => println!("{token}"),
            Err(e) => return Ok(reporter.fatal(e)),
        }
        creds.write()?;
        Ok(crate::ExitCode::Success)
    }
}

// ---------------------------------------------------------------------------
// Device authorization
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct DeviceAuthResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    verification_uri_complete: Option<String>,
    /// Suggested polling interval in seconds.
    interval: Option<u64>,
}

/// Polls a token endpoint until the pending grant resolves.
///
/// Speaks the RFC 8628 error vocabulary: `authorization_pending` sleeps the
/// current interval, `slow_down` widens it by five seconds, and
/// `access_denied` / `expired_token` are terminal.
///
/// Errors:
/// - When the user denies the login, the code expires, an unrecognized
///   error is returned, or [`LOGIN_TIMEOUT`] elapses.
fn poll_token_endpoint(
    endpoint: &str,
    form: &[(&str, &str)],
    server_interval: Option<u64>,
) -> Result<TokenResponse> {
    let client = http_client();
    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    let mut interval = server_interval
        .map(Duration::from_secs)
        .unwrap_or(DEFAULT_POLL_INTERVAL);
    loop {
        if std::time::Instant::now() >= deadline {
            anyhow::bail!(
                "Timed out after {} minutes waiting for the login to be confirmed.",
                LOGIN_TIMEOUT.as_secs() / 60
            );
        }
        let resp = client
            .post(endpoint)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(encode_form(form))
            .send()
            .context("Failed to reach the auth server")?;
        let status = resp.status();
        let value: serde_json::Value = resp
            .json()
            .context("Failed to parse token endpoint response")?;

        if status.is_success() {
            return serde_json::from_value(value).context("Failed to parse token response");
        }

        let error = value.get("error").and_then(|e| e.as_str()).unwrap_or("");
        match error {
            "authorization_pending" => std::thread::sleep(interval),
            "slow_down" => {
                interval += Duration::from_secs(5);
                std::thread::sleep(interval);
            }
            "access_denied" => anyhow::bail!("Login was denied in the browser."),
            "expired_token" => anyhow::bail!(
                "the confirmation code expired before it was used; run `baml auth login` again"
            ),
            _ => anyhow::bail!("Auth server returned {status}: {value}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Request plumbing
// ---------------------------------------------------------------------------

/// WorkOS authenticate response. Field presence varies by grant, so
/// everything but `access_token` is optional.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    user: Option<TokenUser>,
}

#[derive(Debug, Deserialize)]
struct TokenUser {
    id: Option<String>,
    email: Option<String>,
}

/// POSTs a form-encoded body and deserializes a successful JSON response.
///
/// Errors:
/// - On network failure, a non-success status (the response body is
///   included in the error), or a body that fails to deserialize as `T`.
fn post_form<T: serde::de::DeserializeOwned>(url: &str, form: &[(&str, &str)]) -> Result<T> {
    let client = http_client();
    let resp = client
        .post(url)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(encode_form(form))
        .send()
        .context("Failed to reach the auth server")?;
    let status = resp.status();
    if !status.is_success() {
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("Auth server returned {status}: {body}");
    }
    resp.json().context("Failed to parse auth server response")
}

fn encode_form(form: &[(&str, &str)]) -> String {
    form.iter()
        .map(|(k, v)| format!("{k}={}", form_urlencode(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encodes a form value.
///
/// RFC 3986 unreserved characters pass through; every other byte is
/// `%XX`-encoded.
fn form_urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Credential storage: ~/.baml/creds.json
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Credentials {
    /// The PostHog distinct id used for feedback events. Generated locally
    /// on first anonymous feedback; survives logout so continuity and
    /// later retroactive attribution keep working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posthog_distinct_id: Option<String>,
    /// WorkOS user id, once logged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Verified email, once logged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
    /// Cached WorkOS access token (absent while anonymous).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    access_token: Option<String>,
    /// Unix seconds; absent means unknown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    expires_at: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    refresh_token: Option<String>,
}

/// Writes a file readable only by the owner (0600 on unix; other
/// platforms fall back to default permissions). Shared by every store
/// that persists identity data (`creds.json`, `feedback.json`).
pub(crate) fn write_owner_only(path: &std::path::Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::{
            io::Write as _,
            os::unix::fs::{OpenOptionsExt, PermissionsExt},
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
        file.write_all(content.as_bytes())
            .with_context(|| format!("Failed to write {}", path.display()))?;
    }
    #[cfg(not(unix))]
    std::fs::write(path, content).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn creds_path() -> Result<PathBuf> {
    Ok(baml_release::baml_home().join("creds.json"))
}

pub(crate) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Credentials {
    /// Reads stored credentials from `~/.baml/creds.json`.
    ///
    /// Returns:
    /// - `Ok(None)` when no credentials file exists — a normal state, not
    ///   an error.
    ///
    /// Errors:
    /// - When the file exists but cannot be read or parsed.
    pub fn read() -> Result<Option<Self>> {
        let path = creds_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read {}", path.display()))?;
        let creds = serde_json::from_str(&content)
            .with_context(|| format!("Malformed credentials in {}", path.display()))?;
        Ok(Some(creds))
    }

    /// Persists credentials to `~/.baml/creds.json` with owner-only access.
    ///
    /// On Unix the file is created with mode 0600 before any bytes are
    /// written; there is never a window where the contents are readable by
    /// other users.
    pub fn write(&self) -> Result<()> {
        let path = creds_path()?;
        write_owner_only(&path, &serde_json::to_string_pretty(self)?)
    }

    /// Returns a valid access token, refreshing via the OAuth refresh-token
    /// grant when near expiry. Callers persist afterwards if they want the
    /// refreshed state kept.
    ///
    /// Errors:
    /// - When not logged in, or the session is expired and cannot be
    ///   refreshed.
    pub fn access_token(&mut self) -> Result<&str> {
        if self.access_token.is_none() {
            anyhow::bail!("not logged in; run `baml auth login`");
        }
        let expired = match self.expires_at {
            Some(at) => at <= now_unix() + 30,
            // Unknown expiry: refresh when we can, rather than trusting a
            // token we can't validate.
            None => self.refresh_token.is_some(),
        };
        if expired {
            let refresh = self
                .refresh_token
                .as_deref()
                .context("session expired; run `baml auth login` again")?;
            let tokens: TokenResponse = post_form(
                &format!("{}/user_management/authenticate", api_domain()),
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", &client_id()?),
                    ("refresh_token", refresh),
                ],
            )
            .context("failed to refresh session; run `baml auth login` again")?;
            self.access_token = Some(tokens.access_token);
            self.expires_at = tokens.expires_in.map(|s| now_unix() + s);
            if tokens.refresh_token.is_some() {
                self.refresh_token = tokens.refresh_token;
            }
        }
        Ok(self.access_token.as_deref().expect("checked above"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_urlencode_passes_unreserved_and_escapes_the_rest() {
        assert_eq!(form_urlencode("abc-XYZ_0.9~"), "abc-XYZ_0.9~");
        assert_eq!(
            form_urlencode("urn:ietf:params:oauth:grant-type:device_code"),
            "urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        );
    }

    #[test]
    fn credentials_serialize_roundtrip_and_skip_absent_fields() {
        let creds = Credentials {
            posthog_distinct_id: Some("ph-uuid".into()),
            ..Credentials::default()
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("ph-uuid"), "{json}");
        assert!(
            !json.contains("access_token"),
            "absent fields skipped: {json}"
        );
        let back: Credentials = serde_json::from_str(&json).unwrap();
        assert_eq!(back.posthog_distinct_id.as_deref(), Some("ph-uuid"));
        assert!(back.user_email.is_none());
    }
}
