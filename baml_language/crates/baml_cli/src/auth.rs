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
            anyhow::bail!("Login code expired; run `baml auth login` again");
        }
        std::thread::sleep(
            interval.min(deadline.saturating_duration_since(std::time::Instant::now())),
        );
        match client.poll(&device.device_code)? {
            LoginPoll::Pending { interval_seconds } => {
                interval = Duration::from_secs(u64::from(interval_seconds.max(1)))
            }
            LoginPoll::Denied => anyhow::bail!("Login was denied in the browser"),
            LoginPoll::Expired => anyhow::bail!("Login code expired; run `baml auth login` again"),
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

impl WhoamiArgs {
    #[allow(clippy::print_stdout)]
    pub fn run(&self) -> Result<crate::ExitCode> {
        let endpoint = crate::cloud_config::login_endpoint()?;
        bcs_api::Store::new(&endpoint)?.read()?;
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
    pub fn run(&self) -> Result<crate::ExitCode> {
        use bcs_api::{Client, Store};
        let reporter = crate::reporter::Reporter::new();
        let endpoint = crate::cloud_config::login_endpoint()?;
        let store = Store::new(&endpoint)?;
        if let Some(stored) = store.read()? {
            let revocation = Client::new(endpoint).and_then(|client| client.logout(&stored));
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
