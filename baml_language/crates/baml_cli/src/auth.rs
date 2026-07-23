// `baml auth` — anonymous-first authentication with Boundary via WorkOS
// AuthKit agent registration (auth.md).
//
// The model inverts the usual order: work immediately, attach identity later.
//
// - `baml login`: anonymous start. Registers an anonymous agent identity
//   (`POST /agent/identity {type: anonymous}`), exchanges the identity
//   assertion for a short-lived access token via the jwt-bearer grant, and
//   uses the registration id (the assertion's `sub`) as the project id the
//   user can stream data into before any authentication. The claim token is
//   held for later.
// - `baml auth login`: the claim/upgrade. With an anonymous registration
//   present, runs the auth.md claim ceremony: mint a claim attempt, send the
//   user to its verification page (they sign in there and the page shows
//   them a code), read the code back, and complete the claim — the
//   registration and everything streamed into it now belong to that user.
//   With nothing to claim it falls back to a plain human login via WorkOS
//   CLI Auth (OAuth 2.0 device authorization grant, RFC 8628).
//
// Credentials live in `~/.baml/creds.json` (0600) with a lifecycle:
// `anonymous` → `claimed`. Access tokens are ~5-minute and are re-minted
// from the stored identity assertion on expiry; the assertion itself is
// rotated via `{type: refresh}` when it expires.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use base64::Engine as _;
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

// WorkOS environment configuration. The AuthKit domain and client ID are
// baked in at build time from BAML_WORKOS_AUTHKIT_DOMAIN /
// BAML_WORKOS_CLIENT_ID (set by release CI), and the same variables
// override at runtime for non-production environments. Neither is a secret
// (client IDs are public OAuth identifiers; no grant the CLI uses involves
// a client secret), but the values are environment-specific so they don't
// belong in source. The WorkOS API domain is their public API and is only
// runtime-overridable for tests.
const BUILD_AUTHKIT_DOMAIN: Option<&str> = option_env!("BAML_WORKOS_AUTHKIT_DOMAIN");
const BUILD_CLIENT_ID: Option<&str> = option_env!("BAML_WORKOS_CLIENT_ID");
const DEFAULT_API_DOMAIN: &str = "https://api.workos.com";

/// Hard cap on how long `baml auth login` waits during interactive steps.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_secs(5);

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";
const JWT_BEARER_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

fn env_or(var: &str, default: &str) -> String {
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| default.to_string())
}

/// Resolves a required configuration value from the environment.
///
/// Parameters:
/// - `var`: Environment variable name, consulted at runtime first.
/// - `build_default`: Compile-time fallback captured via `option_env!` when
///   release CI sets the same variable during the build.
///
/// Returns:
/// - The runtime value when set and non-blank, otherwise the build-time
///   value.
///
/// Errors:
/// - When neither source provides a value, with a message naming the
///   variable to set.
fn required(var: &str, build_default: Option<&str>) -> Result<String> {
    std::env::var(var)
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| build_default.map(str::to_string))
        .with_context(|| {
            format!(
                "This build has no WorkOS environment configured. Set {var} \
                 (release builds bake it in at compile time)."
            )
        })
}

/// AuthKit domain: hosts auth.md discovery, the agent identity/claim
/// endpoints, the token endpoint, and userinfo.
fn authkit_domain() -> Result<String> {
    required("BAML_WORKOS_AUTHKIT_DOMAIN", BUILD_AUTHKIT_DOMAIN)
}

/// WorkOS API domain: hosts the CLI Auth device-authorization endpoints
/// used for plain human logins.
fn api_domain() -> String {
    env_or("BAML_WORKOS_API_DOMAIN", DEFAULT_API_DOMAIN)
}

fn client_id() -> Result<String> {
    required("BAML_WORKOS_CLIENT_ID", BUILD_CLIENT_ID)
}

#[derive(Subcommand, Debug)]
pub(crate) enum AuthCommands {
    #[command(about = "Claim this machine's anonymous project with your account")]
    Login(ClaimLoginArgs),
    #[command(about = "Remove stored credentials")]
    Logout(LogoutArgs),
    #[command(about = "Show the current identity (anonymous project or user)")]
    Whoami(WhoamiArgs),
    #[command(about = "Print a valid access token", hide = true)]
    Token(TokenArgs),
}

impl AuthCommands {
    pub fn run(&self) -> Result<crate::ExitCode> {
        match self {
            AuthCommands::Login(args) => args.run(),
            AuthCommands::Logout(args) => args.run(),
            AuthCommands::Whoami(args) => args.run(),
            AuthCommands::Token(args) => args.run(),
        }
    }
}

/// `baml login` — the anonymous start.
#[derive(Args, Debug)]
pub(crate) struct LoginArgs {}

/// `baml auth login` — claim the anonymous project (or plain human login).
#[derive(Args, Debug)]
pub(crate) struct ClaimLoginArgs {
    /// Print verification URLs instead of opening a browser.
    #[arg(long)]
    pub no_open: bool,

    /// Email hint for the claim ceremony sign-in page.
    #[arg(long, value_name = "EMAIL")]
    pub email: Option<String>,
}

#[derive(Args, Debug)]
pub(crate) struct LogoutArgs {}

#[derive(Args, Debug)]
pub(crate) struct WhoamiArgs {}

#[derive(Args, Debug)]
pub(crate) struct TokenArgs {}

impl LoginArgs {
    /// Runs `baml login`: the anonymous start.
    ///
    /// Registers an anonymous agent identity, exchanges its assertion for a
    /// project-scoped access token, and persists credentials whose project
    /// id is the registration id (the assertion's `sub` claim). Idempotent:
    /// with credentials already present it reports them instead of
    /// registering again.
    ///
    /// Returns:
    /// - `ExitCode::Success` on registration or when credentials already
    ///   exist.
    ///
    /// Errors:
    /// - When the environment is unconfigured, or registration or the token
    ///   exchange fails.
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        if let Some(creds) = Credentials::read()? {
            match creds.status {
                CredStatus::Anonymous => {
                    println!(
                        "Already started: anonymous project {}.",
                        creds.project_id.as_deref().unwrap_or("<unknown>")
                    );
                    println!("Run `baml auth login` to claim it with your account.");
                }
                CredStatus::Claimed => {
                    println!(
                        "Already logged in{}.",
                        creds
                            .user_email
                            .as_deref()
                            .map(|e| format!(" as {e}"))
                            .unwrap_or_default()
                    );
                }
            }
            return Ok(crate::ExitCode::Success);
        }

        let endpoints = discover_endpoints()?;

        // Step 1: anonymous registration.
        let registration: RegistrationResponse = post_json(
            &endpoints.identity_endpoint,
            &serde_json::json!({ "type": "anonymous" }),
            None,
        )
        .context("Anonymous registration failed")?;

        // The registration id is the assertion's `sub` — it doubles as the
        // project id the user streams data into.
        let project_id = jwt_claim(&registration.identity.assertion, "sub");

        // Step 2: exchange the assertion for a short-lived access token to
        // confirm the registration is usable (and cache it).
        let tokens = exchange_assertion(&endpoints, &registration.identity.assertion)
            .context("Failed to exchange the anonymous registration for a token")?;

        let creds = Credentials {
            status: CredStatus::Anonymous,
            project_id: project_id.clone(),
            assertion: Some(registration.identity.assertion),
            assertion_refresh_token: registration.identity.refresh_token.map(|t| t.value),
            claim_token: registration.claim.map(|c| c.token),
            access_token: tokens.access_token,
            expires_at: tokens.expires_in.map(|s| now_unix() + s),
            refresh_token: tokens.refresh_token,
            user_email: None,
        };
        creds.write()?;

        println!(
            "Started anonymous project {}.",
            project_id.as_deref().unwrap_or("<unknown>")
        );
        println!("You can stream data into it right away — no login needed.");
        println!("When you're ready, claim it with `baml auth login`.");
        Ok(crate::ExitCode::Success)
    }
}

impl ClaimLoginArgs {
    /// Runs `baml auth login`: claim the anonymous project, or plain login.
    ///
    /// Dispatches on stored credentials: already claimed is a no-op;
    /// anonymous credentials with a claim token run the claim ceremony;
    /// anything else falls back to a device-code human login.
    ///
    /// Returns:
    /// - `ExitCode::Success` in every non-error path.
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let existing = Credentials::read()?;
        match existing {
            Some(creds) if matches!(creds.status, CredStatus::Claimed) => {
                println!(
                    "Already logged in{}.",
                    creds
                        .user_email
                        .as_deref()
                        .map(|e| format!(" as {e}"))
                        .unwrap_or_default()
                );
                Ok(crate::ExitCode::Success)
            }
            Some(creds) if creds.claim_token.is_some() => self.run_claim_ceremony(creds),
            _ => self.run_device_login(),
        }
    }

    /// Runs the auth.md claim ceremony for an anonymous registration.
    ///
    /// Mints a claim attempt, sends the user to its verification page (they
    /// sign in there and the page shows a code), prompts for the code in
    /// the terminal, and completes the claim. Wrong-code and
    /// not-yet-confirmed responses re-prompt instead of failing.
    ///
    /// Parameters:
    /// - `creds`: The stored anonymous credentials; must carry a
    ///   `claim_token` (guaranteed by the caller).
    ///
    /// Returns:
    /// - `ExitCode::Success` once the claim completes and upgraded
    ///   credentials are persisted.
    ///
    /// Errors:
    /// - When the attempt cannot be started (e.g. expired or already-claimed
    ///   token), the terminal is non-interactive, or the post-claim token
    ///   exchange fails.
    #[allow(clippy::print_stdout)]
    fn run_claim_ceremony(&self, creds: Credentials) -> Result<crate::ExitCode> {
        let endpoints = discover_endpoints()?;
        let claim_token = creds.claim_token.clone().expect("checked by caller");

        // The claim attempt requires a login hint; prompt when --email was
        // not given rather than letting the server reject the body.
        let email = match &self.email {
            Some(email) => email.clone(),
            None => loop {
                let entered = prompt("Email to sign in with: ")?;
                if !entered.is_empty() {
                    break entered;
                }
            },
        };
        let attempt: ClaimAttemptResponse = post_json(
            &endpoints.claim_endpoint,
            &serde_json::json!({
                "type": "service_auth",
                "claim_token": claim_token,
                "login_hint": email,
            }),
            None,
        )
        .context("Failed to start the claim ceremony")?;

        println!("To claim this project, sign in and read the code off the page.");
        if self.no_open {
            println!("Open: {}", attempt.attempt.verification_uri);
        } else {
            println!("Opening your browser...");
            if webbrowser::open(&attempt.attempt.verification_uri).is_err() {
                println!(
                    "Could not open a browser. Open: {}",
                    attempt.attempt.verification_uri
                );
            }
        }

        // The user reads the code off the page and types it here.
        let complete_url = format!("{}/complete", endpoints.claim_endpoint);
        let verified = loop {
            let user_code = prompt("Enter the code shown on the page: ")?;
            if user_code.is_empty() {
                continue;
            }
            let result: Result<RegistrationResponse> = post_json(
                &complete_url,
                &serde_json::json!({
                    "claim_token": claim_token,
                    "user_code": user_code,
                }),
                None,
            );
            match result {
                Ok(verified) => break verified,
                Err(err) => {
                    let msg = err.to_string();
                    if msg.contains("claim_not_confirmed") {
                        println!(
                            "The page hasn't confirmed yet — finish signing in, then try again."
                        );
                    } else if msg.contains("invalid_user_code") {
                        println!("That code didn't match — check the page and try again.");
                    } else {
                        return Err(err.context("Failed to complete the claim"));
                    }
                }
            }
        };

        // Trade the verified assertion for a token; persist the upgrade.
        let endpoints = discover_endpoints()?;
        let tokens = exchange_assertion(&endpoints, &verified.identity.assertion)
            .context("Claim succeeded but the token exchange failed")?;

        let project_id = creds
            .project_id
            .clone()
            .or_else(|| jwt_claim(&verified.identity.assertion, "sub"));
        let user_email = jwt_claim(&verified.identity.assertion, "email").or(Some(email));
        let new_creds = Credentials {
            status: CredStatus::Claimed,
            project_id: project_id.clone(),
            assertion: Some(verified.identity.assertion),
            assertion_refresh_token: verified.identity.refresh_token.map(|t| t.value),
            claim_token: None,
            access_token: tokens.access_token,
            expires_at: tokens.expires_in.map(|s| now_unix() + s),
            refresh_token: tokens.refresh_token,
            user_email,
        };
        new_creds.write()?;

        match project_id {
            Some(id) => println!("Claimed. Project {id} and its data now belong to your account."),
            None => println!("Claimed."),
        }
        Ok(crate::ExitCode::Success)
    }

    /// Runs a plain human login when there is no registration to claim.
    ///
    /// Uses WorkOS CLI Auth — the OAuth 2.0 device authorization grant
    /// (RFC 8628): show a one-time code, open the verification page, and
    /// poll the authenticate endpoint until the user confirms.
    ///
    /// Returns:
    /// - `ExitCode::Success` once tokens are persisted as claimed
    ///   credentials.
    ///
    /// Errors:
    /// - When authorization can't be started, the user denies the login,
    ///   the code expires, or [`LOGIN_TIMEOUT`] elapses.
    #[allow(clippy::print_stdout)]
    fn run_device_login(&self) -> Result<crate::ExitCode> {
        let client_id = client_id()?;
        let device: DeviceAuthResponse = post_form(
            &format!("{}/user_management/authorize/device", api_domain()),
            &[("client_id", client_id.as_str())],
        )
        .context("Failed to start device authorization")?;

        println!("First, copy your one-time code: {}", device.user_code);
        let open_uri = device
            .verification_uri_complete
            .as_deref()
            .unwrap_or(&device.verification_uri);
        if self.no_open {
            println!("Then log in at: {}", device.verification_uri);
        } else {
            println!("Then confirm the login in your browser...");
            if webbrowser::open(open_uri).is_err() {
                println!(
                    "Could not open a browser. Log in at: {}",
                    device.verification_uri
                );
            }
        }
        println!(
            "Waiting for confirmation... (times out in {} minutes, Ctrl-C to cancel)",
            LOGIN_TIMEOUT.as_secs() / 60
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

        let creds = Credentials {
            status: CredStatus::Claimed,
            project_id: None,
            assertion: None,
            assertion_refresh_token: None,
            claim_token: None,
            user_email: tokens.user.as_ref().and_then(|u| u.email.clone()),
            access_token: tokens.access_token,
            expires_at: tokens.expires_in.map(|s| now_unix() + s),
            refresh_token: tokens.refresh_token,
        };
        creds.write()?;

        match creds.user_email.as_deref() {
            Some(email) => println!("Logged in as {email}."),
            None => println!("Logged in."),
        }
        Ok(crate::ExitCode::Success)
    }
}

impl LogoutArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let path = creds_path()?;
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("Failed to remove {}", path.display()))?;
            println!("Logged out.");
        } else {
            println!("Not logged in.");
        }
        Ok(crate::ExitCode::Success)
    }
}

impl WhoamiArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let mut creds = match Credentials::read()? {
            Some(creds) => creds,
            None => {
                println!("Not logged in. Run `baml login` to start.");
                return Ok(crate::ExitCode::Other);
            }
        };
        let _ = creds.access_token()?;
        match creds.status {
            CredStatus::Anonymous => println!(
                "Anonymous (project {}). Run `baml auth login` to claim it.",
                creds.project_id.as_deref().unwrap_or("<unknown>")
            ),
            CredStatus::Claimed => println!(
                "Logged in{}{}.",
                creds
                    .user_email
                    .as_deref()
                    .map(|e| format!(" as {e}"))
                    .unwrap_or_default(),
                creds
                    .project_id
                    .as_deref()
                    .map(|p| format!(" (project {p})"))
                    .unwrap_or_default()
            ),
        }
        Ok(crate::ExitCode::Success)
    }
}

impl TokenArgs {
    #[allow(clippy::print_stdout, clippy::print_stderr)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let mut creds = match Credentials::read()? {
            Some(creds) => creds,
            None => {
                eprintln!("Not logged in. Run `baml login` to start.");
                return Ok(crate::ExitCode::Other);
            }
        };
        println!("{}", creds.access_token()?);
        Ok(crate::ExitCode::Success)
    }
}

// ---------------------------------------------------------------------------
// Endpoint discovery (RFC 8414 + auth.md `agent_auth` extension)
// ---------------------------------------------------------------------------

struct Endpoints {
    identity_endpoint: String,
    claim_endpoint: String,
    token_endpoint: String,
}

/// Resolves agent endpoints from RFC 8414 authorization-server metadata.
///
/// Reads the auth.md `agent_auth` extension block for the identity and
/// claim endpoints plus the standard `token_endpoint`.
///
/// Returns:
/// - Endpoints from the metadata when available; conventional paths under
///   the AuthKit domain on any fetch or parse failure — discovery is an
///   optimization, not a gate.
///
/// Errors:
/// - Only when the AuthKit domain itself is unconfigured.
fn discover_endpoints() -> Result<Endpoints> {
    let domain = authkit_domain()?;
    let fallback = Endpoints {
        identity_endpoint: format!("{domain}/agent/identity"),
        claim_endpoint: format!("{domain}/agent/identity/claim"),
        token_endpoint: format!("{domain}/oauth2/token"),
    };
    let client = reqwest::blocking::Client::new();
    let resp = match client
        .get(format!("{domain}/.well-known/oauth-authorization-server"))
        .send()
    {
        Ok(resp) if resp.status().is_success() => resp,
        _ => return Ok(fallback),
    };
    let meta: serde_json::Value = match resp.json() {
        Ok(meta) => meta,
        Err(_) => return Ok(fallback),
    };
    let str_at =
        |v: &serde_json::Value, key: &str| v.get(key).and_then(|s| s.as_str()).map(str::to_string);
    let agent = meta.get("agent_auth");
    Ok(Endpoints {
        identity_endpoint: agent
            .and_then(|a| str_at(a, "identity_endpoint"))
            .unwrap_or(fallback.identity_endpoint),
        claim_endpoint: agent
            .and_then(|a| str_at(a, "claim_endpoint"))
            .unwrap_or(fallback.claim_endpoint),
        token_endpoint: str_at(&meta, "token_endpoint").unwrap_or(fallback.token_endpoint),
    })
}

// ---------------------------------------------------------------------------
// auth.md agent registration shapes (as served, nested)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct RegistrationResponse {
    identity: IdentityBlock,
    #[serde(default)]
    claim: Option<ClaimBlock>,
}

#[derive(Debug, Deserialize)]
struct IdentityBlock {
    assertion: String,
    #[serde(default)]
    refresh_token: Option<RefreshTokenBlock>,
}

#[derive(Debug, Deserialize)]
struct RefreshTokenBlock {
    value: String,
}

#[derive(Debug, Deserialize)]
struct ClaimBlock {
    token: String,
}

#[derive(Debug, Deserialize)]
struct ClaimAttemptResponse {
    attempt: ClaimAttemptBlock,
}

#[derive(Debug, Deserialize)]
struct ClaimAttemptBlock {
    verification_uri: String,
}

/// Exchanges an identity assertion for an access token.
///
/// Parameters:
/// - `endpoints`: Resolved agent endpoints; the token endpoint is used.
/// - `assertion`: The auth.md identity assertion JWT.
///
/// Returns:
/// - The token response from the jwt-bearer grant (RFC 7523).
///
/// Errors:
/// - On network failure or a non-success response (e.g. `invalid_grant`
///   when the assertion has expired or been revoked).
fn exchange_assertion(endpoints: &Endpoints, assertion: &str) -> Result<TokenResponse> {
    post_form(
        &endpoints.token_endpoint,
        &[
            ("grant_type", JWT_BEARER_GRANT_TYPE),
            ("assertion", assertion),
        ],
    )
}

/// Rotates an expired or near-expiry identity assertion.
///
/// Parameters:
/// - `endpoints`: Resolved agent endpoints; the identity endpoint is used.
/// - `refresh_token`: The registration's assertion refresh token.
///
/// Returns:
/// - A fresh registration response carrying the rotated assertion (and
///   possibly a rotated refresh token).
///
/// Errors:
/// - On network failure or when the refresh token itself is expired or
///   revoked.
fn refresh_assertion(endpoints: &Endpoints, refresh_token: &str) -> Result<RegistrationResponse> {
    post_json(
        &endpoints.identity_endpoint,
        &serde_json::json!({ "type": "refresh", "refresh_token": refresh_token }),
        None,
    )
}

/// Reads a claim from a JWT's payload without verifying the signature.
///
/// Signature verification is deliberately skipped: the values are used only
/// for display and local bookkeeping, and the server remains the verifier
/// of the token itself.
///
/// Parameters:
/// - `jwt`: A compact-serialized JWT.
/// - `claim`: The payload claim name to read.
///
/// Returns:
/// - The claim's string value, or `None` when the token is malformed or
///   the claim is absent or non-string.
fn jwt_claim(jwt: &str, claim: &str) -> Option<String> {
    let payload = jwt.split('.').nth(1)?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    value.get(claim)?.as_str().map(str::to_string)
}

// ---------------------------------------------------------------------------
// Device authorization (plain human login)
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
/// Parameters:
/// - `endpoint`: The token/authenticate endpoint to poll.
/// - `form`: Form pairs sent on every poll (grant type, client id, code).
/// - `server_interval`: Server-suggested polling interval in seconds;
///   [`DEFAULT_POLL_INTERVAL`] when absent.
///
/// Returns:
/// - The token response once the user confirms.
///
/// Errors:
/// - When the user denies the login, the code expires, an unrecognized
///   error is returned, or [`LOGIN_TIMEOUT`] elapses.
fn poll_token_endpoint(
    endpoint: &str,
    form: &[(&str, &str)],
    server_interval: Option<u64>,
) -> Result<TokenResponse> {
    let client = reqwest::blocking::Client::new();
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
                "The confirmation code expired before it was used. Run `baml auth login` again."
            ),
            _ => anyhow::bail!("Auth server returned {status}: {value}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Request plumbing
// ---------------------------------------------------------------------------

/// Token response. Field presence varies by grant, so everything but
/// `access_token` is optional.
#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    user: Option<TokenUser>,
}

#[derive(Debug, Deserialize)]
struct TokenUser {
    email: Option<String>,
}

/// POSTs a form-encoded body and deserializes a successful JSON response.
///
/// Parameters:
/// - `url`: The endpoint to POST to.
/// - `form`: Key/value pairs; values are percent-encoded, keys are trusted
///   literals.
///
/// Errors:
/// - On network failure, a non-success status (the response body is
///   included in the error so callers can match server error codes), or a
///   body that fails to deserialize as `T`.
fn post_form<T: serde::de::DeserializeOwned>(url: &str, form: &[(&str, &str)]) -> Result<T> {
    let client = reqwest::blocking::Client::new();
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

/// POSTs a JSON body and deserializes a successful JSON response.
///
/// Parameters:
/// - `url`: The endpoint to POST to.
/// - `body`: The JSON request body.
/// - `bearer`: Optional access token sent as a `Bearer` authorization
///   header.
///
/// Errors:
/// - On network failure, a non-success status (the response body is
///   included so callers can match auth.md error codes like
///   `claim_not_confirmed`), or a body that fails to deserialize as `T`.
fn post_json<T: serde::de::DeserializeOwned>(
    url: &str,
    body: &serde_json::Value,
    bearer: Option<&str>,
) -> Result<T> {
    let client = reqwest::blocking::Client::new();
    let mut req = client.post(url).json(body);
    if let Some(token) = bearer {
        req = req.bearer_auth(token);
    }
    let resp = req.send().context("Failed to reach the auth server")?;
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

/// Prints a prompt and reads one trimmed line from stdin.
///
/// Errors:
/// - When stdin cannot be read (e.g. a non-interactive session).
#[allow(clippy::print_stdout)]
fn prompt(message: &str) -> Result<String> {
    use std::io::Write as _;
    print!("{message}");
    std::io::stdout().flush().ok();
    let mut line = String::new();
    let bytes_read = std::io::stdin()
        .read_line(&mut line)
        .context("Failed to read input")?;
    // EOF (closed or exhausted stdin, e.g. a piped invocation) would
    // otherwise return an empty line forever and spin the retry loops that
    // call this; fail instead so non-interactive callers get a clean error.
    if bytes_read == 0 {
        anyhow::bail!(
            "Standard input closed before a response was entered. \
             This command needs an interactive terminal."
        );
    }
    Ok(line.trim().to_string())
}

// ---------------------------------------------------------------------------
// Credential storage: ~/.baml/creds.json, lifecycle anonymous → claimed
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum CredStatus {
    Anonymous,
    Claimed,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct Credentials {
    status: CredStatus,
    /// The agent registration id (the assertion's `sub`); doubles as the
    /// project id data streams into.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    project_id: Option<String>,
    /// The auth.md identity assertion (absent for plain device logins).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assertion: Option<String>,
    /// Rotates the assertion via `{type: refresh}` when it expires.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    assertion_refresh_token: Option<String>,
    /// Held while anonymous so the project can be claimed later.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    claim_token: Option<String>,
    /// Cached short-lived access token.
    access_token: String,
    /// Unix seconds; absent means unknown (treated as never-expiring).
    expires_at: Option<u64>,
    /// OAuth refresh token (plain device logins only).
    refresh_token: Option<String>,
    user_email: Option<String>,
}

fn creds_path() -> Result<PathBuf> {
    Ok(baml_release::baml_home().join("creds.json"))
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Credentials {
    /// Reads stored credentials from `~/.baml/creds.json`.
    ///
    /// Returns:
    /// - `Ok(None)` when no credentials file exists — the normal
    ///   not-logged-in state, not an error.
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
    /// written; there is never a window where the tokens are readable by
    /// other users.
    ///
    /// Errors:
    /// - When the directory cannot be created or the file cannot be
    ///   written.
    pub fn write(&self) -> Result<()> {
        let path = creds_path()?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)?;
        // Create with 0600 from the first byte — chmod-after-write would
        // leave a window where the tokens are world-readable, and this
        // rewrite happens on every token re-mint, not just login.
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
                .open(&path)
                .with_context(|| format!("Failed to write {}", path.display()))?;
            // An existing file keeps its old mode; enforce ours either way.
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
            file.write_all(content.as_bytes())
                .with_context(|| format!("Failed to write {}", path.display()))?;
        }
        #[cfg(not(unix))]
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write {}", path.display()))?;
        Ok(())
    }

    /// Returns a valid access token, re-minting when near expiry.
    ///
    /// Agent sessions re-exchange the stored identity assertion (rotating
    /// it via `{type: refresh}` if the exchange reports it expired); plain
    /// device-login sessions use the OAuth refresh-token grant. A re-minted
    /// token that comes back user-bound while the session is anonymous
    /// means the registration was claimed elsewhere (e.g. the playground):
    /// the stored status flips to claimed. Refreshed state is re-persisted.
    ///
    /// Returns:
    /// - A borrowed access token valid for at least ~30 seconds.
    ///
    /// Errors:
    /// - When the session is expired and cannot be refreshed, or the
    ///   refresh request fails.
    pub fn access_token(&mut self) -> Result<&str> {
        let expired = match self.expires_at {
            Some(at) => at <= now_unix() + 30,
            None => false,
        };
        if !expired {
            return Ok(&self.access_token);
        }

        let endpoints = discover_endpoints()?;
        let tokens = if let Some(assertion) = self.assertion.clone() {
            match exchange_assertion(&endpoints, &assertion) {
                Ok(tokens) => tokens,
                Err(err) if err.to_string().contains("invalid_grant") => {
                    // Assertion expired/revoked: rotate it, then re-exchange.
                    let refresh = self.assertion_refresh_token.as_deref().context(
                        "Session expired and cannot be refreshed. Run `baml login` to start over.",
                    )?;
                    let rotated = refresh_assertion(&endpoints, refresh)
                        .context("Failed to refresh session. Run `baml login` to start over.")?;
                    self.assertion = Some(rotated.identity.assertion.clone());
                    if let Some(t) = rotated.identity.refresh_token {
                        self.assertion_refresh_token = Some(t.value);
                    }
                    exchange_assertion(&endpoints, &rotated.identity.assertion)?
                }
                Err(err) => return Err(err),
            }
        } else {
            let refresh = self.refresh_token.as_deref().context(
                "Session expired and no refresh token is stored. Run `baml auth login`.",
            )?;
            post_form(
                &format!("{}/user_management/authenticate", api_domain()),
                &[
                    ("grant_type", "refresh_token"),
                    ("client_id", &client_id()?),
                    ("refresh_token", refresh),
                ],
            )
            .context("Failed to refresh session. Run `baml auth login`.")?
        };

        self.access_token = tokens.access_token;
        self.expires_at = tokens.expires_in.map(|s| now_unix() + s);
        if tokens.refresh_token.is_some() {
            self.refresh_token = tokens.refresh_token;
        }
        if let Some(email) = tokens.user.and_then(|u| u.email) {
            self.user_email = Some(email);
            if matches!(self.status, CredStatus::Anonymous) {
                // Claimed elsewhere (e.g. the playground): flip lazily.
                self.status = CredStatus::Claimed;
                self.claim_token = None;
            }
        }
        self.write()?;
        Ok(&self.access_token)
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
    fn jwt_claim_reads_sub_without_verifying() {
        // header {"alg":"none"} . payload {"sub":"agent_reg_123","email":"a@b.c"} . sig
        let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(r#"{"sub":"agent_reg_123","email":"a@b.c"}"#);
        let jwt = format!("eyJhbGciOiJub25lIn0.{payload}.sig");
        assert_eq!(jwt_claim(&jwt, "sub").as_deref(), Some("agent_reg_123"));
        assert_eq!(jwt_claim(&jwt, "email").as_deref(), Some("a@b.c"));
        assert_eq!(jwt_claim(&jwt, "missing"), None);
    }

    #[test]
    fn credential_lifecycle_serializes_status() {
        let creds = Credentials {
            status: CredStatus::Anonymous,
            project_id: Some("agent_reg_123".into()),
            assertion: Some("a.b.c".into()),
            assertion_refresh_token: Some("art".into()),
            claim_token: Some("ct".into()),
            access_token: "at".into(),
            expires_at: Some(1),
            refresh_token: None,
            user_email: None,
        };
        let json = serde_json::to_string(&creds).unwrap();
        assert!(json.contains("\"status\":\"anonymous\""), "{json}");
        let back: Credentials = serde_json::from_str(&json).unwrap();
        assert!(matches!(back.status, CredStatus::Anonymous));
        assert_eq!(back.claim_token.as_deref(), Some("ct"));
    }
}
