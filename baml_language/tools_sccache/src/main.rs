//! Native `RUSTC_WRAPPER` shim for BAML's shared sccache R2 cache.
//!
//! Explicit BAML-prefixed credentials are mapped to the AWS names sccache
//! consumes. Local macOS developers can instead hand a short-lived Infisical
//! human-session token to this process; the official Infisical Rust SDK then
//! retrieves the R2 pair and only the spawned sccache process receives it.

use std::{
    ffi::OsString,
    io::{self, Write},
    process::{Command, ExitStatus, exit},
    time::Duration,
};

use async_trait::async_trait;
use infisical::{Client, InfisicalError, secrets::GetSecretRequest};
use reqwest_0_12::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use secrecy::{ExposeSecret, SecretString};

const ACCESS_KEY_ID: &str = "BAML_SCCACHE_R2_ACCESS_KEY_ID";
const LEGACY_ACCESS_KEY: &str = "BAML_SCCACHE_R2_ACCESS_KEY";
const SECRET_ACCESS_KEY: &str = "BAML_SCCACHE_R2_SECRET_ACCESS_KEY";
const INFISICAL_TOKEN: &str = "INFISICAL_TOKEN";
const INFISICAL_CONTROL: &str = "BAML_SCCACHE_INFISICAL";

const INFISICAL_BASE_URL: &str = "https://app.infisical.com";
const INFISICAL_PROJECT_ID: &str = "bdd280e2-259c-4750-9b16-a8597a67214c";
const INFISICAL_ENVIRONMENT: &str = "dev-humans";
const INFISICAL_SECRET_PATH: &str = "/";

const SCCACHE_BUCKET: &str = "baml-build1";
const SCCACHE_REGION: &str = "auto";
const SCCACHE_ENDPOINT: &str = "https://321ca319116f1e5eefa9135d9d019a5a.r2.cloudflarestorage.com";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SafeReason {
    PartialExplicitCredentials,
    MissingSessionToken,
    AuthenticationExpiredOrRejected,
    AccessDenied,
    SecretMissing,
    NetworkUnavailable,
    InvalidResponse,
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
                "the Infisical session cannot access the boundary-tools dev-humans cache secrets"
            }
            Self::SecretMissing => {
                "an R2 cache secret is missing from boundary-tools/dev-humans at /"
            }
            Self::NetworkUnavailable => {
                "Infisical is unavailable; check the network and restart sccache to retry"
            }
            Self::InvalidResponse => {
                "Infisical returned an unusable response; restart sccache to retry"
            }
        }
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
}

impl<A> InfisicalCredentialProvider<A> {
    fn new(authentication: A) -> Self {
        Self { authentication }
    }
}

#[async_trait]
impl<A> CredentialProvider for InfisicalCredentialProvider<A>
where
    A: AuthenticationProvider + Sync,
{
    async fn fetch(&self) -> Result<R2Credentials, SafeReason> {
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

        let access_key = fetch_secret(&client, ACCESS_KEY_ID);
        let secret_key = fetch_secret(&client, SECRET_ACCESS_KEY);
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
        .default_headers(headers)
        .build()
        .map_err(|_| SafeReason::InvalidResponse)?;
    client.logged_in = true;
    Ok(())
}

async fn fetch_secret(client: &Client, name: &'static str) -> Result<SecretString, SafeReason> {
    let request = GetSecretRequest::builder(name, INFISICAL_PROJECT_ID, INFISICAL_ENVIRONMENT)
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
    Remote(R2Credentials),
    LocalFallback(SafeReason),
    Passthrough,
}

async fn resolve_credentials(
    environment: &impl Environment,
    context: RuntimeContext,
    provider: &impl CredentialProvider,
) -> Resolution {
    match explicit_credentials(environment) {
        ExplicitCredentials::Complete(credentials) => return Resolution::Remote(credentials),
        ExplicitCredentials::Partial => {
            return Resolution::LocalFallback(SafeReason::PartialExplicitCredentials);
        }
        ExplicitCredentials::Absent => {}
    }

    if context.is_ci || context.server_running || !automatic_lookup_enabled(environment, context) {
        return Resolution::Passthrough;
    }

    match provider.fetch().await {
        Ok(credentials) => Resolution::Remote(credentials),
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
    use std::os::unix::fs::FileTypeExt;

    environment
        .var_os("SCCACHE_SERVER_UDS")
        .and_then(|path| std::fs::metadata(path).ok())
        .is_some_and(|metadata| metadata.file_type().is_socket())
}

#[cfg(not(unix))]
fn server_running(_environment: &impl Environment) -> bool {
    false
}

enum ChildEnvironment {
    Remote(R2Credentials),
    Local,
    Passthrough,
}

impl ChildEnvironment {
    fn from_resolution(resolution: Resolution) -> (Self, Option<SafeReason>) {
        match resolution {
            Resolution::Remote(credentials) => (Self::Remote(credentials), None),
            Resolution::LocalFallback(reason) => (Self::Local, Some(reason)),
            Resolution::Passthrough => (Self::Passthrough, None),
        }
    }

    fn apply(&self, command: &mut Command, environment: &impl Environment) {
        command.env_remove(INFISICAL_TOKEN);

        match self {
            Self::Remote(credentials) => {
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
                    .env("SCCACHE_BUCKET", SCCACHE_BUCKET)
                    .env("SCCACHE_REGION", SCCACHE_REGION)
                    .env("SCCACHE_ENDPOINT", SCCACHE_ENDPOINT)
                    .env(
                        "SCCACHE_S3_KEY_PREFIX",
                        if is_ci(environment) {
                            "baml/ci/"
                        } else {
                            "baml/local/"
                        },
                    );
            }
            Self::Local => {
                for name in [
                    ACCESS_KEY_ID,
                    LEGACY_ACCESS_KEY,
                    SECRET_ACCESS_KEY,
                    "AWS_ACCESS_KEY_ID",
                    "AWS_SECRET_ACCESS_KEY",
                    "SCCACHE_BUCKET",
                    "SCCACHE_REGION",
                    "SCCACHE_ENDPOINT",
                    "SCCACHE_S3_KEY_PREFIX",
                ] {
                    command.env_remove(name);
                }
            }
            Self::Passthrough => {}
        }
    }
}

async fn run() -> Result<ExitStatus, io::Error> {
    let environment = ProcessEnvironment;
    let authentication = EnvironmentTokenProvider::from_environment(&environment);
    let provider = InfisicalCredentialProvider::new(authentication);
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
                "baml-sccache: failed to spawn sccache: {error}"
            );
            exit(127);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        ffi::OsStr,
        sync::atomic::{AtomicUsize, Ordering},
    };

    use super::*;

    #[derive(Default)]
    struct FakeEnvironment(HashMap<&'static str, OsString>);

    impl FakeEnvironment {
        fn with(mut self, name: &'static str, value: &'static str) -> Self {
            self.0.insert(name, OsString::from(value));
            self
        }
    }

    impl Environment for FakeEnvironment {
        fn var_os(&self, name: &str) -> Option<OsString> {
            self.0.get(name).cloned()
        }
    }

    struct FakeProvider {
        calls: AtomicUsize,
        result: Result<(&'static str, &'static str), SafeReason>,
    }

    impl FakeProvider {
        fn success() -> Self {
            Self {
                calls: AtomicUsize::new(0),
                result: Ok(("fake-access", "fake-secret")),
            }
        }

        fn failure(reason: SafeReason) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                result: Err(reason),
            }
        }
    }

    #[async_trait]
    impl CredentialProvider for FakeProvider {
        async fn fetch(&self) -> Result<R2Credentials, SafeReason> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.result
                .map(|(access, secret)| R2Credentials::new(access.into(), secret.into()))
        }
    }

    fn local_macos() -> RuntimeContext {
        RuntimeContext {
            is_macos: true,
            is_ci: false,
            server_running: false,
        }
    }

    fn expect_remote(resolution: Resolution) -> R2Credentials {
        match resolution {
            Resolution::Remote(credentials) => credentials,
            Resolution::LocalFallback(_) | Resolution::Passthrough => panic!("expected remote"),
        }
    }

    #[tokio::test]
    async fn explicit_credentials_have_precedence_and_skip_infisical() {
        let environment = FakeEnvironment::default()
            .with(ACCESS_KEY_ID, "explicit-access")
            .with(SECRET_ACCESS_KEY, "explicit-secret")
            .with(INFISICAL_TOKEN, "session-token");
        let provider = FakeProvider::success();

        let credentials =
            expect_remote(resolve_credentials(&environment, local_macos(), &provider).await);

        assert_eq!(credentials.access_key_id.expose_secret(), "explicit-access");
        assert_eq!(
            credentials.secret_access_key.expose_secret(),
            "explicit-secret"
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn github_ci_forwards_explicit_baml_pair_without_infisical() {
        let environment = FakeEnvironment::default()
            .with(ACCESS_KEY_ID, "ci-access")
            .with(SECRET_ACCESS_KEY, "ci-secret")
            .with("GITHUB_ACTIONS", "true")
            .with(INFISICAL_CONTROL, "1");
        let provider = FakeProvider::success();
        let context = RuntimeContext {
            is_macos: false,
            is_ci: true,
            server_running: false,
        };

        let credentials =
            expect_remote(resolve_credentials(&environment, context, &provider).await);
        let child = ChildEnvironment::Remote(credentials);
        let mut command = Command::new("sccache");
        child.apply(&mut command, &environment);
        let values: HashMap<_, _> = command.get_envs().collect();

        assert_eq!(
            values.get(OsStr::new("AWS_ACCESS_KEY_ID")),
            Some(&Some(OsStr::new("ci-access")))
        );
        assert_eq!(
            values.get(OsStr::new("SCCACHE_S3_KEY_PREFIX")),
            Some(&Some(OsStr::new("baml/ci/")))
        );
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn legacy_access_key_alias_remains_supported() {
        let environment = FakeEnvironment::default()
            .with(LEGACY_ACCESS_KEY, "legacy-access")
            .with(SECRET_ACCESS_KEY, "explicit-secret");
        let provider = FakeProvider::success();

        let credentials =
            expect_remote(resolve_credentials(&environment, local_macos(), &provider).await);

        assert_eq!(credentials.access_key_id.expose_secret(), "legacy-access");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn partial_explicit_credentials_disable_remote_without_lookup() {
        let environment = FakeEnvironment::default()
            .with(ACCESS_KEY_ID, "partial-access")
            .with(INFISICAL_TOKEN, "session-token");
        let provider = FakeProvider::success();

        let resolution = resolve_credentials(&environment, local_macos(), &provider).await;

        assert!(matches!(
            resolution,
            Resolution::LocalFallback(SafeReason::PartialExplicitCredentials)
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn automatic_lookup_is_local_macos_only_and_never_ci() {
        let environment = FakeEnvironment::default().with(INFISICAL_CONTROL, "1");
        let provider = FakeProvider::success();
        let ci = RuntimeContext {
            is_macos: true,
            is_ci: true,
            server_running: false,
        };
        let other_platform = RuntimeContext {
            is_macos: false,
            is_ci: false,
            server_running: false,
        };

        assert!(matches!(
            resolve_credentials(&environment, ci, &provider).await,
            Resolution::Passthrough
        ));
        assert!(matches!(
            resolve_credentials(&FakeEnvironment::default(), other_platform, &provider).await,
            Resolution::Passthrough
        ));
        assert!(matches!(
            resolve_credentials(&environment, other_platform, &provider).await,
            Resolution::Remote(_)
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn opt_out_and_running_server_skip_lookup() {
        let disabled = FakeEnvironment::default().with(INFISICAL_CONTROL, "0");
        let provider = FakeProvider::success();
        let running = RuntimeContext {
            server_running: true,
            ..local_macos()
        };

        assert!(matches!(
            resolve_credentials(&disabled, local_macos(), &provider).await,
            Resolution::Passthrough
        ));
        assert!(matches!(
            resolve_credentials(&FakeEnvironment::default(), running, &provider).await,
            Resolution::Passthrough
        ));
        assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn authenticated_provider_credentials_are_used() {
        let environment = FakeEnvironment::default().with(INFISICAL_TOKEN, "session-token");
        let provider = FakeProvider::success();

        let credentials =
            expect_remote(resolve_credentials(&environment, local_macos(), &provider).await);

        assert_eq!(credentials.access_key_id.expose_secret(), "fake-access");
        assert_eq!(credentials.secret_access_key.expose_secret(), "fake-secret");
        assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn lookup_errors_fall_back_to_local_cache_with_safe_reasons() {
        for reason in [
            SafeReason::MissingSessionToken,
            SafeReason::AuthenticationExpiredOrRejected,
            SafeReason::AccessDenied,
            SafeReason::NetworkUnavailable,
            SafeReason::SecretMissing,
        ] {
            let provider = FakeProvider::failure(reason);
            let resolution = resolve_credentials(
                &FakeEnvironment::default().with(INFISICAL_TOKEN, "session-token"),
                local_macos(),
                &provider,
            )
            .await;
            assert!(matches!(resolution, Resolution::LocalFallback(actual) if actual == reason));
        }
    }

    #[test]
    fn authentication_provider_debug_is_redacted() {
        let provider = EnvironmentTokenProvider {
            token: Some(SecretString::from("super-secret-token".to_owned())),
        };
        let rendered = format!("{provider:?}");

        assert!(rendered.contains("[REDACTED]"));
        assert!(!rendered.contains("super-secret-token"));
    }

    #[test]
    fn child_environment_maps_remote_credentials_and_strips_parent_names() {
        let environment = FakeEnvironment::default();
        let child = ChildEnvironment::Remote(R2Credentials::new(
            "child-access".into(),
            "child-secret".into(),
        ));
        let mut command = Command::new("sccache");

        child.apply(&mut command, &environment);
        let values: HashMap<_, _> = command.get_envs().collect();

        assert_eq!(
            values.get(OsStr::new("AWS_ACCESS_KEY_ID")),
            Some(&Some(OsStr::new("child-access")))
        );
        assert_eq!(
            values.get(OsStr::new("AWS_SECRET_ACCESS_KEY")),
            Some(&Some(OsStr::new("child-secret")))
        );
        assert_eq!(values.get(OsStr::new(ACCESS_KEY_ID)), Some(&None));
        assert_eq!(values.get(OsStr::new(INFISICAL_TOKEN)), Some(&None));
    }

    #[test]
    fn local_child_environment_removes_all_remote_configuration() {
        let mut command = Command::new("sccache");
        ChildEnvironment::Local.apply(&mut command, &FakeEnvironment::default());
        let values: HashMap<_, _> = command.get_envs().collect();

        for name in [
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "SCCACHE_BUCKET",
            "SCCACHE_ENDPOINT",
            INFISICAL_TOKEN,
        ] {
            assert_eq!(values.get(OsStr::new(name)), Some(&None));
        }
    }
}
