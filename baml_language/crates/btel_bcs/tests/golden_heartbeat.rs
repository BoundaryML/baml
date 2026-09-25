use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use btel_bcs::{
    liveness::Heartbeat,
    wire::{HeartbeatPolicy, Liveness, ProducerState},
};
use serde_json::Value;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path},
};

const SENDER: &str = include_str!("fixtures/cloud-v1/heartbeat/sender.json");
const MESSAGES: &str = include_str!("fixtures/cloud-v1/heartbeat/reordered-messages.json");
const OBSERVATIONS: &str = include_str!("fixtures/cloud-v1/heartbeat/reordered-observations.json");

#[tokio::test]
async fn actual_sender_matches_fixed_heartbeat_wire_examples() {
    #[cfg(feature = "ring-crypto")]
    if rustls::crypto::CryptoProvider::get_default().is_none() {
        let _ = rustls::crypto::ring::default_provider().install_default();
    }
    let fixture: Value = serde_json::from_str(SENDER).unwrap();
    assert_eq!(fixture["fixture_version"], 1);
    let url: reqwest::Url = fixture["url"].as_str().unwrap().parse().unwrap();
    let policy: HeartbeatPolicy = serde_json::from_value(fixture["policy"].clone()).unwrap();
    let client = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .no_proxy()
        .build()
        .unwrap();
    let mut process_session = None;
    let mut previous_sequence = 0;
    for request in fixture["requests"].as_array().unwrap() {
        let expected: Liveness = serde_json::from_value(request.clone()).unwrap();
        let server = MockServer::start().await;
        let received = Arc::new(tokio::sync::Notify::new());
        let notify = Arc::clone(&received);
        let count = AtomicUsize::new(0);
        let status = u16::try_from(fixture["response"]["status"].as_u64().unwrap()).unwrap();
        let body = fixture["response"]["body"].as_str().unwrap().to_owned();
        Mock::given(method(fixture["method"].as_str().unwrap()))
            .and(path(url.path()))
            .respond_with(move |_: &Request| {
                // A second request proves the first response was fully consumed.
                if count.fetch_add(1, Ordering::Relaxed) == 1 {
                    notify.notify_one();
                }
                ResponseTemplate::new(status).set_body_string(body.clone())
            })
            .mount(&server)
            .await;
        let heartbeat = Heartbeat::new();
        heartbeat.configure(Some(policy)).unwrap();
        heartbeat.set_state(expected.state);
        let worker = heartbeat.clone();
        let endpoint = format!("{}{}", server.uri(), url.path()).parse().unwrap();
        let token = fixture["headers"]["authorization"]
            .as_str()
            .unwrap()
            .strip_prefix("Bearer ")
            .unwrap()
            .to_owned();
        let client = client.clone();
        let task = tokio::spawn(async move {
            worker
                .run(client, endpoint, Some(token), Duration::from_secs(2))
                .await;
        });
        let observed = tokio::time::timeout(Duration::from_secs(5), received.notified()).await;
        heartbeat.stop();
        task.await.unwrap();
        observed.expect("fixture heartbeat must reach the mock endpoint");
        assert_eq!(heartbeat.last_error(), None);
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        for actual in requests {
            assert_eq!(actual.method, fixture["method"].as_str().unwrap());
            assert_eq!(actual.url.path(), url.path());
            assert_eq!(actual.url.query(), url.query());
            for (header, value) in fixture["headers"].as_object().unwrap() {
                assert_eq!(actual.headers[header.as_str()], value.as_str().unwrap());
                assert_eq!(actual.headers.get_all(header.as_str()).iter().count(), 1);
            }
            assert!(!actual.headers.contains_key("idempotency-key"));
            assert_eq!(actual.headers.get_all("content-length").iter().count(), 1);
            assert_eq!(
                actual.headers["content-length"],
                actual.body.len().to_string()
            );
            let mut message: Liveness = serde_json::from_slice(&actual.body).unwrap();
            assert_eq!(message.producer_session_id.get_version_num(), 4);
            assert_eq!(
                process_session.get_or_insert(message.producer_session_id),
                &message.producer_session_id
            );
            assert!(message.liveness_sequence > previous_sequence);
            previous_sequence = message.liveness_sequence;
            // These are the only nondeterministic wire fields.
            message.producer_session_id = expected.producer_session_id;
            message.liveness_sequence = expected.liveness_sequence;
            assert_eq!(serde_json::to_value(message).unwrap(), *request);
        }
    }
}

#[test]
fn duplicate_and_reordered_messages_preserve_the_latest_observation() {
    // This reference fold is a proposed server contract, not BCS server coverage.
    let messages: Vec<Liveness> = serde_json::from_str(MESSAGES).unwrap();
    let observations: Vec<Value> = serde_json::from_str(OBSERVATIONS).unwrap();
    assert_eq!(messages.len(), observations.len());
    let mut latest: Option<&Liveness> = None;
    for (message, expected) in messages.iter().zip(&observations) {
        assert_eq!(message.producer_session_id, messages[0].producer_session_id);
        assert_eq!(message.producer_session_id.get_version_num(), 4);
        if latest.is_none_or(|previous| message.liveness_sequence > previous.liveness_sequence) {
            latest = Some(message);
        }
        let latest = latest.unwrap();
        assert_eq!(expected["highest_sequence"], latest.liveness_sequence);
        assert_eq!(
            serde_json::from_value::<ProducerState>(expected["state"].clone()).unwrap(),
            latest.state
        );
    }
    assert_eq!(
        messages[1], messages[2],
        "fixture includes an exact duplicate"
    );
    assert!(messages[3].liveness_sequence < messages[2].liveness_sequence);
    assert_eq!(messages[3].state, ProducerState::Running);
    assert_eq!(latest.unwrap().state, ProducerState::Draining);
}
