//! Boundary device login for the CLI. Identity sessions use the shared native
//! client; feedback keeps its existing anonymous identifier and opt-in association.

use std::{
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::{Deserialize, Serialize};

const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

pub(crate) fn http_client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .expect("build HTTP client")
}

#[derive(Subcommand, Debug)]
pub(crate) enum AuthCommands {
    #[command(about = "Log in to Boundary with your email")]
    Login(LoginArgs),
    #[command(about = "Verify authentication and show the selected project")]
    Status(StatusArgs),
    #[command(about = "Log out (keeps your anonymous feedback id)")]
    Logout(LogoutArgs),
    #[command(about = "Print a valid access token", hide = true)]
    Token(TokenArgs),
}

impl AuthCommands {
    pub fn run(&self) -> Result<crate::ExitCode> {
        let result = match self {
            AuthCommands::Login(args) => args.run(),
            AuthCommands::Status(args) => args.run(),
            AuthCommands::Logout(args) => args.run(),
            AuthCommands::Token(args) => args.run(),
        };
        result.map_err(|error| match error.downcast::<bcs_api::Error>() {
            Ok(source) => bcs_api::diagnostics::Context {
                endpoint: crate::cloud_config::login_endpoint().ok(),
                source: bcs_api::diagnostics::CredentialSource::Configured,
                operation: bcs_api::diagnostics::Operation::Authentication,
            }
            .report(source, bcs_api::diagnostics::Outcome::AuthenticationFailed)
            .into(),
            Err(error) => error,
        })
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
#[command(
    after_long_help = "Examples:\n  Verify authentication and show the selected project:\n    baml auth status"
)]
pub(crate) struct StatusArgs {}

#[derive(Args, Debug)]
#[command(after_long_help = "Examples:\n  Log out:\n    baml auth logout")]
pub(crate) struct LogoutArgs {}

#[derive(Args, Debug)]
pub(crate) struct TokenArgs {}

impl LoginArgs {
    /// Runs `baml auth login`.
    ///
    /// Device-code login via the Boundary gateway. The persistent credential
    /// goes to protected storage; temporary access stays in this process.
    ///
    /// Returns:
    /// - `ExitCode::Success` once credentials are persisted.
    ///
    /// Errors:
    /// - When authorization can't be started, the user denies the login,
    ///   the code expires, or [`LOGIN_TIMEOUT`] elapses.
    pub fn run(&self) -> Result<crate::ExitCode> {
        let reporter = crate::reporter::Reporter::new();
        // Check protected storage before asking the user to approve a login.
        let endpoint = crate::cloud_config::login_endpoint()?;
        bcs_api::Store::new(&endpoint)?.read()?;
        let existing = Credentials::read()?.unwrap_or_default();
        // Only skip the flow when the stored session can still produce a
        // token; a session that can't refresh falls through to a fresh
        // login instead of dead-ending against "already logged in".
        if let Some(email) = existing.user_email.clone() {
            match boundary_session() {
                Ok(session) => {
                    existing.write()?;
                    reporter.status(
                        "Login",
                        format!("already logged in as {}", session.caller.email),
                    );
                    return Ok(crate::ExitCode::Success);
                }
                Err(error)
                    if error
                        .downcast_ref::<bcs_api::Error>()
                        .is_some_and(|failure| {
                            failure.status() == Some(reqwest::StatusCode::UNAUTHORIZED)
                        }) =>
                {
                    reporter.status(
                        "Login",
                        format!("the session for {email} was revoked; logging in again"),
                    );
                }
                Err(error) => return Err(error.context("Could not check existing Boundary login")),
            }
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
/// preserving any anonymous feedback identifier carried in `existing`.
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
    use bcs_api::{Client, LoginPoll, Store, StoredSession};
    let reporter = crate::reporter::Reporter::new();
    let endpoint = crate::cloud_config::login_endpoint()?;
    let store = Store::new(&endpoint)?;
    let client = Client::new(endpoint)?;
    let device = client.start().context("Failed to start Boundary login")?;
    reporter.status(
        "Login",
        format!("copy your one-time code: {}", device.user_code),
    );
    if no_open || webbrowser::open(&device.verification_uri).is_err() {
        reporter.status("Login", format!("confirm at: {}", device.verification_uri));
    }
    reporter.status("Waiting", "for browser confirmation (ctrl-c to cancel)");
    let deadline = std::time::Instant::now() + LOGIN_TIMEOUT;
    let mut interval = Duration::from_secs(u64::from(device.interval_seconds.max(1)));
    loop {
        if now_unix() >= device.expires_at || std::time::Instant::now() >= deadline {
            return Err(bcs_api::Error::LoginExpired.into());
        }
        std::thread::sleep(
            interval.min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
        match client.poll(&device.device_code)? {
            LoginPoll::Pending { interval_seconds } => {
                interval = Duration::from_secs(u64::from(interval_seconds.max(1)))
            }
            LoginPoll::Denied => return Err(bcs_api::Error::LoginDenied.into()),
            LoginPoll::Expired => return Err(bcs_api::Error::LoginExpired.into()),
            LoginPoll::Approved { session } => {
                store.write(&StoredSession::from(&session))?;
                let creds = Credentials {
                    posthog_distinct_id: existing
                        .posthog_distinct_id
                        .or_else(|| Some(uuid::Uuid::new_v4().to_string())),
                    user_id: Some(session.caller.user_id),
                    user_email: Some(session.caller.email),
                };
                creds.write()?;
                return Ok(creds);
            }
        }
    }
}

impl StatusArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        use bcs_api::diagnostics::{
            Context as DiagnosticContext, CredentialSource, Operation, Outcome,
        };
        let root = crate::project_load::find_project_root_from(None)?;
        let boundary = match root.as_deref() {
            Some(root) => crate::cloud_config::Boundary::read(root)?,
            None => crate::cloud_config::Boundary::default(),
        };
        let endpoint = boundary.endpoint()?;
        let diagnostic = |source| DiagnosticContext {
            endpoint: Some(endpoint.clone()),
            source,
            operation: Operation::Authentication,
        };
        let login_context = diagnostic(CredentialSource::SavedLogin);
        let report_login = |error| login_context.report(error, Outcome::AuthenticationFailed);
        let api_key = baml_env::string_var("BOUNDARY_API_KEY")?
            .filter(|key| key != bcs_api::credentials::LOCAL_API_KEY)
            .map(bcs_api::Secret::new);
        let project = match baml_env::string_var("BOUNDARY_PROJECT")? {
            Some(project) => format!("{project} (BOUNDARY_PROJECT)"),
            None => match boundary.project {
                Some(project) => format!("{project} (baml.toml)"),
                None if root.is_some() => "Not configured (baml.toml)".into(),
                None => "Not selected (outside a BAML project)".into(),
            },
        };
        let client = bcs_api::Client::new(endpoint.clone()).map_err(report_login)?;
        if let Some(key) = &api_key {
            client.verify_api_key(key).map_err(|error| {
                diagnostic(CredentialSource::ApiKey).report(error, Outcome::AuthenticationFailed)
            })?;
        }
        // Verify the active API key independently of the optional saved login.
        // A broken saved login remains fatal when it is the only credential.
        let login = (|| -> std::result::Result<Option<String>, bcs_api::Error> {
            let store = bcs_api::Store::new(&endpoint)?;
            let Some(stored) = store.read()? else {
                return Ok(None);
            };
            let session = client.refresh(&stored)?;
            store.write(&bcs_api::StoredSession::from(&session))?;
            Ok(Some(session.caller.email))
        })();
        let user = match login {
            Ok(user) => user,
            Err(error) if api_key.is_some() => {
                crate::reporter::Reporter::new().warning(format!(
                    "Could not verify your saved Boundary login. BOUNDARY_API_KEY is verified.\n\n{error}"
                ));
                None
            }
            Err(error) => return Err(report_login(error).into()),
        };
        let authenticated = user.is_some() || api_key.is_some();
        println!(
            "{}",
            AuthenticationStatus {
                user,
                api_key: api_key.is_some(),
                project,
                endpoint
            }
        );
        Ok(if authenticated {
            crate::ExitCode::Success
        } else {
            crate::ExitCode::Other
        })
    }
}

struct AuthenticationStatus {
    user: Option<String>,
    api_key: bool,
    project: String,
    endpoint: bcs_api::Endpoint,
}
impl std::fmt::Display for AuthenticationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.user {
            Some(email) => writeln!(f, "User: {email} (verified)")?,
            None => writeln!(f, "User: Not logged in")?,
        }
        if self.api_key {
            let label = if self.user.is_some() {
                "Credential overridden by"
            } else {
                "Credential"
            };
            writeln!(f, "{label}: BOUNDARY_API_KEY (verified)")?;
        }
        write!(f, "Project: {}", self.project)?;
        if self.endpoint.as_str() != bcs_api::auth::DEFAULT_API_URL {
            write!(f, "\nEndpoint: {}", self.endpoint.as_str())?;
        }
        Ok(())
    }
}

impl LogoutArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        use bcs_api::{Client, Store};
        let reporter = crate::reporter::Reporter::new();
        let endpoint = crate::cloud_config::login_endpoint()?;
        let store = Store::new(&endpoint)?;
        let revocation = match store.read() {
            Ok(Some(stored)) => {
                Some(Client::new(endpoint).and_then(|client| client.logout(&stored)))
            }
            Ok(None) => None,
            // Malformed local state must remain removable through logout. Without
            // a readable session we cannot confirm server-side revocation.
            Err(error @ bcs_api::Error::Protocol(_)) => Some(Err(error)),
            Err(error) => return Err(error.into()),
        };
        if let Some(revocation) = revocation {
            store.clear()?;
            reporter.status("Logout", "local login removed");
            if let Err(error) = revocation {
                reporter.warning(format!(
                    "Boundary could not confirm server-side logout.\nThe saved credential was removed from this machine, but the server session may still be active.\n\n{error}"
                ));
            }
        } else {
            reporter.status("Logout", "not logged in");
        }
        Ok(crate::ExitCode::Success)
    }
}

impl TokenArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let token = boundary_session()?;
        println!("{}", token.access_token.expose());
        Ok(crate::ExitCode::Success)
    }
}

fn boundary_session() -> Result<bcs_api::Session> {
    use bcs_api::{Client, Store, StoredSession};
    let endpoint = crate::cloud_config::login_endpoint()?;
    let store = Store::new(&endpoint)?;
    let stored = store
        .read()?
        .context("not logged in; run `baml auth login`")?;
    let session = Client::new(endpoint)?.refresh(&stored)?;
    store.write(&StoredSession::from(&session))?;
    Ok(session)
}

// ---------------------------------------------------------------------------
// Anonymous feedback state: ~/.baml/creds.json
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Deserialize, Serialize)]
pub(crate) struct Credentials {
    /// The PostHog distinct id used for feedback events. Generated locally
    /// on first anonymous feedback; survives logout so continuity and
    /// later retroactive attribution keep working.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub posthog_distinct_id: Option<String>,
    /// Caller ID from the endpoint-scoped protected login.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Verified email, once logged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_email: Option<String>,
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
    /// Combines anonymous feedback state with the selected endpoint's cached
    /// caller profile. The OS credential store is the source of login identity.
    pub fn read() -> Result<Option<Self>> {
        let path = creds_path()?;
        let content = if path.exists() {
            std::fs::read_to_string(&path)
        } else {
            Ok("{}".to_owned())
        }
        .with_context(|| format!("Failed to read {}", path.display()))?;
        let mut creds: Credentials = serde_json::from_str(&content)
            .with_context(|| format!("Malformed credentials in {}", path.display()))?;
        // Invalid endpoint configuration must fail even for anonymous feedback;
        // a credential-store outage can omit identity, but cannot change the endpoint.
        let endpoint = crate::cloud_config::login_endpoint()?;
        // Anonymous feedback remains usable when an OS credential store is
        // unavailable. Explicit auth commands report that failure themselves.
        let stored = bcs_api::Store::new(&endpoint)
            .and_then(|store| store.read())
            .unwrap_or(None);
        creds.user_id = stored.as_ref().map(|s| s.caller.user_id.clone());
        creds.user_email = stored.as_ref().map(|s| s.caller.email.clone());
        Ok(Some(creds))
    }

    /// Persists anonymous feedback state with owner-only access.
    ///
    /// On Unix the file is created with mode 0600 before any bytes are
    /// written; there is never a window where the contents are readable by
    /// other users.
    pub fn write(&self) -> Result<()> {
        let path = creds_path()?;
        // Caller identity is read from the endpoint-scoped keyring, never from this
        // global analytics file. It contains only the anonymous feedback identifier.
        let anonymous = Credentials {
            posthog_distinct_id: self.posthog_distinct_id.clone(),
            ..Credentials::default()
        };
        write_owner_only(&path, &serde_json::to_string_pretty(&anonymous)?)
    }
}

#[cfg(test)]
mod status_tests {
    use super::*;

    #[test]
    fn status_has_exact_output_for_each_credential_source() {
        for (user, api_key, expected) in [
            (
                Some("alice@acme.com"),
                true,
                r#"User: alice@acme.com (verified)
Credential overridden by: BOUNDARY_API_KEY (verified)
Project: acme/app (baml.toml)"#,
            ),
            (
                Some("alice@acme.com"),
                false,
                r#"User: alice@acme.com (verified)
Project: acme/app (baml.toml)"#,
            ),
            (
                None,
                true,
                r#"User: Not logged in
Credential: BOUNDARY_API_KEY (verified)
Project: acme/app (baml.toml)"#,
            ),
            (
                None,
                false,
                r#"User: Not logged in
Project: acme/app (baml.toml)"#,
            ),
        ] {
            let status = AuthenticationStatus {
                user: user.map(str::to_owned),
                api_key,
                project: "acme/app (baml.toml)".into(),
                endpoint: bcs_api::Endpoint::parse(bcs_api::auth::DEFAULT_API_URL).unwrap(),
            };
            assert_eq!(status.to_string(), expected);
        }
    }
}
