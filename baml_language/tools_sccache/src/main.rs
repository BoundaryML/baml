//! Native `RUSTC_WRAPPER` shim for BAML's shared sccache R2 cache.
//!
//! Explicit BAML-prefixed credentials are mapped to the AWS names sccache
//! consumes. Local macOS developers can instead hand a short-lived Infisical
//! human-session token to this process; the official Infisical Rust SDK then
//! retrieves the R2 pair and only the spawned sccache process receives it.

use std::{
    ffi::OsString,
    io::{self, Write},
    net::{Ipv4Addr, SocketAddr, SocketAddrV4, TcpStream},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, exit},
    time::Duration,
};

use async_trait::async_trait;
use infisical::{Client, InfisicalError, secrets::GetSecretRequest};
use reqwest_0_12::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;

const ACCESS_KEY_ID: &str = "BAML_SCCACHE_R2_ACCESS_KEY_ID";
const LEGACY_ACCESS_KEY: &str = "BAML_SCCACHE_R2_ACCESS_KEY";
const SECRET_ACCESS_KEY: &str = "BAML_SCCACHE_R2_SECRET_ACCESS_KEY";
const INFISICAL_TOKEN: &str = "INFISICAL_TOKEN";
const INFISICAL_CONTROL: &str = "BAML_SCCACHE_INFISICAL";
const DIRENV_LOADED: &str = "BAML_SCCACHE_DIRENV_LOADED";

const INFISICAL_BASE_URL: &str = "https://app.infisical.com";
const INFISICAL_PROJECT_FILE: &str = ".infisical.json";
const INFISICAL_ENVIRONMENT: &str = "gh-ci";
const INFISICAL_SECRET_PATH: &str = "/";

const SCCACHE_CONFIGURATION: [&str; 4] = [
    "SCCACHE_BUCKET",
    "SCCACHE_REGION",
    "SCCACHE_ENDPOINT",
    "SCCACHE_S3_KEY_PREFIX",
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SafeReason {
    PartialExplicitCredentials,
    MissingSessionToken,
    AuthenticationExpiredOrRejected,
    AccessDenied,
    SecretMissing,
    NetworkUnavailable,
    InvalidResponse,
    MissingProjectConfiguration,
    MissingSccacheConfiguration,
}

impl SafeReason {
    fn message(self) -> &'static str {
        match self {
            Self::PartialExplicitCredentials => {
                "only one explicit R2 credential is set; set both canonical BAML_SCCACHE_R2_* names or unset both"
            }
            Self::MissingSessionToken => {
                "no short-lived INFISICAL_TOKEN handoff; see tools/baml-sccache.md"
            }
            Self::AuthenticationExpiredOrRejected => {
                "the Infisical session token is expired or rejected; refresh it and restart sccache"
            }
            Self::AccessDenied => {
                "the Infisical session cannot access the gh-ci cache secrets for this repository's project"
            }
            Self::SecretMissing => {
                "an R2 cache secret is missing from the configured project's gh-ci environment at /"
            }
            Self::NetworkUnavailable => {
                "Infisical is unavailable; check the network and restart sccache to retry"
            }
            Self::InvalidResponse => {
                "Infisical returned an unusable response; restart sccache to retry"
            }
            Self::MissingProjectConfiguration => {
                "cannot read the Infisical project from .infisical.json"
            }
            Self::MissingSccacheConfiguration => {
                "SCCACHE_BUCKET configuration is not loaded; install and allow direnv for this repository"
            }
        }
    }
}

struct InfisicalConfiguration {
    project_id: String,
}

#[derive(Deserialize)]
struct InfisicalProjectFile {
    #[serde(rename = "workspaceId", alias = "projectId")]
    project_id: String,
}

impl InfisicalConfiguration {
    fn from_repository_root(repository_root: &Path) -> Result<Self, SafeReason> {
        let contents = std::fs::read_to_string(repository_root.join(INFISICAL_PROJECT_FILE))
            .map_err(|_| SafeReason::MissingProjectConfiguration)?;
        Self::from_json(&contents)
    }

    fn from_json(contents: &str) -> Result<Self, SafeReason> {
        let project: InfisicalProjectFile =
            serde_json::from_str(contents).map_err(|_| SafeReason::MissingProjectConfiguration)?;
        if project.project_id.is_empty() {
            return Err(SafeReason::MissingProjectConfiguration);
        }
        Ok(Self {
            project_id: project.project_id,
        })
    }
}

struct SccacheConfiguration {
    bucket: OsString,
    region: OsString,
    endpoint: OsString,
    key_prefix: OsString,
}

impl SccacheConfiguration {
    fn from_environment(environment: &impl Environment) -> Result<Self, SafeReason> {
        let value = |name| {
            environment
                .var_os(name)
                .filter(|value| !value.is_empty())
                .ok_or(SafeReason::MissingSccacheConfiguration)
        };
        Ok(Self {
            bucket: value("SCCACHE_BUCKET")?,
            region: value("SCCACHE_REGION")?,
            endpoint: value("SCCACHE_ENDPOINT")?,
            key_prefix: value("SCCACHE_S3_KEY_PREFIX")?,
        })
    }
}

struct R2Credentials {
    access_key_id: SecretString,
    secret_access_key: SecretString,
}

impl R2Credentials {
    fn new(access_key_id: String, secret_access_key: String) -> Self {
        Self {
            access_key_id: SecretString::from(access_key_id),
            secret_access_key: SecretString::from(secret_access_key),
        }
    }
}

trait AuthenticationProvider {
    fn access_token(&self) -> Result<&SecretString, SafeReason>;
}

struct EnvironmentTokenProvider {
    token: Option<SecretString>,
}

impl EnvironmentTokenProvider {
    fn from_environment(environment: &impl Environment) -> Self {
        Self {
            token: environment
                .var_os(INFISICAL_TOKEN)
                .and_then(|value| value.into_string().ok())
                .filter(|value| !value.is_empty())
                .map(SecretString::from),
        }
    }
}

impl std::fmt::Debug for EnvironmentTokenProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("EnvironmentTokenProvider")
            .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
            .finish()
    }
}

impl AuthenticationProvider for EnvironmentTokenProvider {
    fn access_token(&self) -> Result<&SecretString, SafeReason> {
        self.token.as_ref().ok_or(SafeReason::MissingSessionToken)
    }
}

#[async_trait]
trait CredentialProvider {
    async fn fetch(&self) -> Result<R2Credentials, SafeReason>;
}

struct InfisicalCredentialProvider<A> {
    authentication: A,
    repository_root: Option<PathBuf>,
}

impl<A> InfisicalCredentialProvider<A> {
    fn new(authentication: A, repository_root: Option<PathBuf>) -> Self {
        Self {
            authentication,
            repository_root,
        }
    }
}

#[async_trait]
impl<A> CredentialProvider for InfisicalCredentialProvider<A>
where
    A: AuthenticationProvider + Sync,
{
    async fn fetch(&self) -> Result<R2Credentials, SafeReason> {
        let repository_root = self
            .repository_root
            .as_deref()
            .ok_or(SafeReason::MissingProjectConfiguration)?;
        let configuration = InfisicalConfiguration::from_repository_root(repository_root)?;
        let token = self.authentication.access_token()?;
        let mut client = Client::builder()
            .base_url(INFISICAL_BASE_URL)
            .user_agent(concat!("baml-sccache/", env!("CARGO_PKG_VERSION")))
            .request_timeout(Duration::from_secs(5))
            .build()
            .await
            .map_err(classify_infisical_error)?;

        // Infisical's official Rust SDK 0.0.3 only documents Universal Auth.
        // Its Client fields are public, so this is the narrowest available
        // token handoff until the SDK adds a supported access-token setter.
        // HeaderValue is marked sensitive and the SDK still performs both
        // secret retrieval requests and response decoding.
        authenticate_client(&mut client, token)?;

        let access_key = fetch_secret(&client, ACCESS_KEY_ID, &configuration.project_id);
        let secret_key = fetch_secret(&client, SECRET_ACCESS_KEY, &configuration.project_id);
        let (access_key_id, secret_access_key) = tokio::try_join!(access_key, secret_key)?;

        Ok(R2Credentials {
            access_key_id,
            secret_access_key,
        })
    }
}

fn authenticate_client(client: &mut Client, token: &SecretString) -> Result<(), SafeReason> {
    let bearer = SecretString::from(format!("Bearer {}", token.expose_secret()));
    let mut value = HeaderValue::from_str(bearer.expose_secret())
        .map_err(|_| SafeReason::AuthenticationExpiredOrRejected)?;
    value.set_sensitive(true);

    let mut headers = HeaderMap::new();
    headers.insert(AUTHORIZATION, value);
    client.http_client = reqwest_0_12::Client::builder()
        .timeout(Duration::from_secs(5))
        .user_agent(concat!("baml-sccache/", env!("CARGO_PKG_VERSION")))
        .use_rustls_tls()
        .redirect(reqwest_0_12::redirect::Policy::none())
        .default_headers(headers)
        .build()
        .map_err(|_| SafeReason::InvalidResponse)?;
    client.logged_in = true;
    Ok(())
}

async fn fetch_secret(
    client: &Client,
    name: &'static str,
    project_id: &str,
) -> Result<SecretString, SafeReason> {
    let request = GetSecretRequest::builder(name, project_id, INFISICAL_ENVIRONMENT)
        .path(INFISICAL_SECRET_PATH)
        .expand_secret_references(false)
        .build();
    let secret = client
        .secrets()
        .get(request)
        .await
        .map_err(classify_infisical_error)?;
    if secret.secret_value.is_empty() {
        return Err(SafeReason::SecretMissing);
    }
    Ok(SecretString::from(secret.secret_value))
}

fn classify_infisical_error(error: InfisicalError) -> SafeReason {
    match error {
        InfisicalError::HttpError { status, .. } if status.as_u16() == 401 => {
            SafeReason::AuthenticationExpiredOrRejected
        }
        InfisicalError::HttpError { status, .. } if status.as_u16() == 403 => {
            SafeReason::AccessDenied
        }
        InfisicalError::HttpError { status, .. } if status.as_u16() == 404 => {
            SafeReason::SecretMissing
        }
        InfisicalError::RequestError(error) if error.is_connect() || error.is_timeout() => {
            SafeReason::NetworkUnavailable
        }
        InfisicalError::NotAuthenticated | InfisicalError::InvalidAuthMethod => {
            SafeReason::AuthenticationExpiredOrRejected
        }
        _ => SafeReason::InvalidResponse,
    }
}

trait Environment {
    fn var_os(&self, name: &str) -> Option<OsString>;
}

struct ProcessEnvironment;

impl Environment for ProcessEnvironment {
    fn var_os(&self, name: &str) -> Option<OsString> {
        std::env::var_os(name)
    }
}

#[derive(Clone, Copy)]
struct RuntimeContext {
    is_macos: bool,
    is_ci: bool,
    server_running: bool,
}

enum Resolution {
    Remote(R2Credentials, SccacheConfiguration),
    Local,
    LocalFallback(SafeReason),
    Passthrough,
}

async fn resolve_credentials(
    environment: &impl Environment,
    context: RuntimeContext,
    provider: &impl CredentialProvider,
) -> Resolution {
    match explicit_credentials(environment) {
        ExplicitCredentials::Complete(credentials) => {
            return match SccacheConfiguration::from_environment(environment) {
                Ok(configuration) => Resolution::Remote(credentials, configuration),
                Err(reason) => Resolution::LocalFallback(reason),
            };
        }
        ExplicitCredentials::Partial => {
            return Resolution::LocalFallback(SafeReason::PartialExplicitCredentials);
        }
        ExplicitCredentials::Absent => {}
    }

    if context.server_running {
        return Resolution::Passthrough;
    }
    if context.is_ci || !automatic_lookup_enabled(environment, context) {
        return Resolution::Local;
    }

    let configuration = match SccacheConfiguration::from_environment(environment) {
        Ok(configuration) => configuration,
        Err(reason) => return Resolution::LocalFallback(reason),
    };

    match provider.fetch().await {
        Ok(credentials) => Resolution::Remote(credentials, configuration),
        Err(reason) => Resolution::LocalFallback(reason),
    }
}

enum ExplicitCredentials {
    Complete(R2Credentials),
    Partial,
    Absent,
}

fn explicit_credentials(environment: &impl Environment) -> ExplicitCredentials {
    let canonical_access_key = nonempty_utf8(environment.var_os(ACCESS_KEY_ID));
    let legacy_access_key = nonempty_utf8(environment.var_os(LEGACY_ACCESS_KEY));
    let secret_access_key = nonempty_utf8(environment.var_os(SECRET_ACCESS_KEY));
    let access_key = canonical_access_key.or(legacy_access_key);

    match (access_key, secret_access_key) {
        (Some(access_key_id), Some(secret_access_key)) => {
            ExplicitCredentials::Complete(R2Credentials::new(access_key_id, secret_access_key))
        }
        (None, None) => ExplicitCredentials::Absent,
        _ => ExplicitCredentials::Partial,
    }
}

fn nonempty_utf8(value: Option<OsString>) -> Option<String> {
    value
        .and_then(|value| value.into_string().ok())
        .filter(|value| !value.is_empty())
}

fn automatic_lookup_enabled(environment: &impl Environment, context: RuntimeContext) -> bool {
    match nonempty_utf8(environment.var_os(INFISICAL_CONTROL)).as_deref() {
        Some("0" | "false" | "off") => false,
        Some("1" | "true" | "on") => true,
        _ => context.is_macos,
    }
}

fn is_ci(environment: &impl Environment) -> bool {
    ["CI", "GITHUB_ACTIONS"]
        .into_iter()
        .filter_map(|name| nonempty_utf8(environment.var_os(name)))
        .any(|value| !matches!(value.as_str(), "0" | "false" | "off"))
}

#[cfg(unix)]
fn server_running(environment: &impl Environment) -> bool {
    use std::os::unix::net::UnixStream;

    if let Some(path) = environment.var_os("SCCACHE_SERVER_UDS") {
        return UnixStream::connect(path).is_ok();
    }
    tcp_server_running(environment)
}

#[cfg(not(unix))]
fn server_running(environment: &impl Environment) -> bool {
    tcp_server_running(environment)
}

fn tcp_server_address(environment: &impl Environment) -> SocketAddr {
    let port = nonempty_utf8(environment.var_os("SCCACHE_SERVER_PORT"))
        .and_then(|value| value.parse().ok())
        .unwrap_or(4226);
    SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, port))
}

fn tcp_server_running(environment: &impl Environment) -> bool {
    tcp_endpoint_running(tcp_server_address(environment))
}

fn tcp_endpoint_running(address: SocketAddr) -> bool {
    TcpStream::connect_timeout(&address, Duration::from_millis(50)).is_ok()
}

enum ChildEnvironment {
    Remote(R2Credentials, SccacheConfiguration),
    Local,
    Passthrough,
}

impl ChildEnvironment {
    fn from_resolution(resolution: Resolution) -> (Self, Option<SafeReason>) {
        match resolution {
            Resolution::Remote(credentials, configuration) => {
                (Self::Remote(credentials, configuration), None)
            }
            Resolution::Local => (Self::Local, None),
            Resolution::LocalFallback(reason) => (Self::Local, Some(reason)),
            Resolution::Passthrough => (Self::Passthrough, None),
        }
    }

    fn apply(&self, command: &mut Command, _environment: &impl Environment) {
        command.env_remove(INFISICAL_TOKEN);

        match self {
            Self::Remote(credentials, configuration) => {
                for name in [ACCESS_KEY_ID, LEGACY_ACCESS_KEY, SECRET_ACCESS_KEY] {
                    command.env_remove(name);
                }
                command
                    .env(
                        "AWS_ACCESS_KEY_ID",
                        credentials.access_key_id.expose_secret(),
                    )
                    .env(
                        "AWS_SECRET_ACCESS_KEY",
                        credentials.secret_access_key.expose_secret(),
                    )
                    .env("SCCACHE_BUCKET", &configuration.bucket)
                    .env("SCCACHE_REGION", &configuration.region)
                    .env("SCCACHE_ENDPOINT", &configuration.endpoint)
                    .env("SCCACHE_S3_KEY_PREFIX", &configuration.key_prefix);
            }
            Self::Local => {
                for name in [
                    ACCESS_KEY_ID,
                    LEGACY_ACCESS_KEY,
                    SECRET_ACCESS_KEY,
                    "AWS_ACCESS_KEY_ID",
                    "AWS_SECRET_ACCESS_KEY",
                ] {
                    command.env_remove(name);
                }
                for name in SCCACHE_CONFIGURATION {
                    command.env_remove(name);
                }
            }
            Self::Passthrough => {}
        }
    }
}

fn repository_root_from(start: &Path) -> Option<PathBuf> {
    start
        .ancestors()
        .find(|candidate| {
            candidate.join(".envrc").is_file() && candidate.join(INFISICAL_PROJECT_FILE).is_file()
        })
        .map(Path::to_path_buf)
}

fn find_repository_root() -> Option<PathBuf> {
    std::env::current_dir()
        .ok()
        .and_then(|path| repository_root_from(&path))
        .or_else(|| {
            std::env::current_exe()
                .ok()
                .and_then(|path| repository_root_from(&path))
        })
}

fn direnv_command(
    repository_root: &Path,
    executable: &Path,
    arguments: impl IntoIterator<Item = OsString>,
) -> Command {
    let mut command = Command::new("direnv");
    command
        .arg("exec")
        .arg(repository_root)
        .arg(executable)
        .args(arguments);
    command
}

fn direnv_needs_loading(environment: &impl Environment) -> bool {
    environment.var_os(DIRENV_LOADED).is_none()
}

fn run_through_direnv_if_needed(
    environment: &impl Environment,
    repository_root: Option<&Path>,
) -> Result<Option<ExitStatus>, io::Error> {
    if !direnv_needs_loading(environment) {
        return Ok(None);
    }
    let Some(repository_root) = repository_root else {
        return Ok(None);
    };
    let executable = std::env::current_exe()?;
    let mut command = direnv_command(repository_root, &executable, std::env::args_os().skip(1));
    match command.status() {
        Ok(status) => Ok(Some(status)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

async fn run() -> Result<ExitStatus, io::Error> {
    let environment = ProcessEnvironment;
    let repository_root = find_repository_root();
    if let Some(status) = run_through_direnv_if_needed(&environment, repository_root.as_deref())? {
        return Ok(status);
    }
    let authentication = EnvironmentTokenProvider::from_environment(&environment);
    let provider = InfisicalCredentialProvider::new(authentication, repository_root);
    let context = RuntimeContext {
        is_macos: cfg!(target_os = "macos"),
        is_ci: is_ci(&environment),
        server_running: server_running(&environment),
    };
    let resolution = resolve_credentials(&environment, context, &provider).await;
    let (child_environment, fallback_reason) = ChildEnvironment::from_resolution(resolution);
    if let Some(reason) = fallback_reason {
        let _ = writeln!(
            io::stderr().lock(),
            "baml-sccache: R2 cache disabled: {}",
            reason.message()
        );
    }

    let mut command = Command::new("sccache");
    command.args(std::env::args_os().skip(1));
    child_environment.apply(&mut command, &environment);
    command.status()
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    match run().await {
        Ok(status) => exit(status.code().unwrap_or(1)),
        Err(error) => {
            let _ = writeln!(
                io::stderr().lock(),
                "baml-sccache: failed to start direnv or sccache: {error}"
            );
            exit(127);
        }
    }
}
