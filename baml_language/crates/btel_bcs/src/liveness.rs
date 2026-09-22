//! Advisory process liveness, independent of delivery admission and retries.
//!
//! Run one worker per recording. A successful control-plane POST must call
//! `mark_control_success`; S3 PUTs and failed POSTs must not. `stop` cancels even
//! an in-flight heartbeat, so shutdown never waits for a heartbeat retry.

use std::{
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};

use tokio::{
    sync::{Notify, watch},
    time::Instant,
};

use crate::wire::{HeartbeatPolicy, Liveness, PrepareUploadsRequest, ProducerState};

static SESSION: OnceLock<String> = OnceLock::new();
static SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeartbeatError {
    SequenceExhausted,
    InvalidPolicy,
    InvalidEndpoint,
    Http,
    Status(u16),
    ResponseTooLarge,
    Timeout,
    Worker,
}

struct Schedule {
    policy: Option<HeartbeatPolicy>,
    last_success: Instant,
    next_attempt: Instant,
    producer_state: ProducerState,
    last_error: Option<HeartbeatError>,
    failures: u64,
}

struct Shared {
    state: Mutex<Schedule>,
    changed: Notify,
    stopped: watch::Sender<bool>,
}

#[derive(Clone)]
pub struct Heartbeat(Arc<Shared>);

impl Default for Heartbeat {
    fn default() -> Self {
        Self::new()
    }
}

impl Heartbeat {
    pub fn new() -> Self {
        let now = Instant::now();
        Self(Arc::new(Shared {
            state: Mutex::new(Schedule {
                policy: None,
                last_success: now,
                next_attempt: now,
                producer_state: ProducerState::Running,
                last_error: None,
                failures: 0,
            }),
            changed: Notify::new(),
            stopped: watch::channel(false).0,
        }))
    }

    pub fn liveness(&self) -> Result<Liveness, HeartbeatError> {
        let sequence = SEQUENCE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| {
                self.observe_error(HeartbeatError::SequenceExhausted);
                HeartbeatError::SequenceExhausted
            })?
            + 1;
        Ok(Liveness {
            producer_session_id: SESSION
                .get_or_init(|| uuid::Uuid::new_v4().to_string())
                .clone(),
            liveness_sequence: sequence,
            state: self.0.state.lock().unwrap().producer_state,
        })
    }

    pub fn decorate_prepare(
        &self,
        request: &PrepareUploadsRequest,
    ) -> Result<PrepareUploadsRequest, HeartbeatError> {
        let liveness = self.liveness()?;
        let mut request = request.clone();
        request.producer_session_id = Some(liveness.producer_session_id);
        request.liveness_sequence = Some(liveness.liveness_sequence);
        request.state = Some(liveness.state);
        Ok(request)
    }

    pub fn configure(&self, policy: Option<HeartbeatPolicy>) -> Result<(), HeartbeatError> {
        if policy.is_some_and(|p| {
            p.interval_ms == 0
                || p.staleness_threshold_ms < p.interval_ms
                || Instant::now()
                    .checked_add(Duration::from_millis(p.interval_ms))
                    .is_none()
        }) {
            self.observe_error(HeartbeatError::InvalidPolicy);
            return Err(HeartbeatError::InvalidPolicy);
        }
        self.0.state.lock().unwrap().policy = policy;
        self.0.changed.notify_one();
        Ok(())
    }

    pub fn mark_control_success(&self) {
        self.0.state.lock().unwrap().last_success = Instant::now();
        self.0.changed.notify_one();
    }

    /// Changes the state on subsequent control-plane POSTs without forcing one.
    /// A quick drain may finish before another heartbeat is due; shutdown must
    /// not wait for the interval merely to report `DRAINING`.
    pub fn set_state(&self, state: ProducerState) {
        let mut current = self.0.state.lock().unwrap();
        if current.producer_state == ProducerState::Disabled
            || (current.producer_state == ProducerState::Draining
                && state == ProducerState::Running)
        {
            return;
        }
        current.producer_state = state;
    }

    pub fn last_error(&self) -> Option<HeartbeatError> {
        self.0.state.lock().unwrap().last_error
    }

    pub fn failure_count(&self) -> u64 {
        self.0.state.lock().unwrap().failures
    }

    pub(crate) fn observe_error(&self, error: HeartbeatError) {
        let mut state = self.0.state.lock().unwrap();
        state.last_error = Some(error);
        state.failures = state.failures.saturating_add(1);
    }

    pub fn stop(&self) {
        self.0.stopped.send_replace(true);
    }

    fn deadline(&self) -> Option<Instant> {
        let state = self.0.state.lock().unwrap();
        state.policy.map(|p| {
            (state.last_success + Duration::from_millis(p.interval_ms)).max(state.next_attempt)
        })
    }

    /// The supplied client must disable redirects and have no default auth
    /// headers. The endpoint must be the authorized BCS heartbeat URL; only
    /// this POST receives the BCS bearer token. No URL or token enters errors.
    pub async fn run(
        &self,
        client: reqwest::Client,
        endpoint: reqwest::Url,
        bearer_token: Option<String>,
        request_timeout: Duration,
    ) {
        if !matches!(endpoint.scheme(), "http" | "https")
            || !endpoint.username().is_empty()
            || endpoint.password().is_some()
            || endpoint.query().is_some()
            || endpoint.fragment().is_some()
            || request_timeout.is_zero()
        {
            self.observe_error(HeartbeatError::InvalidEndpoint);
            return;
        }
        let mut stopped = self.0.stopped.subscribe();
        loop {
            if *stopped.borrow() {
                return;
            }
            let deadline = self.deadline();
            tokio::select! {
                biased;
                _ = stopped.changed() => return,
                () = self.0.changed.notified() => continue,
                () = async {
                    match deadline {
                        Some(deadline) => tokio::time::sleep_until(deadline).await,
                        None => std::future::pending().await,
                    }
                } => {}
            }
            let Ok(liveness) = self.liveness() else {
                return;
            };
            let result = tokio::select! {
                biased;
                _ = stopped.changed() => return,
                result = tokio::time::timeout(
                    request_timeout,
                    post(&client, &endpoint, bearer_token.as_deref(), liveness),
                ) => result.unwrap_or(Err(HeartbeatError::Timeout)),
            };
            if let Err(error) = result {
                self.observe_error(error);
            } else {
                self.mark_control_success();
            }
            let mut state = self.0.state.lock().unwrap();
            // A failed advisory POST does not reset liveness, but must not spin.
            state.next_attempt =
                Instant::now() + Duration::from_millis(state.policy.map_or(1, |p| p.interval_ms));
        }
    }
}

async fn post(
    client: &reqwest::Client,
    endpoint: &reqwest::Url,
    bearer_token: Option<&str>,
    body: Liveness,
) -> Result<(), HeartbeatError> {
    const MAX_RESPONSE_BYTES: usize = 4096;
    let mut request = client
        .post(endpoint.clone())
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .body(serde_json::to_vec(&body).map_err(|_| HeartbeatError::Http)?);
    if let Some(token) = bearer_token {
        request = request.bearer_auth(token);
    }
    let mut response = request.send().await.map_err(|_| HeartbeatError::Http)?;
    if !response.status().is_success() {
        return Err(HeartbeatError::Status(response.status().as_u16()));
    }
    if response
        .content_length()
        .is_some_and(|n| n > MAX_RESPONSE_BYTES as u64)
    {
        return Err(HeartbeatError::ResponseTooLarge);
    }
    let mut received = 0;
    while let Some(chunk) = response.chunk().await.map_err(|_| HeartbeatError::Http)? {
        if chunk.len() > MAX_RESPONSE_BYTES - received {
            return Err(HeartbeatError::ResponseTooLarge);
        }
        received += chunk.len();
    }
    // The minimal contract has no response payload or upload acknowledgement.
    Ok(())
}

#[cfg(test)]
mod tests {
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};

    use super::*;

    fn policy() -> HeartbeatPolicy {
        HeartbeatPolicy {
            interval_ms: 1_000,
            staleness_threshold_ms: 3_000,
        }
    }

    fn request() -> PrepareUploadsRequest {
        serde_json::from_value(serde_json::json!({
            "recording": {
                "recording_id": "01",
                "recording_file_sequence": 1,
                "encoded_length": 2,
                "sha256": "00"
            },
            "candidates": [],
            "proposed_uploads": []
        }))
        .unwrap()
    }

    #[test]
    fn process_identity_and_sequence_survive_engine_recreation() {
        let first = Heartbeat::new().liveness().unwrap();
        let second = Heartbeat::new().liveness().unwrap();
        assert_eq!(first.producer_session_id, second.producer_session_id);
        assert!(second.liveness_sequence > first.liveness_sequence);
        uuid::Uuid::parse_str(&first.producer_session_id).unwrap();
    }

    #[test]
    fn prepare_attempts_are_fresh_without_mutating_plan_identity() {
        let heartbeat = Heartbeat::new();
        let original = request();
        let first = heartbeat.decorate_prepare(&original).unwrap();
        heartbeat.set_state(ProducerState::Draining);
        let second = heartbeat.decorate_prepare(&original).unwrap();
        assert!(second.liveness_sequence > first.liveness_sequence);
        assert_eq!(second.state, Some(ProducerState::Draining));
        assert_eq!(first.recording, original.recording);
        assert_eq!(first.candidates, original.candidates);
        assert_eq!(first.proposed_uploads, original.proposed_uploads);
        assert_eq!(original.liveness_sequence, None);
        let json = serde_json::to_value(&second).unwrap();
        assert_eq!(
            serde_json::from_value::<PrepareUploadsRequest>(json).unwrap(),
            second
        );
    }

    #[tokio::test(start_paused = true)]
    async fn only_successful_control_posts_postpone_deadline() {
        let heartbeat = Heartbeat::new();
        assert_eq!(heartbeat.deadline(), None);
        heartbeat.configure(Some(policy())).unwrap();
        let first_deadline = heartbeat.deadline().unwrap();
        tokio::time::advance(Duration::from_millis(500)).await;
        // Failed prepare and direct PUT never call mark_control_success.
        heartbeat.decorate_prepare(&request()).unwrap();
        heartbeat.observe_error(HeartbeatError::Http);
        assert_eq!(heartbeat.deadline(), Some(first_deadline));
        heartbeat.mark_control_success();
        assert_eq!(
            heartbeat.deadline(),
            Some(first_deadline + Duration::from_millis(500))
        );
        assert_eq!(heartbeat.liveness().unwrap().state, ProducerState::Running);
        heartbeat.configure(None).unwrap();
        assert_eq!(heartbeat.deadline(), None);
    }

    #[test]
    fn invalid_policy_is_advisory_and_does_not_replace_policy() {
        let heartbeat = Heartbeat::new();
        heartbeat.configure(Some(policy())).unwrap();
        let deadline = heartbeat.deadline();
        assert_eq!(
            heartbeat.configure(Some(HeartbeatPolicy {
                interval_ms: 0,
                staleness_threshold_ms: 0,
            })),
            Err(HeartbeatError::InvalidPolicy)
        );
        assert_eq!(heartbeat.deadline(), deadline);
        assert_eq!(heartbeat.failure_count(), 1);
        assert_eq!(heartbeat.liveness().unwrap().state, ProducerState::Running);
    }

    #[tokio::test]
    async fn response_is_bounded_and_errors_are_sanitized() {
        let server = MockServer::start().await;
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let endpoint = server.uri().parse().unwrap();
        for (response, expected) in [
            (
                ResponseTemplate::new(503).set_body_string("secret"),
                Err(HeartbeatError::Status(503)),
            ),
            (
                ResponseTemplate::new(200).set_body_bytes(vec![0; 4097]),
                Err(HeartbeatError::ResponseTooLarge),
            ),
            (ResponseTemplate::new(204), Ok(())),
        ] {
            server.reset().await;
            Mock::given(method("POST"))
                .respond_with(response)
                .mount(&server)
                .await;
            let result = post(
                &client,
                &endpoint,
                Some("private"),
                Heartbeat::new().liveness().unwrap(),
            )
            .await;
            assert_eq!(result, expected);
            assert!(!format!("{result:?}").contains("private"));
            assert!(!format!("{result:?}").contains("secret"));
        }
    }

    #[tokio::test(start_paused = true)]
    async fn failures_are_advisory_rate_limited_and_worker_is_cancellable() {
        let heartbeat = Heartbeat::new();
        heartbeat.configure(Some(policy())).unwrap();
        let start = Instant::now();
        let worker = heartbeat.run(
            reqwest::Client::new(),
            "http://127.0.0.1:1/heartbeat".parse().unwrap(),
            None,
            Duration::from_millis(100),
        );
        tokio::pin!(worker);
        assert!(futures::poll!(&mut worker).is_pending());
        tokio::time::advance(Duration::from_millis(1_000)).await;
        assert!(futures::poll!(&mut worker).is_pending());
        tokio::time::advance(Duration::from_millis(100)).await;
        assert!(futures::poll!(&mut worker).is_pending());
        assert_eq!(heartbeat.failure_count(), 1);
        assert_eq!(heartbeat.0.state.lock().unwrap().last_success, start);
        assert_eq!(heartbeat.liveness().unwrap().state, ProducerState::Running);
        for _ in 0..10 {
            assert!(futures::poll!(&mut worker).is_pending());
        }
        assert_eq!(heartbeat.failure_count(), 1);
        heartbeat.stop();
        assert!(futures::poll!(&mut worker).is_ready());
    }

    #[tokio::test(start_paused = true)]
    async fn stopped_worker_does_not_wait_for_interval() {
        let heartbeat = Heartbeat::new();
        heartbeat.configure(Some(policy())).unwrap();
        let start = Instant::now();
        heartbeat.stop();
        heartbeat
            .run(
                reqwest::Client::new(),
                "http://127.0.0.1:1/heartbeat".parse().unwrap(),
                None,
                Duration::from_secs(30),
            )
            .await;
        assert_eq!(Instant::now(), start);
        assert_eq!(heartbeat.failure_count(), 0);
    }
}
