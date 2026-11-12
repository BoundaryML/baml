use serde_json::json;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{header, method, path},
};

use super::*;

const VERSION: &str = "dsv_0123456789abcdef0123456789abcdef";
fn publication(expected_version: Option<u64>) -> Publication {
    Publication {
        target: Target {
            project: Some("acme/app".into()),
            environment: Some("production".into()),
            ..Default::default()
        },
        key: "overview".into(),
        name: "Overview".into(),
        expected_version,
    }
}
fn pending(storage: &str) -> serde_json::Value {
    json!({"dashboardId":"dashboard", "versionId":VERSION, "dataPlaneUrl":storage, "uploadToken":"upload-secret", "expiresAt":4_102_444_800_u64})
}
#[tokio::test]
async fn publication_uses_separate_credentials_and_exact_bytes_for_create_and_update() {
    for expected in [None, Some(4)] {
        let gateway = MockServer::start().await;
        let storage = MockServer::start().await;
        let html = b"<!doctype html>\n<h1>Private dashboard</h1>".to_vec();
        let sha = hex::encode(Sha256::digest(&html));
        Mock::given(method("POST"))
            .and(path("/v1/dashboards/prepare"))
            .and(header("authorization", "Bearer bdry_session_native"))
            .respond_with(ResponseTemplate::new(200).set_body_json(pending(&storage.uri())))
            .expect(1)
            .mount(&gateway)
            .await;
        Mock::given(method("PUT"))
            .and(path(format!("/v1/dashboard-versions/{VERSION}/bundle")))
            .and(header("authorization", "Bearer upload-secret"))
            .and(header("content-type", "text/html"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "id":"dashboard", "projectId":"app", "key":"overview",
                "latest":{"version":expected.map_or(1, |v|v+1), "sha256":sha, "size":html.len()}
            })))
            .expect(1)
            .mount(&storage)
            .await;
        let url = gateway.uri();
        let sent = html.clone();
        let published = tokio::task::spawn_blocking(move || {
            Client::new(Endpoint::parse(&url).unwrap())
                .unwrap()
                .publish_dashboard(
                    &Secret::new("bdry_session_native".into()),
                    &publication(expected),
                    sent,
                )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(published.latest.version, expected.map_or(1, |v| v + 1));
        let control = gateway.received_requests().await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(&control[0].body).unwrap();
        assert_eq!(
            body["target"],
            json!({"project":"acme/app", "environment":"production"})
        );
        assert_eq!(body["sha256"], sha);
        assert_eq!(body["size"], html.len());
        assert_eq!(
            body.get("expectedVersion"),
            expected
                .as_ref()
                .map(|v| serde_json::Value::from(*v))
                .as_ref()
        );
        let uploaded = storage.received_requests().await.unwrap();
        assert_eq!(uploaded[0].body, html);
        assert!(!String::from_utf8_lossy(&uploaded[0].body).contains("bdry_session_native"));
    }
}

#[tokio::test]
async fn rejected_preparation_never_uploads_and_public_errors_redact_the_session() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/v1/dashboards/prepare"))
        .respond_with(ResponseTemplate::new(403).set_body_json(json!({
            "code":"PERMISSION_REQUIRED", "retryable":false, "message":"denied bdry_session_native"
        })))
        .expect(1)
        .mount(&server)
        .await;
    let url = server.uri();
    let error = tokio::task::spawn_blocking(move || {
        Client::new(Endpoint::parse(&url).unwrap())
            .unwrap()
            .publish_dashboard(
                &Secret::new("bdry_session_native".into()),
                &publication(None),
                b"html".to_vec(),
            )
    })
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.status(), Some(reqwest::StatusCode::FORBIDDEN));
    assert!(!format!("{error:?} {error}").contains("bdry_session_native"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn untrusted_upload_locations_and_expired_tokens_never_receive_a_credential() {
    for (field, value) in [
        ("versionId", json!("../secret")),
        ("dataPlaneUrl", json!("http://remote.example.test")),
        ("dataPlaneUrl", json!("https://secret@example.test")),
        ("expiresAt", json!(0)),
    ] {
        let server = MockServer::start().await;
        let mut response = pending(&server.uri());
        response[field] = value;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(response))
            .expect(1)
            .mount(&server)
            .await;
        let url = server.uri();
        assert!(
            tokio::task::spawn_blocking(move || Client::new(Endpoint::parse(&url).unwrap())
                .unwrap()
                .publish_dashboard(
                    &Secret::new("bdry_session_native".into()),
                    &publication(None),
                    b"html".to_vec()
                ))
            .await
            .unwrap()
            .is_err()
        );
        assert_eq!(server.received_requests().await.unwrap().len(), 1);
    }
}
