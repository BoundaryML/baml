//! Boundary device login for the CLI. Identity sessions use the shared native
//! client.

use std::{
    io::{self, Write},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use serde::Serialize;

use crate::{
    ExitCode,
    output::{Command, CommandContext, Outcome, Report, Style, run_command},
};

const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Subcommand, Debug)]
pub(crate) enum AuthCommands {
    #[command(about = "Log in to Boundary with your email")]
    Login(LoginArgs),
    #[command(about = "Verify authentication and show the selected project")]
    Status(StatusArgs),
    #[command(about = "Log out")]
    Logout(LogoutArgs),
    #[command(about = "Print a valid access token", hide = true)]
    Token(TokenArgs),
}

impl AuthCommands {
    pub fn run(&self, cx: &CommandContext) -> Result<ExitCode> {
        match self {
            AuthCommands::Login(args) => Self::run_one(args, cx),
            AuthCommands::Status(args) => Self::run_one(args, cx),
            AuthCommands::Logout(args) => Self::run_one(args, cx),
            AuthCommands::Token(args) => Self::run_one(args, cx),
        }
    }

    /// Runs one auth command, reporting transport failures as actionable
    /// Boundary diagnostics.
    pub fn run_one<C: Command>(command: &C, cx: &CommandContext) -> Result<ExitCode> {
        run_command(command, cx).map_err(|error| match error.downcast::<bcs_api::Error>() {
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

/// `baml cloud auth login`: device-code email login.
#[derive(Args, Debug)]
#[command(after_long_help = "\
Examples:
  Log in using a browser:
    baml cloud auth login

  Print the verification URL instead:
    baml cloud auth login --no-open")]
pub(crate) struct LoginArgs {
    /// Print the verification URL instead of opening a browser.
    #[arg(long)]
    pub no_open: bool,
}

#[derive(Args, Debug)]
#[command(
    after_long_help = "Examples:\n  Verify authentication and show the selected project:\n    baml cloud auth status"
)]
pub(crate) struct StatusArgs {}

#[derive(Args, Debug)]
#[command(after_long_help = "Examples:\n  Log out:\n    baml cloud auth logout")]
pub(crate) struct LogoutArgs {}

#[derive(Args, Debug)]
pub(crate) struct TokenArgs {}

impl Command for LoginArgs {
    type Output = ();

    /// Runs `baml cloud auth login`.
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
    fn run(&self, _cx: &CommandContext) -> Result<Outcome<()>> {
        let reporter = crate::reporter::Reporter::new();
        // Check protected storage before asking the user to approve a login.
        let endpoint = crate::cloud_config::login_endpoint()?;
        let existing = bcs_api::Store::new(&endpoint)?.read()?;
        // Only skip the flow when the stored session can still produce a
        // token; a session that can't refresh falls through to a fresh
        // login instead of dead-ending against "already logged in".
        if let Some(email) = existing.map(|stored| stored.caller.email) {
            match boundary_session() {
                Ok(session) => {
                    reporter.status(
                        "Login",
                        format!("already logged in as {}", session.caller.email),
                    );
                    return Ok(Outcome::ok(()));
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
        let email = device_login(self.no_open)?;
        reporter.finish("Login", format!("logged in as {email}"));
        Ok(Outcome::ok(()))
    }
}

/// Runs the device authorization flow, persists the resulting session and
/// returns the caller's email.
///
/// Errors when authorization can't be started or the poll ends in denial,
/// expiry, or timeout.
fn device_login(no_open: bool) -> Result<String> {
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
                return Ok(session.caller.email);
            }
        }
    }
}

impl Command for StatusArgs {
    type Output = AuthenticationStatus;

    fn run(&self, cx: &CommandContext) -> Result<Outcome<AuthenticationStatus>> {
        use bcs_api::diagnostics::{
            Context as DiagnosticContext, CredentialSource, Operation, Outcome as Failure,
        };
        let root = crate::project_load::find_project_root_from(cx.project_dir.as_deref())?;
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
        let report_login = |error| login_context.report(error, Failure::AuthenticationFailed);
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
                diagnostic(CredentialSource::ApiKey).report(error, Failure::AuthenticationFailed)
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
        let status = AuthenticationStatus {
            user,
            api_key: api_key.is_some(),
            project,
            endpoint: endpoint.as_str().to_owned(),
        };
        Ok(Outcome::new(
            status,
            if authenticated {
                ExitCode::Success
            } else {
                ExitCode::Failure
            },
        ))
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct AuthenticationStatus {
    /// The verified saved login, if any.
    user: Option<String>,
    /// Whether a verified `BOUNDARY_API_KEY` is in effect.
    api_key: bool,
    /// The selected cloud project and where the selection came from.
    project: String,
    endpoint: String,
}

impl Report for AuthenticationStatus {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        match &self.user {
            Some(email) => writeln!(out, "User: {email} (verified)")?,
            None => writeln!(out, "User: Not logged in")?,
        }
        if self.api_key {
            let label = if self.user.is_some() {
                "Credential overridden by"
            } else {
                "Credential"
            };
            writeln!(out, "{label}: BOUNDARY_API_KEY (verified)")?;
        }
        writeln!(out, "Project: {}", self.project)?;
        if self.endpoint != bcs_api::auth::DEFAULT_API_URL {
            writeln!(out, "Endpoint: {}", self.endpoint)?;
        }
        Ok(())
    }
}

impl Command for LogoutArgs {
    type Output = ();

    fn run(&self, _cx: &CommandContext) -> Result<Outcome<()>> {
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
            Err(error @ bcs_api::Error::InvalidStoredLogin { .. }) => Some(Err(error)),
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
        Ok(Outcome::ok(()))
    }
}

/// A valid access token.
#[derive(Serialize)]
#[serde(transparent)]
pub(crate) struct Token(String);

impl Report for Token {
    fn text(&self, out: &mut dyn Write, _style: &Style) -> io::Result<()> {
        writeln!(out, "{}", self.0)
    }
}

impl Command for TokenArgs {
    type Output = Token;

    fn run(&self, _cx: &CommandContext) -> Result<Outcome<Token>> {
        let session = boundary_session()?;
        Ok(Outcome::ok(Token(session.access_token.expose().to_owned())))
    }
}

fn boundary_session() -> Result<bcs_api::Session> {
    use bcs_api::{Client, Store, StoredSession};
    let endpoint = crate::cloud_config::login_endpoint()?;
    let store = Store::new(&endpoint)?;
    let stored = store
        .read()?
        .context("not logged in; run `baml cloud auth login`")?;
    let session = Client::new(endpoint)?.refresh(&stored)?;
    store.write(&StoredSession::from(&session))?;
    Ok(session)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
                endpoint: bcs_api::auth::DEFAULT_API_URL.to_owned(),
            };
            assert_eq!(crate::output::text_of(&status).trim_end(), expected);
        }
    }
}
