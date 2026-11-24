//! Build registration (BEP-088) against a loopback mock of BCS.
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use btel_bcs::{
    build::register_blocking,
    delivery::{BcsDelivery, DeliveryConfig},
};
use btel_types::{
    BuildOrigin, BuildRecord, BuildRegistration, GitHash, GitInfo, PackageRecord, ProgramRecord,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[path = "support/finish.rs"]
mod finish;
use finish::finish;

/// The source of BCS's own verification test, and its `source_hash`
/// (`data-plane/.../projector/source.rs`).
const SOURCE: &str = r#"{"main.baml":"hello"}"#;
const SOURCE_HASH: &str = "72659bc07e31fabd0451aee9113c58816e63d541556e402bc8d6938ef354cd1d";

fn config(server: &MockServer) -> DeliveryConfig {
    DeliveryConfig {
        bearer_token: Some("register-only-secret".into()),
        request_timeout: Duration::from_millis(500),
        retry_delay: Duration::from_millis(5),
        max_attempts: 2,
        ..DeliveryConfig::new(server.uri().parse().unwrap())
    }
}

fn registration() -> BuildRegistration {
    BuildRegistration {
        build: BuildRecord {
            build_id: [0x11; 16],
            timestamp_unix_ns: 1_763_424_000_123_456_789,
            origin: BuildOrigin::Pack,
            git: Some(GitInfo {
                remote: Some("https://github.com/acme/app.git".into()),
                hash: GitHash::Sha1([0xab; 20]),
                dirty: true,
                branch: None,
                package_path: "pkg".into(),
            }),
            source_hash: hex::decode(SOURCE_HASH).unwrap().try_into().unwrap(),
            compilation_unit_hash: [0x22; 32],
        },
        package: PackageRecord {
            compilation_unit_hash: [0x22; 32],
            package_interface: [0x33; 32],
            dependencies: vec![[0x44; 32]],
            name: "app".into(),
            version: Some("1.2.3".into()),
            compiler_version: "0.300.0".into(),
        },
        program: Some(ProgramRecord {
            program_id: [0x55; 16],
            static_packages: vec![[0x66; 32], [0x22; 32]],
        }),
        source: Some([("main.baml".to_owned(), "hello".to_owned())].into()),
    }
}

async fn serve(server: &MockServer, answer: serde_json::Value) {
    Mock::given(method("POST"))
        .and(path("/v1/builds"))
        .respond_with(ResponseTemplate::new(200).set_body_json(answer))
        .mount(server)
        .await;
    Mock::given(method("PUT"))
        .and(path("/put/source"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
}

fn upload_required(server: &MockServer) -> serde_json::Value {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let expires = u64::try_from(now.as_millis()).unwrap() + 60_000;
    json!({
        "status": "upload_required",
        "presigned_put_url": format!("{}/put/source", server.uri()),
        "expires_at_unix_ms": expires,
    })
}

async fn register(
    server: &MockServer,
    registration: BuildRegistration,
) -> Result<(), btel_bcs::delivery::DeliveryError> {
    let config = config(server);
    tokio::task::spawn_blocking(move || register_blocking(&config, &registration))
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_registers_its_rows_then_uploads_the_source_bcs_verifies() {
    let server = MockServer::start().await;
    serve(&server, upload_required(&server)).await;
    register(&server, registration()).await.unwrap();

    let requests = server.received_requests().await.unwrap();
    let [post, put] = requests.as_slice() else {
        panic!("one registration and one upload: {requests:?}");
    };
    assert_eq!(post.headers["authorization"], "Bearer register-only-secret");
    let body: serde_json::Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(
        body,
        json!({
            "build": {
                "build_id": "11".repeat(16),
                "timestamp_ns": "1763424000123456789",
                "origin": "pack",
                "git": {
                    "remote": "https://github.com/acme/app.git",
                    "hash": "ab".repeat(20),
                    "dirty": true,
                    "branch": null,
                    "package_path": "pkg",
                },
                "source_hash": SOURCE_HASH,
                "compilation_unit_hash": "22".repeat(32),
            },
            "package": {
                "compilation_unit_hash": "22".repeat(32),
                "package_interface": "33".repeat(32),
                "dependencies": ["44".repeat(32)],
                "name": "app",
                "version": "1.2.3",
                "compiler_version": "0.300.0",
            },
            "program": {
                "program_id": "55".repeat(16),
                "static_packages": ["66".repeat(32), "22".repeat(32)],
            },
            "source": {
                "sha256": hex::encode(Sha256::digest(SOURCE)),
                "byte_length": SOURCE.len(),
            },
        })
    );
    // The presigned URL is the upload's whole authorization.
    assert!(!put.headers.contains_key("authorization"));
    assert_eq!(put.headers["content-type"], "application/json");
    assert_eq!(put.body, SOURCE.as_bytes());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_build_without_source_or_one_bcs_holds_uploads_nothing() {
    let server = MockServer::start().await;
    serve(&server, json!({"status": "not_requested"})).await;
    register(
        &server,
        BuildRegistration {
            source: None,
            program: None,
            ..registration()
        },
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    let [post] = requests.as_slice() else {
        panic!("a registration alone: {requests:?}");
    };
    let body: serde_json::Value = serde_json::from_slice(&post.body).unwrap();
    assert_eq!(body["source"], json!(null));
    assert_eq!(body["program"], json!(null));

    let server = MockServer::start().await;
    serve(&server, json!({"status": "already_present"})).await;
    register(&server, registration()).await.unwrap();
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_bcs_would_refuse_is_left_out_of_the_registration() {
    let server = MockServer::start().await;
    serve(&server, json!({"status": "not_requested"})).await;
    register(
        &server,
        BuildRegistration {
            source: Some([("dir\\main.baml".to_owned(), "hello".to_owned())].into()),
            ..registration()
        },
    )
    .await
    .unwrap();
    let requests = server.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
    assert_eq!(body["source"], json!(null));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_refusal_is_final_and_a_failing_dependency_is_retried() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/builds"))
        .respond_with(
            ResponseTemplate::new(409)
                .set_body_json(json!({"code": "CONTENT_CONFLICT", "retryable": false})),
        )
        .mount(&server)
        .await;
    assert!(register(&server, registration()).await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/builds"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;
    assert!(register(&server, registration()).await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_upload_url_off_https_is_never_sent_the_source() {
    let server = MockServer::start().await;
    serve(
        &server,
        json!({
            "status": "upload_required",
            "presigned_put_url": "ftp://127.0.0.1/put/source",
            "expires_at_unix_ms": u64::MAX,
        }),
    )
    .await;
    assert!(register(&server, registration()).await.is_err());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

/// A recording's delivery registers builds beside its uploads, and shutdown
/// waits for one already handed over.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_registers_a_build_in_the_background_and_drains_it() {
    let server = MockServer::start().await;
    serve(&server, upload_required(&server)).await;
    let delivery = BcsDelivery::new(config(&server), |_| {}).unwrap();
    delivery.handle().register_build(&registration());
    finish(delivery).await.unwrap();
    let requests = server.received_requests().await.unwrap();
    assert_eq!(
        requests
            .iter()
            .map(|request| (request.method.as_str(), request.url.path()))
            .collect::<Vec<_>>(),
        [("POST", "/v1/builds"), ("PUT", "/put/source")]
    );
}

/// Answers every registration after `delay`, and remembers when each arrived.
struct Slow {
    delay: Duration,
    arrivals: std::sync::Arc<std::sync::Mutex<Vec<std::time::Instant>>>,
}

impl wiremock::Respond for Slow {
    fn respond(&self, _: &wiremock::Request) -> ResponseTemplate {
        self.arrivals
            .lock()
            .unwrap()
            .push(std::time::Instant::now());
        ResponseTemplate::new(200)
            .set_body_json(json!({"status": "not_requested"}))
            .set_delay(self.delay)
    }
}

/// A host that compiles faster than BCS answers holds a bounded number of
/// registrations in flight: the rest wait in the bounded queue.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn delivery_bounds_the_registrations_in_flight() {
    let delay = Duration::from_millis(250);
    let limit = btel_bcs::delivery::MAX_ACTIVE_BUILDS;
    let server = MockServer::start().await;
    let arrivals = std::sync::Arc::default();
    Mock::given(method("POST"))
        .and(path("/v1/builds"))
        .respond_with(Slow {
            delay,
            arrivals: std::sync::Arc::clone(&arrivals),
        })
        .mount(&server)
        .await;
    let delivery = BcsDelivery::new(config(&server), |_| {}).unwrap();
    for id in 0..=limit {
        let mut build = registration();
        build.build.build_id = [u8::try_from(id).unwrap() + 1; 16];
        build.source = None;
        delivery.handle().register_build(&build);
    }
    finish(delivery).await.unwrap();
    let arrivals = arrivals.lock().unwrap();
    assert_eq!(arrivals.len(), limit + 1, "every build registers");
    // The one past the limit is sent only once an earlier one was answered.
    assert!(arrivals[limit].duration_since(arrivals[0]) >= delay);
}
