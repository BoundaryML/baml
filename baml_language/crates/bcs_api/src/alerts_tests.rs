use serde_json::json;
use sha2::{Digest, Sha256};
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_bytes, body_partial_json, header, method, path, query_param},
};

use super::*;

fn alert(version: u64) -> serde_json::Value {
    json!({"id": "alr_1", "name": "FindFacts errors", "version": version, "versionId": "alv_2",
        "schedule": {"kind": "every", "seconds": 900}, "nextRunAt": 1_800_000_900,
        "updatedBy": "user_ada", "updatedAt": 1_800_000_000_000_u64})
}

fn definition() -> Definition {
    Definition {
        name: "FindFacts errors".to_owned(),
        sql: "SELECT span_id FROM spans WHERE error_value IS NOT NULL".to_owned(),
        destinations: vec!["dst_1".to_owned()],
        message_rows: 5,
    }
}

#[tokio::test]
async fn a_save_sends_the_sql_only_to_the_data_plane_with_the_digest_the_control_api_got() {
    let control = MockServer::start().await;
    let plane = MockServer::start().await;
    let bytes = serde_json::to_vec(&definition()).unwrap();
    let digest = hex::encode(Sha256::digest(&bytes));
    Mock::given(method("POST"))
        .and(path("/v1/environments/env_1/alerts"))
        .and(header("authorization", "Bearer bdry_session_access"))
        .and(body_partial_json(json!({"name": "FindFacts errors",
            "schedule": {"kind": "every", "seconds": 900}, "runNow": true,
            "expectedVersion": 0, "sha256": digest, "size": bytes.len()})))
        .respond_with(
            ResponseTemplate::new(201).set_body_json(json!({"alertId": "alr_1",
            "versionId": "alv_2", "dataPlaneUrl": plane.uri(), "uploadToken": "upload-token",
            "expiresAt": 1})),
        )
        .expect(1)
        .mount(&control)
        .await;
    Mock::given(method("PUT"))
        .and(path("/v1/alert-versions/alv_2/definition"))
        .and(header("authorization", "Bearer upload-token"))
        .and(body_bytes(bytes))
        .respond_with(ResponseTemplate::new(200).set_body_json(alert(1)))
        .expect(1)
        .mount(&plane)
        .await;
    let url = control.uri();
    tokio::task::spawn_blocking(move || {
        let client = Client::new(
            Endpoint::parse(&url).unwrap(),
            Secret::new("bdry_session_access".to_owned()),
        )
        .unwrap();
        let saved = client
            .create(
                "env_1",
                &definition(),
                &Schedule::Every { seconds: 900 },
                true,
            )
            .unwrap();
        assert_eq!(saved.version, 1);
        assert_eq!(saved.next_run_at, Some(1_800_000_900));
    })
    .await
    .unwrap();
    // The control API never saw the SQL.
    let requests = control.received_requests().await.unwrap();
    assert!(
        requests
            .iter()
            .all(|request| !String::from_utf8_lossy(&request.body).contains("SELECT"))
    );
}

#[tokio::test]
async fn reads_name_an_environment_page_runs_and_report_refusals_without_the_token() {
    let control = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/environment-lookup"))
        .and(query_param("project", "acme/demo"))
        .and(query_param("environment", "production"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"organizationId": "org_1",
            "projectId": "prj_1", "environmentId": "env_1", "environmentName": "production"})),
        )
        .mount(&control)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/environments/env_1/alert-runs"))
        .and(query_param("limit", "10"))
        .and(query_param("before", "41"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"runs": [{"id": 40,
            "alertId": "alr_1", "alertName": "FindFacts errors", "runAt": 1_800_000_000,
            "outcome": "undeliverable", "deliveries": [{"destinationId": "dst_1",
            "destinationName": "#ai", "deleted": true, "delivered": false,
            "reason": "the destination was deleted"}]}]})),
        )
        .mount(&control)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/environments/env_2/alerts"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({"code": "NOT_FOUND",
            "message": "not found"})),
        )
        .mount(&control)
        .await;
    let url = control.uri();
    tokio::task::spawn_blocking(move || {
        let client = Client::new(
            Endpoint::parse(&url).unwrap(),
            Secret::new("bdry_session_access".to_owned()),
        )
        .unwrap();
        let found = client.environment("acme/demo", Some("production")).unwrap();
        assert_eq!(found.environment_id, "env_1");
        let page = client.runs("env_1", 10, Some(41)).unwrap();
        assert_eq!(
            page.runs[0].deliveries[0].destination_name.as_deref(),
            Some("#ai")
        );
        assert!(page.next_before.is_none());
        let refused = client.list("env_2").unwrap_err();
        assert_eq!(refused.status(), Some(reqwest::StatusCode::NOT_FOUND));
        assert!(!refused.to_string().contains("bdry_session_access"));
    })
    .await
    .unwrap();
}

#[test]
fn the_control_api_is_boundary_s_unless_configured() {
    assert_eq!(
        control_endpoint(Some("http://localhost:22701"))
            .unwrap()
            .as_str(),
        "http://localhost:22701"
    );
    assert!(control_endpoint(Some("http://example.com")).is_err());
}
