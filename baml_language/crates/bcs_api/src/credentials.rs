//! Process-local authorization shared across operations, with request targets kept separate.
use std::{
    collections::HashMap,
    fmt,
    sync::{Arc, Mutex, OnceLock, Weak},
    time::{SystemTime, UNIX_EPOCH},
};

use futures::channel::oneshot;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

pub const SESSION_PREFIX: &str = "bdry_session_";
pub const ACCESS_PREFIX: &str = "bdry_access_";
pub const SECRET_PREFIX: &str = "bdry_secret_";
pub const PUBLIC_PREFIX: &str = "bdry_public_";
pub const ACCESS_TOKEN_HEADER: &str = "boundary-access-token";
pub const ACCESS_EXPIRY_HEADER: &str = "boundary-access-expires-at";

#[derive(Clone, Deserialize, Serialize)]
#[serde(transparent)]
pub struct Secret(String);
impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }
    pub fn expose(&self) -> &str {
        &self.0
    }
}
impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Target {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub org_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_id: Option<String>,
}

#[derive(Debug)]
struct CachedAccess {
    token: Secret,
    expires_at: u64,
}
#[derive(Debug, Default)]
struct Cache {
    access: Option<CachedAccess>,
    renewing: bool,
    waiters: Vec<oneshot::Sender<Result<(), RenewalError>>>,
}
impl Cache {
    fn valid_access(&self) -> Option<&CachedAccess> {
        self.access
            .as_ref()
            .filter(|grant| grant.expires_at > now())
    }
}
#[derive(Debug)]
struct State {
    endpoint: String,
    credential: Secret,
    cache: Mutex<Cache>,
}

/// Clones share authorization; independently constructed clients share it only for the same
/// endpoint and original credential. Neither credentials nor access grants enter diagnostics.
#[derive(Clone, Debug)]
pub struct Authentication(Arc<State>);
type CacheKey = (String, [u8; 32]);
type Registry = HashMap<CacheKey, Weak<State>>;
static AUTHENTICATIONS: OnceLock<Mutex<Registry>> = OnceLock::new();

impl Authentication {
    pub fn shared(endpoint: &str, credential: Secret) -> Self {
        let hash: [u8; 32] = Sha256::digest(credential.expose().as_bytes()).into();
        let mut registry = AUTHENTICATIONS
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        registry.retain(|_, state| state.strong_count() > 0);
        let key = (endpoint.trim_end_matches('/').to_owned(), hash);
        if let Some(state) = registry.get(&key).and_then(Weak::upgrade) {
            return Self(state);
        }
        let state = Arc::new(State {
            endpoint: key.0.clone(),
            credential,
            cache: Mutex::new(Cache::default()),
        });
        registry.insert(key, Arc::downgrade(&state));
        Self(state)
    }
    pub fn endpoint(&self) -> &str {
        &self.0.endpoint
    }
    pub fn accepts_url(&self, url: &reqwest::Url) -> bool {
        url.as_str()
            .strip_prefix(&self.0.endpoint)
            .is_some_and(|suffix| suffix.starts_with('/'))
    }
    /// Snapshot the current token for validation. HTTP transports use `acquire_*` instead,
    /// so sending a session credential participates in shared renewal coordination.
    pub fn bearer(&self) -> Secret {
        let cache = self
            .0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        cache
            .valid_access()
            .map_or_else(|| self.0.credential.clone(), |grant| grant.token.clone())
    }
    /// Elect one operation to bootstrap/renew; all other operations await its result.
    /// Blocking HTTP callers park their thread, while async HTTP callers yield their task.
    #[cfg(any(feature = "auth", test))]
    pub(crate) fn acquire_blocking(&self) -> Result<AuthorizationAttempt, RenewalError> {
        loop {
            match self.acquire() {
                Acquisition::Ready(attempt) => return Ok(attempt),
                Acquisition::Wait(waiter) => {
                    futures::executor::block_on(waiter).map_err(|_| RenewalError::Aborted)??;
                }
            }
        }
    }
    pub(crate) async fn acquire_async(&self) -> Result<AuthorizationAttempt, RenewalError> {
        loop {
            match self.acquire() {
                Acquisition::Ready(attempt) => return Ok(attempt),
                Acquisition::Wait(waiter) => {
                    waiter.await.map_err(|_| RenewalError::Aborted)??;
                }
            }
        }
    }
    fn acquire(&self) -> Acquisition {
        let mut cache = self
            .0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(grant) = cache.valid_access() {
            return Acquisition::Ready(AuthorizationAttempt::new(self, grant.token.clone(), false));
        }
        if !self.0.credential.expose().starts_with(SESSION_PREFIX) {
            return Acquisition::Ready(AuthorizationAttempt::new(
                self,
                self.0.credential.clone(),
                false,
            ));
        }
        if cache.renewing {
            let (sender, receiver) = oneshot::channel();
            cache.waiters.push(sender);
            return Acquisition::Wait(receiver);
        }
        cache.renewing = true;
        Acquisition::Ready(AuthorizationAttempt::new(
            self,
            self.0.credential.clone(),
            true,
        ))
    }
    fn complete_renewal(&self, failure: Option<RenewalError>) {
        let (waiters, outcome) = {
            let mut cache = self
                .0
                .cache
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            cache.renewing = false;
            let outcome = match failure {
                Some(error) => Err(error),
                None if cache.valid_access().is_some() => Ok(()),
                None => Err(RenewalError::MissingAccessToken),
            };
            (std::mem::take(&mut cache.waiters), outcome)
        };
        // Waking tasks can execute arbitrary caller code; never wake them under the cache lock.
        for waiter in waiters {
            let _ = waiter.send(outcome);
        }
    }
    /// Accept headers only from a successful response from this client's selected gateway.
    pub fn observe(&self, status: reqwest::StatusCode, headers: &reqwest::header::HeaderMap) {
        if !status.is_success() || !self.0.credential.expose().starts_with(SESSION_PREFIX) {
            return;
        }
        let token = headers
            .get(ACCESS_TOKEN_HEADER)
            .and_then(|v| v.to_str().ok());
        let expiry = headers
            .get(ACCESS_EXPIRY_HEADER)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let Some((token, expires_at)) = token.zip(expiry) else {
            return;
        };
        let now = now();
        if !token.starts_with(ACCESS_PREFIX)
            || token.len() > 64 * 1024
            || expires_at <= now
            || expires_at > now.saturating_add(300)
        {
            return;
        }
        let mut cache = self
            .0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache
            .access
            .as_ref()
            .is_none_or(|grant| expires_at >= grant.expires_at)
        {
            cache.access = Some(CachedAccess {
                token: Secret::new(token.to_owned()),
                expires_at,
            });
        }
    }
    /// A rejected cached token may be retried once. Clear only the token this request used;
    /// if another operation renewed it concurrently, the retry uses that newer token.
    pub fn rejected(&self, sent: &Secret) -> bool {
        if !self.0.credential.expose().starts_with(SESSION_PREFIX)
            || !sent.expose().starts_with(ACCESS_PREFIX)
        {
            return false;
        }
        let mut cache = self
            .0
            .cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache
            .access
            .as_ref()
            .is_some_and(|grant| grant.token.expose() == sent.expose())
        {
            cache.access = None;
        }
        true
    }
}

enum Acquisition {
    Ready(AuthorizationAttempt),
    Wait(oneshot::Receiver<Result<(), RenewalError>>),
}

/// A failed shared renewal does not trigger another bootstrap for every waiting operation.
/// Subsequent operations may try again; the original operation retains its own response/error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenewalError {
    Rejected(reqwest::StatusCode),
    MissingAccessToken,
    Aborted,
}
impl fmt::Display for RenewalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected(status) => write!(f, "Boundary credential renewal failed ({status})"),
            Self::MissingAccessToken => {
                f.write_str("Boundary credential renewal returned no usable access token")
            }
            Self::Aborted => f.write_str("Boundary credential renewal did not complete"),
        }
    }
}
impl std::error::Error for RenewalError {}

/// Owns renewal leadership until response headers arrive. Drop also handles transport failures,
/// panics and async cancellation, ensuring that waiters cannot be stranded.
pub(crate) struct AuthorizationAttempt {
    authentication: Authentication,
    token: Secret,
    renewing: bool,
}
impl AuthorizationAttempt {
    fn new(authentication: &Authentication, token: Secret, renewing: bool) -> Self {
        Self {
            authentication: authentication.clone(),
            token,
            renewing,
        }
    }
    pub(crate) fn token(&self) -> &Secret {
        &self.token
    }
    pub(crate) fn complete(
        mut self,
        status: reqwest::StatusCode,
        headers: &reqwest::header::HeaderMap,
    ) {
        self.authentication.observe(status, headers);
        if self.renewing {
            self.renewing = false;
            self.authentication
                .complete_renewal((!status.is_success()).then_some(RenewalError::Rejected(status)));
        }
    }
}
impl Drop for AuthorizationAttempt {
    fn drop(&mut self) {
        if self.renewing {
            self.authentication
                .complete_renewal(Some(RenewalError::Aborted));
        }
    }
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs())
}

#[derive(Debug, Clone)]
pub struct RequestAuthorization {
    pub authentication: Authentication,
    pub target: Option<Target>,
}

#[derive(Serialize)]
pub struct Targeted<'a, T> {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<&'a Target>,
    #[serde(flatten)]
    pub payload: T,
}

#[cfg(test)]
mod tests {
    use reqwest::{
        StatusCode,
        header::{HeaderMap, HeaderValue},
    };

    use super::*;
    fn headers(token: &str, expiry: u64) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(ACCESS_TOKEN_HEADER, HeaderValue::from_str(token).unwrap());
        headers.insert(ACCESS_EXPIRY_HEADER, HeaderValue::from(expiry));
        headers
    }
    async fn wait_for_waiters(auth: &Authentication, count: usize) {
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if auth.0.cache.lock().unwrap().waiters.len() == count {
                    break;
                }
                tokio::task::yield_now().await;
            }
        })
        .await
        .expect("requests should join the in-flight renewal");
    }

    // The default single-thread async executor also proves waiters yield rather than block it.
    #[tokio::test]
    async fn bootstrap_and_expiry_share_one_renewal_across_threads_and_tasks() {
        for expired in [false, true] {
            let auth = Authentication::shared(
                "https://single-renewal.example.test",
                Secret::new(format!("bdry_session_single-{expired}")),
            );
            if expired {
                auth.0.cache.lock().unwrap().access = Some(CachedAccess {
                    token: Secret::new("bdry_access_expired".into()),
                    expires_at: 0,
                });
            }
            let leader = auth.acquire_async().await.unwrap();
            assert!(leader.renewing);
            assert!(leader.token().expose().starts_with(SESSION_PREFIX));
            let mut tasks = Vec::new();
            for _ in 0..8 {
                let blocking = auth.clone();
                tasks.push(tokio::task::spawn_blocking(move || {
                    let attempt = blocking.acquire_blocking().unwrap();
                    assert!(!attempt.renewing);
                    attempt.token().expose().to_owned()
                }));
                let asynchronous = auth.clone();
                tasks.push(tokio::spawn(async move {
                    let attempt = asynchronous.acquire_async().await.unwrap();
                    assert!(!attempt.renewing);
                    attempt.token().expose().to_owned()
                }));
            }
            wait_for_waiters(&auth, tasks.len()).await;
            leader.complete(StatusCode::OK, &headers("bdry_access_single", now() + 250));
            for task in tasks {
                assert_eq!(task.await.unwrap(), "bdry_access_single");
            }
            assert!(!auth.0.cache.lock().unwrap().renewing);
        }
    }

    #[tokio::test]
    async fn failed_renewal_releases_the_cohort_without_starting_more_renewals() {
        for status in [
            StatusCode::UNAUTHORIZED,
            StatusCode::FORBIDDEN,
            StatusCode::SERVICE_UNAVAILABLE,
            StatusCode::OK,
        ] {
            let auth = Authentication::shared(
                "https://failed-renewal.example.test",
                Secret::new(format!("bdry_session_failure-{status}")),
            );
            let leader = auth.acquire_async().await.unwrap();
            let mut tasks = Vec::new();
            for _ in 0..8 {
                let blocking = auth.clone();
                tasks.push(tokio::task::spawn_blocking(move || {
                    blocking.acquire_blocking().err()
                }));
                let asynchronous = auth.clone();
                tasks.push(tokio::spawn(async move {
                    asynchronous.acquire_async().await.err()
                }));
            }
            wait_for_waiters(&auth, tasks.len()).await;
            leader.complete(status, &HeaderMap::new());
            let expected = if status.is_success() {
                RenewalError::MissingAccessToken
            } else {
                RenewalError::Rejected(status)
            };
            for task in tasks {
                assert_eq!(task.await.unwrap(), Some(expected));
            }
            {
                let cache = auth.0.cache.lock().unwrap();
                assert!(!cache.renewing);
                assert!(cache.waiters.is_empty());
            }
            // A future independent operation can recover after the failed cohort finishes.
            assert!(auth.acquire_async().await.unwrap().renewing);
        }
    }

    #[tokio::test]
    async fn cancelling_the_leader_releases_waiters_and_allows_recovery() {
        let auth = Authentication::shared(
            "https://cancel-renewal.example.test",
            Secret::new("bdry_session_cancel".into()),
        );
        let leader_auth = auth.clone();
        let (ready, leader_ready) = oneshot::channel();
        let leader = tokio::spawn(async move {
            let _attempt = leader_auth.acquire_async().await.unwrap();
            ready.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        leader_ready.await.unwrap();
        let blocking = auth.clone();
        let blocked = tokio::task::spawn_blocking(move || blocking.acquire_blocking().err());
        let asynchronous = auth.clone();
        let waiting = tokio::spawn(async move { asynchronous.acquire_async().await.err() });
        wait_for_waiters(&auth, 2).await;
        leader.abort();
        assert!(leader.await.unwrap_err().is_cancelled());
        assert_eq!(blocked.await.unwrap(), Some(RenewalError::Aborted));
        assert_eq!(waiting.await.unwrap(), Some(RenewalError::Aborted));
        let next = auth.acquire_async().await.unwrap();
        next.complete(
            StatusCode::OK,
            &headers("bdry_access_recovered", now() + 250),
        );
        assert_eq!(auth.bearer().expose(), "bdry_access_recovered");
    }

    #[test]
    fn api_keys_and_valid_access_tokens_do_not_wait_for_renewal() {
        let key = Authentication::shared(
            "https://key.example.test",
            Secret::new("bdry_secret_key".into()),
        );
        let first = key.acquire_blocking().unwrap();
        let second = key.acquire_blocking().unwrap();
        assert!(!first.renewing && !second.renewing);
        assert_eq!(second.token().expose(), "bdry_secret_key");
        let session = Authentication::shared(
            "https://cached.example.test",
            Secret::new("bdry_session_cached".into()),
        );
        session.observe(StatusCode::OK, &headers("bdry_access_cached", now() + 250));
        let first = session.acquire_blocking().unwrap();
        let second = session.acquire_blocking().unwrap();
        assert!(!first.renewing && !second.renewing);
        assert_eq!(second.token().expose(), "bdry_access_cached");
    }

    #[tokio::test]
    async fn concurrent_rejections_join_one_renewal_and_late_rejections_preserve_it() {
        let auth = Authentication::shared(
            "https://rejected-renewal.example.test",
            Secret::new("bdry_session_rejected".into()),
        );
        auth.observe(StatusCode::OK, &headers("bdry_access_old", now() + 200));
        let first = auth.acquire_blocking().unwrap();
        let late = auth.acquire_blocking().unwrap();
        assert!(auth.rejected(first.token()));
        first.complete(StatusCode::UNAUTHORIZED, &HeaderMap::new());
        let leader = auth.acquire_async().await.unwrap();
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let concurrent = auth.clone();
            tasks.push(tokio::spawn(async move {
                assert!(concurrent.rejected(&Secret::new("bdry_access_old".into())));
                concurrent
                    .acquire_async()
                    .await
                    .unwrap()
                    .token()
                    .expose()
                    .to_owned()
            }));
        }
        wait_for_waiters(&auth, tasks.len()).await;
        leader.complete(StatusCode::OK, &headers("bdry_access_new", now() + 250));
        for task in tasks {
            assert_eq!(task.await.unwrap(), "bdry_access_new");
        }
        assert!(auth.rejected(late.token()));
        late.complete(StatusCode::UNAUTHORIZED, &HeaderMap::new());
        assert_eq!(
            auth.acquire_async().await.unwrap().token().expose(),
            "bdry_access_new"
        );
    }

    #[cfg(feature = "auth")]
    #[tokio::test]
    async fn query_and_telemetry_wait_for_the_same_renewal_before_sending_requests() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{header, method, path},
        };
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/queries/shared/cancel"))
            .and(header("authorization", "Bearer bdry_access_http-shared"))
            .respond_with(ResponseTemplate::new(204))
            .expect(4)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/v1/recordings/r/uploads:prepare"))
            .and(header("authorization", "Bearer bdry_access_http-shared"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .expect(4)
            .mount(&server)
            .await;
        let auth = Authentication::shared(
            &server.uri(),
            Secret::new("bdry_session_http-shared".into()),
        );
        let leader = auth.acquire_async().await.unwrap();
        let mut tasks = Vec::new();
        for _ in 0..4 {
            let endpoint = crate::Endpoint::parse(&server.uri()).unwrap();
            let request = RequestAuthorization {
                authentication: auth.clone(),
                target: None,
            };
            tasks.push(tokio::task::spawn_blocking(move || {
                crate::Client::new(endpoint)
                    .unwrap()
                    .cancel_query(&request, "shared")
                    .unwrap();
            }));
            let endpoint = format!("{}/v1/recordings/r/uploads:prepare", server.uri())
                .parse()
                .unwrap();
            let request = RequestAuthorization {
                authentication: auth.clone(),
                target: None,
            };
            tasks.push(tokio::spawn(async move {
                let result = crate::telemetry::prepare(
                    &reqwest::Client::new(),
                    &endpoint,
                    None,
                    Some(&request),
                    "r:1",
                    bytes::Bytes::from_static(b"{}"),
                )
                .await
                .unwrap();
                assert!(result.status().is_success());
            }));
        }
        wait_for_waiters(&auth, tasks.len()).await;
        assert!(server.received_requests().await.unwrap().is_empty());
        leader.complete(
            StatusCode::OK,
            &headers("bdry_access_http-shared", now() + 250),
        );
        for task in tasks {
            task.await.unwrap();
        }
    }
    #[test]
    fn cache_is_shared_across_operations_but_isolated_by_endpoint_and_account() {
        let original = Secret::new("bdry_session_cache-a".into());
        let a = Authentication::shared("https://a.example.test", original.clone());
        let same = Authentication::shared("https://a.example.test/", original.clone());
        let other_endpoint = Authentication::shared("https://b.example.test", original);
        let other_account = Authentication::shared(
            "https://a.example.test",
            Secret::new("bdry_session_cache-b".into()),
        );
        a.observe(StatusCode::OK, &headers("bdry_access_cached", now() + 300));
        assert_eq!(same.bearer().expose(), "bdry_access_cached");
        assert_eq!(other_endpoint.bearer().expose(), "bdry_session_cache-a");
        assert_eq!(other_account.bearer().expose(), "bdry_session_cache-b");
        assert!(!format!("{a:?}").contains("bdry_session_cache-a"));
        assert!(!a.accepts_url(&"https://a.example.test.evil.test/v1/query".parse().unwrap()));
    }
    #[test]
    fn renewal_ignores_failed_malformed_expired_and_stale_headers() {
        let a = Authentication::shared(
            "https://renew.example.test",
            Secret::new("bdry_session_original".into()),
        );
        a.observe(StatusCode::OK, &headers("bdry_access_new", now() + 250));
        for (status, token, expiry) in [
            (StatusCode::FORBIDDEN, "bdry_access_bad", now() + 300),
            (StatusCode::OK, "bdry_secret_wrong", now() + 300),
            (StatusCode::OK, "bdry_access_expired", 0),
            (StatusCode::OK, "bdry_access_overlong", now() + 301),
            (StatusCode::OK, "bdry_access_stale", now() + 100),
        ] {
            a.observe(status, &headers(token, expiry));
            assert_eq!(a.bearer().expose(), "bdry_access_new");
        }
        assert!(a.rejected(&Secret::new("bdry_access_stale".into())));
        assert_eq!(a.bearer().expose(), "bdry_access_new");
        assert!(a.rejected(&Secret::new("bdry_access_new".into())));
        assert_eq!(a.bearer().expose(), "bdry_session_original");
        assert!(!a.rejected(&Secret::new("bdry_session_original".into())));
        a.0.cache.lock().unwrap().access.replace(CachedAccess {
            token: Secret::new("bdry_access_expired".into()),
            expires_at: 0,
        });
        assert_eq!(a.bearer().expose(), "bdry_session_original");
    }
}
