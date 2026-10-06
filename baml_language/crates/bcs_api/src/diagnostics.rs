//! Shared, credential-safe Boundary diagnostics. Hosts choose the operation's outcome.
use std::{fmt, sync::Arc};

use crate::{Error, credentials::RenewalError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CredentialSource {
    ApiKey,
    SavedLogin,
    Configured,
    Embedded,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    Ingest,
    Query,
    Authentication,
    MintBuildToken,
}
impl Operation {
    fn description(self) -> &'static str {
        match self {
            Self::Ingest => "cloud telemetry ingestion",
            Self::Query => "the cloud query",
            Self::Authentication => "the authentication request",
            Self::MintBuildToken => "telemetry credential minting",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    ExecutionCancelled,
    QueryFailed,
    AuthenticationFailed,
    ArtifactBuildFailed,
    TelemetryDisabled,
}
impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExecutionCancelled => "Execution cancelled.",
            Self::QueryFailed => "Query failed.",
            Self::AuthenticationFailed => "Authentication command failed.",
            Self::ArtifactBuildFailed => "Artifact build failed.",
            Self::TelemetryDisabled => "Cloud telemetry disabled. Program execution continues.",
        })
    }
}

/// Only diagnostic-safe context: never credentials, grants, SQL or response bodies.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Context {
    pub endpoint: Option<crate::Endpoint>,
    pub source: CredentialSource,
    pub operation: Operation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    AuthenticationRejected,
    PermissionDenied,
    MissingCredentials,
    InvalidConfiguration(&'static str),
    RequestEncoding,
    CredentialStorage,
    InvalidStoredLogin,
    Timeout,
    Transport,
    InvalidResponse(&'static str),
    RenewalInterrupted,
    RequestRejected(u16),
    LoginExpired,
    LoginDenied,
}
impl FailureKind {
    pub fn from_status(status: u16) -> Self {
        match status {
            401 => Self::AuthenticationRejected,
            403 => Self::PermissionDenied,
            status => Self::RequestRejected(status),
        }
    }
    pub fn code(self) -> &'static str {
        match self {
            Self::AuthenticationRejected => "BOUNDARY_AUTH_REJECTED",
            Self::PermissionDenied => "BOUNDARY_PERMISSION_DENIED",
            Self::MissingCredentials => "BOUNDARY_CREDENTIAL_REQUIRED",
            Self::InvalidConfiguration(_) => "BOUNDARY_CONFIG_INVALID",
            Self::RequestEncoding => "BOUNDARY_REQUEST_ENCODING_FAILED",
            Self::CredentialStorage => "BOUNDARY_CREDENTIAL_STORAGE_UNAVAILABLE",
            Self::InvalidStoredLogin => "BOUNDARY_STORED_LOGIN_INVALID",
            Self::Timeout => "BOUNDARY_TIMEOUT",
            Self::Transport => "BOUNDARY_CONNECTION_FAILED",
            Self::InvalidResponse(_) => "BOUNDARY_RESPONSE_INVALID",
            Self::RenewalInterrupted => "BOUNDARY_RENEWAL_INTERRUPTED",
            Self::RequestRejected(_) => "BOUNDARY_REQUEST_REJECTED",
            Self::LoginExpired => "BOUNDARY_LOGIN_EXPIRED",
            Self::LoginDenied => "BOUNDARY_LOGIN_DENIED",
        }
    }
    fn from_error(error: &Error) -> Self {
        if let Some(status) = error.status() {
            return Self::from_status(status.as_u16());
        }
        match error {
            Error::Transport(error) if error.is_timeout() => Self::Timeout,
            Error::Read(error) if error.kind() == std::io::ErrorKind::TimedOut => Self::Timeout,
            Error::Transport(_) | Error::Read(_) => Self::Transport,
            Error::Storage { .. } => Self::CredentialStorage,
            Error::InvalidStoredLogin { .. } => Self::InvalidStoredLogin,
            Error::Protocol(reason) => Self::InvalidResponse(reason),
            Error::Renewal(RenewalError::MissingAccessToken) => {
                Self::InvalidResponse("Boundary credential renewal returned no usable access token")
            }
            Error::Renewal(RenewalError::Aborted) => Self::RenewalInterrupted,
            Error::MissingCredentials => Self::MissingCredentials,
            Error::LoginExpired => Self::LoginExpired,
            Error::LoginDenied => Self::LoginDenied,
            Error::Endpoint(reason) => Self::InvalidConfiguration(reason),
            Error::Url(_) => {
                Self::InvalidConfiguration("Boundary API URL must be a valid absolute URL")
            }
            Error::Environment(_) => {
                Self::InvalidConfiguration("Boundary environment variables could not be read")
            }
            Error::EndpointMismatch => Self::InvalidConfiguration(
                "The credential belongs to a different Boundary endpoint",
            ),
            Error::QueryId => Self::InvalidConfiguration("Cloud query ID is invalid"),
            Error::Encode(_) => Self::RequestEncoding,
            // HTTP failures and rejected renewals always have a status above.
            Error::Http(_) | Error::Renewal(RenewalError::Rejected(_)) => unreachable!(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SavedLogin {
    Exists,
    Missing,
    Unreadable,
}

/// The artifact's permitted recording-level override, used in recovery suggestions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RecordingOverride {
    Fixed,
    EnvVar(String),
    Artifact {
        level: Option<String>,
        project: Option<String>,
        api_key: Option<String>,
    },
}
impl Default for RecordingOverride {
    fn default() -> Self {
        Self::EnvVar("BAML_TELEMETRY".to_owned())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub kind: FailureKind,
    pub context: Context,
    pub outcome: Outcome,
    saved_login: SavedLogin,
    api_error: Option<Arc<crate::ApiErrorBody>>,
    credential_store: Option<crate::auth::CredentialStoreLocation>,
    recording_override: RecordingOverride,
}

impl Context {
    pub fn connection_failed(&self, outcome: Outcome) -> Diagnostic {
        self.diagnostic(FailureKind::Transport, outcome)
    }
    pub fn rejected(&self, status: u16, outcome: Outcome) -> Diagnostic {
        self.diagnostic(FailureKind::from_status(status), outcome)
    }
    pub fn report(&self, source: Error, outcome: Outcome) -> ReportedError {
        let mut diagnostic = self.diagnostic(FailureKind::from_error(&source), outcome);
        diagnostic.api_error = source
            .http_failure()
            .and_then(|failure| failure.body.clone());
        if let Error::Storage { location, .. } | Error::InvalidStoredLogin { location } = &source {
            diagnostic.credential_store = Some(location.clone());
        }
        ReportedError { diagnostic, source }
    }
    pub fn rejected_failure(&self, failure: &crate::HttpFailure, outcome: Outcome) -> Diagnostic {
        let mut diagnostic = self.rejected(failure.status.as_u16(), outcome);
        diagnostic.api_error.clone_from(&failure.body);
        diagnostic
    }
    fn diagnostic(&self, kind: FailureKind, outcome: Outcome) -> Diagnostic {
        // Recovery discovery runs only on failure, never on an API-key startup path.
        let saved_login = if kind == FailureKind::AuthenticationRejected
            && self.source == CredentialSource::ApiKey
        {
            let session = self
                .endpoint
                .as_ref()
                .map(|endpoint| crate::Store::new(endpoint).and_then(|store| store.read()));
            match session {
                Some(Ok(Some(_))) => SavedLogin::Exists,
                Some(Ok(None)) => SavedLogin::Missing,
                None | Some(Err(_)) => SavedLogin::Unreadable,
            }
        } else {
            SavedLogin::Missing
        };
        Diagnostic {
            kind,
            context: self.clone(),
            outcome,
            saved_login,
            api_error: None,
            credential_store: None,
            recording_override: RecordingOverride::default(),
        }
    }
}

/// Retain the transport source for programmatic inspection. CLI rendering uses `diagnostic`.
#[derive(Debug, thiserror::Error)]
#[error("{diagnostic}")]
pub struct ReportedError {
    pub diagnostic: Diagnostic,
    #[source]
    pub source: Error,
}

impl Diagnostic {
    #[must_use]
    pub fn with_recording_override(mut self, policy: RecordingOverride) -> Self {
        self.recording_override = policy;
        self
    }

    fn api_key_name(&self) -> Option<&str> {
        match &self.recording_override {
            RecordingOverride::Artifact { api_key, .. } => api_key.as_deref(),
            _ => Some("BOUNDARY_API_KEY"),
        }
    }
    fn authentication_choices(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.context.source == CredentialSource::Embedded {
            writeln!(
                f,
                "    • Ask the publisher to rebuild with `--embed-telemetry` and an active ingestion credential."
            )?;
            if let RecordingOverride::Artifact {
                project: Some(project),
                api_key: Some(key),
                ..
            } = &self.recording_override
            {
                writeln!(
                    f,
                    "    • Set {key} to your own Boundary API key, or run `baml auth login` and set {project}=org_handle/project_name."
                )?;
            }
            return Ok(());
        }
        let key = self.api_key_name();
        match self.context.source {
            CredentialSource::ApiKey => {
                let key = key.unwrap_or("the configured API key");
                writeln!(f, "    • Replace {key} with a valid API key.")?;
                match self.saved_login {
                    SavedLogin::Exists => {
                        writeln!(f, "    • Unset {key} to use your saved Boundary login.")
                    }
                    SavedLogin::Missing => {
                        writeln!(f, "    • Unset {key}, then run `baml auth login`.")
                    }
                    SavedLogin::Unreadable => writeln!(
                        f,
                        "    • Your saved login could not be read. Unset {key}, then run `baml auth login` to restore user authentication."
                    ),
                }
            }
            CredentialSource::SavedLogin => {
                writeln!(f, "    • Run `baml auth login` again.")?;
                if let Some(key) = key {
                    writeln!(f, "    • Set {key} to a valid API key.")?;
                }
                Ok(())
            }
            CredentialSource::Embedded => unreachable!(),
            CredentialSource::Configured => writeln!(
                f,
                "    • Replace the configured credential with a valid credential for this endpoint."
            ),
        }
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let operation = self.context.operation;
        match self.kind {
            FailureKind::AuthenticationRejected => {
                let credential = match self.context.source {
                    CredentialSource::ApiKey => {
                        self.api_key_name().unwrap_or("the configured API key")
                    }
                    CredentialSource::SavedLogin => "your saved login",
                    CredentialSource::Embedded => "the embedded telemetry credential",
                    CredentialSource::Configured if operation == Operation::Authentication => {
                        "the authentication request"
                    }
                    CredentialSource::Configured => "the configured credential",
                };
                writeln!(f, "Boundary rejected {credential} (401).")?;
            }
            FailureKind::PermissionDenied => {
                writeln!(f, "Boundary denied {} (HTTP 403).", operation.description())?;
            }
            FailureKind::MissingCredentials => writeln!(
                f,
                "No Boundary credential is available for {}.",
                operation.description()
            )?,
            FailureKind::InvalidConfiguration(reason) => {
                writeln!(f, "Boundary configuration is invalid: {reason}.")?;
            }
            FailureKind::RequestEncoding => writeln!(f, "Could not encode the Boundary request.")?,
            FailureKind::CredentialStorage => {
                if let Some(location) = &self.credential_store {
                    writeln!(
                        f,
                        "Boundary could not access the local OS credential store at {location}."
                    )?;
                } else {
                    writeln!(
                        f,
                        "Boundary could not access the local OS credential store."
                    )?;
                }
            }
            FailureKind::InvalidStoredLogin => {
                if let Some(location) = &self.credential_store {
                    writeln!(
                        f,
                        "The saved Boundary login in the local OS credential store at {location} is invalid."
                    )?;
                } else {
                    writeln!(f, "The saved Boundary login is invalid.")?;
                }
            }
            FailureKind::Timeout => writeln!(
                f,
                "Boundary timed out while processing {}.",
                operation.description()
            )?,
            FailureKind::Transport => writeln!(
                f,
                "Could not communicate with Boundary for {}.",
                operation.description()
            )?,
            FailureKind::InvalidResponse(reason) => {
                writeln!(f, "Boundary returned an invalid response: {reason}.")?;
            }
            FailureKind::RenewalInterrupted => {
                writeln!(f, "Boundary credential renewal did not complete.")?;
            }
            FailureKind::RequestRejected(status) => writeln!(
                f,
                "Boundary rejected {} (HTTP {status}).",
                operation.description()
            )?,
            FailureKind::LoginExpired => writeln!(f, "Login code expired.")?,
            FailureKind::LoginDenied => writeln!(f, "Login was denied in the browser.")?,
        }
        if let Some(endpoint) = self
            .context
            .endpoint
            .as_ref()
            .filter(|endpoint| endpoint.as_str() != crate::auth::DEFAULT_API_URL)
        {
            writeln!(f, "  Endpoint: {}", endpoint.as_str())?;
        }
        if let Some(body) = &self.api_error {
            writeln!(f, "  Code: {}", body.code)?;
            if let Some(message) = &body.message {
                for line in message.lines() {
                    writeln!(f, "  {line}")?;
                }
            }
        }
        writeln!(f, "\n  To continue, choose one:")?;
        match self.kind {
            FailureKind::RequestRejected(400)
                if self
                    .api_error
                    .as_ref()
                    .is_some_and(|body| body.code == "ENVIRONMENT_REQUIRED") =>
            {
                match operation {
                    Operation::Query => writeln!(
                        f,
                        "    • Select an environment with --environment <name> and, when needed, --project <org/project>."
                    ),
                    Operation::Ingest => {
                        if let RecordingOverride::Artifact {
                            project, api_key, ..
                        } = &self.recording_override
                        {
                            match (project, api_key) {
                                (Some(project), Some(key)) => writeln!(
                                    f,
                                    "    • Set {project}=org_handle/project_name for user authentication, or set {key} to an API key bound to an ingestion environment."
                                ),
                                _ => writeln!(
                                    f,
                                    "    • Ask the publisher to rebuild with a valid telemetry destination."
                                ),
                            }
                        } else {
                            writeln!(
                                f,
                                "    • Set BOUNDARY_PROJECT=org_handle/project_name for user authentication, or use an API key bound to an ingestion environment."
                            )
                        }
                    }
                    Operation::Authentication | Operation::MintBuildToken => {
                        writeln!(f, "    • Check the requested project and environment.")
                    }
                }?;
            }
            FailureKind::AuthenticationRejected
                if operation != Operation::Authentication
                    || self.context.source == CredentialSource::ApiKey =>
            {
                self.authentication_choices(f)?;
            }
            FailureKind::AuthenticationRejected
            | FailureKind::LoginExpired
            | FailureKind::LoginDenied => writeln!(
                f,
                "    • Run `baml auth login` again and approve the new code in your browser."
            )?,
            FailureKind::PermissionDenied => {
                if self.context.source == CredentialSource::Embedded {
                    self.authentication_choices(f)?;
                } else if operation == Operation::Authentication {
                    writeln!(
                        f,
                        "    • Ask your Boundary administrator to check authentication access for this deployment."
                    )?;
                } else {
                    let permission = match operation {
                        Operation::Ingest => "ingestion",
                        Operation::MintBuildToken => "minting (or project administrator access)",
                        _ => "query",
                    };
                    writeln!(
                        f,
                        "    • Use a credential with {permission} permission for the selected project and environment."
                    )?;
                }
            }
            FailureKind::MissingCredentials => {
                writeln!(f, "    • Run `baml auth login`.")?;
                if let Some(key) = self.api_key_name() {
                    writeln!(f, "    • Set {key} to a valid API key.")?;
                }
            }
            FailureKind::InvalidConfiguration(reason) if reason.starts_with("Boundary API URL") => {
                writeln!(
                    f,
                    "    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml."
                )?;
            }
            FailureKind::InvalidConfiguration(_) => writeln!(
                f,
                "    • Check BOUNDARY_API_URL and [boundary] settings in baml.toml."
            )?,
            FailureKind::RequestEncoding => writeln!(
                f,
                "    • Check the request inputs and BAML client version before retrying."
            )?,
            FailureKind::CredentialStorage => {
                writeln!(
                    f,
                    "    • Unlock your OS credential store and allow BAML to access it."
                )?;
                if operation != Operation::Authentication {
                    if let Some(key) = self.api_key_name() {
                        writeln!(f, "    • Set {key} to a valid API key.")?;
                    }
                }
            }
            FailureKind::InvalidStoredLogin => {
                writeln!(
                    f,
                    "    • Remove the invalid saved login: run `baml auth logout`, then `baml auth login`."
                )?;
                if let Some(key) = self.api_key_name() {
                    writeln!(f, "    • Set {key} to a valid API key.")?;
                }
            }
            FailureKind::Timeout | FailureKind::Transport => writeln!(
                f,
                "    • Check the endpoint and network connection, then retry the command."
            )?,
            FailureKind::InvalidResponse(_) => writeln!(
                f,
                "    • Check that the endpoint is a Boundary API compatible with this BAML client."
            )?,
            FailureKind::RenewalInterrupted => writeln!(f, "    • Retry the command.")?,
            FailureKind::RequestRejected(429 | 500..=599) => writeln!(
                f,
                "    • Retry the command after Boundary becomes available."
            )?,
            FailureKind::RequestRejected(404) => writeln!(
                f,
                "    • Check the endpoint and requested project/environment, then retry."
            )?,
            FailureKind::RequestRejected(_) => writeln!(
                f,
                "    • Check the command inputs and requested project/environment."
            )?,
        }
        match operation {
            Operation::Ingest => {
                if let Some(key) = self.api_key_name() {
                    writeln!(f, "    • Record locally: rerun with {key}=local.")?;
                }
                let level = match &self.recording_override {
                    RecordingOverride::EnvVar(name) => Some(name.as_str()),
                    RecordingOverride::Artifact { level, .. } => level.as_deref(),
                    RecordingOverride::Fixed => None,
                };
                if let Some(name) = level {
                    writeln!(f, "    • Disable recording: rerun with {name}=off.")?;
                }
            }
            Operation::Query => writeln!(
                f,
                "    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local."
            )?,
            Operation::Authentication | Operation::MintBuildToken => {}
        }
        write!(f, "\n  {}", self.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpFailure;

    fn context(operation: Operation, source: CredentialSource) -> Context {
        Context {
            endpoint: Some(crate::Endpoint::parse("https://api.cloud.boundaryml.com").unwrap()),
            source,
            operation,
        }
    }

    #[test]
    fn normalized_default_endpoint_is_omitted() {
        for endpoint in [
            crate::auth::DEFAULT_API_URL,
            "https://api.cloud.boundaryml.com/",
            "https://API.CLOUD.BOUNDARYML.COM:443/",
        ] {
            let mut context = context(Operation::Query, CredentialSource::SavedLogin);
            context.endpoint = Some(crate::Endpoint::parse(endpoint).unwrap());
            assert_eq!(
                context.rejected(401, Outcome::QueryFailed).to_string(),
                r#"Boundary rejected your saved login (401).

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
            );
        }
    }

    #[test]
    fn custom_endpoint_is_displayed() {
        let mut context = context(Operation::Query, CredentialSource::SavedLogin);
        context.endpoint =
            Some(crate::Endpoint::parse("https://proxy.example.test/boundary").unwrap());
        assert_eq!(
            context.rejected(401, Outcome::QueryFailed).to_string(),
            r#"Boundary rejected your saved login (401).
  Endpoint: https://proxy.example.test/boundary

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        );
    }

    #[test]
    fn ingestion_recovery_respects_the_artifacts_recording_override() {
        for (policy, expected) in [
            (
                RecordingOverride::Fixed,
                r#"Boundary rejected your saved login (401).

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Record locally: rerun with BOUNDARY_API_KEY=local.

  Execution cancelled."#,
            ),
            (
                RecordingOverride::EnvVar("ACME_TELEMETRY".into()),
                r#"Boundary rejected your saved login (401).

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with ACME_TELEMETRY=off.

  Execution cancelled."#,
            ),
        ] {
            assert_eq!(
                context(Operation::Ingest, CredentialSource::SavedLogin)
                    .rejected(401, Outcome::ExecutionCancelled)
                    .with_recording_override(policy)
                    .to_string(),
                expected,
            );
        }
    }

    #[test]
    fn rejected_key_suggests_the_correct_login_recovery() {
        for (saved_login, expected) in [
            (
                SavedLogin::Exists,
                r#"Boundary rejected BOUNDARY_API_KEY (401).

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • Unset BOUNDARY_API_KEY to use your saved Boundary login.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#,
            ),
            (
                SavedLogin::Missing,
                r#"Boundary rejected BOUNDARY_API_KEY (401).

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • Unset BOUNDARY_API_KEY, then run `baml auth login`.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#,
            ),
            (
                SavedLogin::Unreadable,
                r#"Boundary rejected BOUNDARY_API_KEY (401).

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • Your saved login could not be read. Unset BOUNDARY_API_KEY, then run `baml auth login` to restore user authentication.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#,
            ),
        ] {
            let message = Diagnostic {
                kind: FailureKind::AuthenticationRejected,
                context: context(Operation::Ingest, CredentialSource::ApiKey),
                outcome: Outcome::ExecutionCancelled,
                saved_login,
                api_error: None,
                credential_store: None,
                recording_override: RecordingOverride::default(),
            }
            .to_string();
            assert_eq!(message, expected, "saved login: {saved_login:?}");
        }
    }

    #[test]
    fn saved_login_failure_preserves_the_agreed_message() {
        let message = context(Operation::Ingest, CredentialSource::SavedLogin)
            .rejected(401, Outcome::ExecutionCancelled)
            .to_string();
        assert_eq!(
            message,
            r#"Boundary rejected your saved login (401).

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
        );
    }

    #[test]
    fn structured_error_code_and_optional_message_have_exact_diagnostics() {
        for (message, expected) in [
            (
                Some("Select an environment: this credential can query multiple environments."),
                r#"Boundary rejected the cloud query (HTTP 400).
  Code: ENVIRONMENT_REQUIRED
  Select an environment: this credential can query multiple environments.

  To continue, choose one:
    • Select an environment with --environment <name> and, when needed, --project <org/project>.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
            (
                None,
                r#"Boundary rejected the cloud query (HTTP 400).
  Code: ENVIRONMENT_REQUIRED

  To continue, choose one:
    • Select an environment with --environment <name> and, when needed, --project <org/project>.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
        ] {
            let report = context(Operation::Query, CredentialSource::ApiKey).report(
                Error::Http(HttpFailure {
                    status: reqwest::StatusCode::BAD_REQUEST,
                    retry_after: None,
                    body: Some(Arc::new(crate::ApiErrorBody {
                        code: "ENVIRONMENT_REQUIRED".into(),
                        retryable: false,
                        message: message.map(str::to_owned),
                    })),
                }),
                Outcome::QueryFailed,
            );
            assert_eq!(report.to_string(), expected);
            assert!(
                !report
                    .source
                    .http_failure()
                    .unwrap()
                    .body
                    .as_ref()
                    .unwrap()
                    .retryable
            );
        }
    }

    #[test]
    fn telemetry_rejection_retains_the_public_explanation() {
        let failure = HttpFailure {
            status: reqwest::StatusCode::FORBIDDEN,
            retry_after: None,
            body: Some(Arc::new(crate::ApiErrorBody {
                code: "ACCESS_REQUIRED".into(),
                retryable: false,
                message: Some(
                    "User authentication can ingest only into your personal environment.".into(),
                ),
            })),
        };
        let diagnostic = context(Operation::Ingest, CredentialSource::SavedLogin)
            .rejected_failure(&failure, Outcome::ExecutionCancelled);
        assert_eq!(
            diagnostic.to_string(),
            r#"Boundary denied cloud telemetry ingestion (HTTP 403).
  Code: ACCESS_REQUIRED
  User authentication can ingest only into your personal environment.

  To continue, choose one:
    • Use a credential with ingestion permission for the selected project and environment.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
        );
    }

    #[test]
    fn query_failures_have_query_specific_recovery_and_outcomes() {
        let message = context(Operation::Query, CredentialSource::SavedLogin)
            .rejected(401, Outcome::QueryFailed)
            .to_string();
        assert_eq!(
            message,
            r#"Boundary rejected your saved login (401).

  To continue, choose one:
    • Run `baml auth login` again.
    • Set BOUNDARY_API_KEY to a valid API key.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        );
        let message = context(Operation::Query, CredentialSource::ApiKey)
            .rejected(403, Outcome::QueryFailed)
            .to_string();
        assert_eq!(
            message,
            r#"Boundary denied the cloud query (HTTP 403).

  To continue, choose one:
    • Use a credential with query permission for the selected project and environment.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        );
    }

    #[test]
    fn classification_keeps_status_and_source_without_guessing_permission_reason() {
        let context = context(Operation::Query, CredentialSource::SavedLogin);
        for (status, expected) in [
            (
                400,
                r#"Boundary rejected the cloud query (HTTP 400).

  To continue, choose one:
    • Check the command inputs and requested project/environment.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
            (
                403,
                r#"Boundary denied the cloud query (HTTP 403).

  To continue, choose one:
    • Use a credential with query permission for the selected project and environment.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
            (
                404,
                r#"Boundary rejected the cloud query (HTTP 404).

  To continue, choose one:
    • Check the endpoint and requested project/environment, then retry.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
            (
                429,
                r#"Boundary rejected the cloud query (HTTP 429).

  To continue, choose one:
    • Retry the command after Boundary becomes available.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
            (
                503,
                r#"Boundary rejected the cloud query (HTTP 503).

  To continue, choose one:
    • Retry the command after Boundary becomes available.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#,
            ),
        ] {
            let report = context.report(
                Error::Http(HttpFailure::new(
                    reqwest::StatusCode::from_u16(status).unwrap(),
                )),
                Outcome::QueryFailed,
            );
            assert_eq!(report.source.status().unwrap().as_u16(), status);
            assert_eq!(report.diagnostic.kind, FailureKind::from_status(status));
            assert_eq!(report.to_string(), expected, "HTTP {status}");
        }
    }

    #[test]
    fn timeout_and_invalid_response_preserve_classification_and_exact_messages() {
        let context = context(Operation::Query, CredentialSource::SavedLogin);
        let report = context.report(
            Error::Read(std::io::Error::from(std::io::ErrorKind::TimedOut)),
            Outcome::QueryFailed,
        );
        assert_eq!(report.diagnostic.kind, FailureKind::Timeout);
        assert_eq!(
            report.to_string(),
            r#"Boundary timed out while processing the cloud query.

  To continue, choose one:
    • Check the endpoint and network connection, then retry the command.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        );
        let report = context.report(
            Error::Protocol("Cloud query ended without an outcome"),
            Outcome::QueryFailed,
        );
        assert_eq!(report.diagnostic.kind.code(), "BOUNDARY_RESPONSE_INVALID");
        assert_eq!(
            report.to_string(),
            r#"Boundary returned an invalid response: Cloud query ended without an outcome.

  To continue, choose one:
    • Check that the endpoint is a Boundary API compatible with this BAML client.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        );
    }

    #[test]
    fn auth_storage_and_login_failures_get_auth_specific_recovery() {
        let context = context(Operation::Authentication, CredentialSource::Configured);
        for (error, expected) in [
            (
                Error::LoginDenied,
                r#"Login was denied in the browser.

  To continue, choose one:
    • Run `baml auth login` again and approve the new code in your browser.

  Authentication command failed."#,
            ),
            (
                Error::LoginExpired,
                r#"Login code expired.

  To continue, choose one:
    • Run `baml auth login` again and approve the new code in your browser.

  Authentication command failed."#,
            ),
            (
                Error::Storage {
                    operation: "read",
                    location: crate::auth::CredentialStoreLocation {
                        backend: "Test credential store",
                        service: "Boundary BAML login",
                        account: "test-account".into(),
                    },
                    source: keyring::Error::NoEntry,
                },
                r#"Boundary could not access the local OS credential store at Test credential store (service "Boundary BAML login", account "test-account").

  To continue, choose one:
    • Unlock your OS credential store and allow BAML to access it.

  Authentication command failed."#,
            ),
        ] {
            let report = context.report(error, Outcome::AuthenticationFailed);
            let message = report.to_string();
            assert_eq!(message, expected);
        }
    }
}

#[cfg(test)]
mod artifact_diagnostics_tests {
    use super::*;
    #[test]
    fn fixed_embedded_rejection_guides_the_publisher_without_ambient_override_suggestions() {
        let context = Context {
            endpoint: None,
            source: CredentialSource::Embedded,
            operation: Operation::Ingest,
        };
        assert_eq!(
            context
                .rejected(401, Outcome::ExecutionCancelled)
                .with_recording_override(RecordingOverride::Artifact {
                    level: None,
                    project: None,
                    api_key: None
                })
                .to_string(),
            r#"Boundary rejected the embedded telemetry credential (401).

  To continue, choose one:
    • Ask the publisher to rebuild with `--embed-telemetry` and an active ingestion credential.

  Execution cancelled."#
        );
    }
    #[test]
    fn embedded_recovery_names_only_publisher_permitted_variables() {
        let context = Context {
            endpoint: None,
            source: CredentialSource::Embedded,
            operation: Operation::Ingest,
        };
        assert_eq!(
            context
                .rejected(403, Outcome::ExecutionCancelled)
                .with_recording_override(RecordingOverride::Artifact {
                    level: Some("ACME_TELEMETRY".into()),
                    project: Some("ACME_PROJECT".into()),
                    api_key: Some("ACME_KEY".into())
                })
                .to_string(),
            r#"Boundary denied cloud telemetry ingestion (HTTP 403).

  To continue, choose one:
    • Ask the publisher to rebuild with `--embed-telemetry` and an active ingestion credential.
    • Set ACME_KEY to your own Boundary API key, or run `baml auth login` and set ACME_PROJECT=org_handle/project_name.
    • Record locally: rerun with ACME_KEY=local.
    • Disable recording: rerun with ACME_TELEMETRY=off.

  Execution cancelled."#
        );
    }
}
