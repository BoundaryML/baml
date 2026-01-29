//! Tests for HTTP operations (baml.http.fetch, `Response` methods).

mod common;

use bex_engine::BexExternalValue;
use common::{EngineProgram, assert_engine_executes};
use indexmap::indexmap;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

/// Test basic fetch and text extraction.
#[tokio::test]
async fn http_fetch_and_text() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/data"))
        .respond_with(ResponseTemplate::new(200).set_body_string("Hello from HTTP!"))
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> string {{
            let response = baml.http.fetch("{}/data");
            response.text()
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::String("Hello from HTTP!".to_string())),
    })
    .await
}

/// Test status code access (now a field, not a method).
#[tokio::test]
async fn http_response_status() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/status"))
        .respond_with(ResponseTemplate::new(201))
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> int {{
            let response = baml.http.fetch("{}/status");
            response.status_code
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Int(201)),
    })
    .await
}

/// Test `ok()` returns true for 2xx.
#[tokio::test]
async fn http_response_ok_true() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/ok"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> bool {{
            let response = baml.http.fetch("{}/ok");
            response.ok()
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Bool(true)),
    })
    .await
}

/// Test `ok()` returns false for 4xx.
#[tokio::test]
async fn http_response_ok_false() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/notfound"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> bool {{
            let response = baml.http.fetch("{}/notfound");
            response.ok()
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::Bool(false)),
    })
    .await
}

/// Test `url` field returns the request URL.
#[tokio::test]
async fn http_response_url() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/endpoint"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&mock_server)
        .await;

    let expected_url = format!("{}/endpoint", mock_server.uri());
    let source = format!(
        r#"
        function main() -> string {{
            let response = baml.http.fetch("{}/endpoint");
            response.url
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Ok(BexExternalValue::String(expected_url)),
    })
    .await
}

/// Test network error handling.
#[tokio::test]
async fn http_fetch_network_error() -> anyhow::Result<()> {
    // Use an address that will definitely fail to connect
    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: r#"
            function main() -> string {
                let response = baml.http.fetch("http://localhost:1");
                response.text()
            }
        "#,
        entry: "main",
        inputs: vec![],
        expected: Err("HTTP request failed"),
    })
    .await
}

/// Test that calling `text()` twice fails.
#[tokio::test]
async fn http_response_text_consumed() -> anyhow::Result<()> {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/once"))
        .respond_with(ResponseTemplate::new(200).set_body_string("body"))
        .mount(&mock_server)
        .await;

    let source = format!(
        r#"
        function main() -> string {{
            let response = baml.http.fetch("{}/once");
            let first = response.text();
            let second = response.text();
            second
        }}
    "#,
        mock_server.uri()
    );

    assert_engine_executes(EngineProgram {
        fs: indexmap! {},
        source: Box::leak(source.into_boxed_str()),
        entry: "main",
        inputs: vec![],
        expected: Err("already been consumed"),
    })
    .await
}
