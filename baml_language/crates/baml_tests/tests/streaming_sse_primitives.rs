//! Layer 1: Raw SSE primitive tests.
//!
//! Tests the HTTP SSE layer (`fetch_sse`, `SseStream.next()`, `SseStream.close()`)
//! directly without the LLM accumulator or SAP parsing.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{body_string, header, method, path},
};

#[tokio::test]
async fn sse_fetch_and_next_returns_events() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: message\ndata: hello\n\nevent: message\ndata: world\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output.result.expect("should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    assert!(
        json.contains("\"data\":\"hello\""),
        "should contain hello event: {json}"
    );
    assert!(
        json.contains("\"data\":\"world\""),
        "should contain world event: {json}"
    );
}

#[tokio::test]
async fn sse_next_returns_null_when_done() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: message\ndata: only-one\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let first = sse.next();
            let second = sse.next();
            sse.close();
            if (first == null) {{ return "first-null"; }}
            if (second != null) {{ return "second-non-null"; }}
            first
        }}
    "#
    ));

    let result = output.result.expect("should succeed");
    let BexExternalValue::String(events) = result else {
        panic!("expected string, got {result:?}");
    };
    assert!(
        events.contains("\"data\":\"only-one\""),
        "first batch should contain the only event: {events}"
    );
}

#[tokio::test]
async fn sse_close_does_not_hang() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("event: message\ndata: hello\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> bool {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            events != null
        }}
    "#
    ));

    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn sse_fetch_error_on_bad_status() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(ResponseTemplate::new(500).set_body_string("Internal Server Error"))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            "should not reach"
        }}
    "#
    ));

    assert!(output.result.is_err(), "Expected error for 500 response");
    let err = output.result.unwrap_err().to_string();
    assert!(
        err.contains("500"),
        "Error should mention status code: {err}"
    );
}

// ============================================================================
// HTTP behavior
// ============================================================================

#[tokio::test]
async fn sse_post_with_body() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/sse"))
        .and(body_string("my-request-body"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: ok\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "POST",
                url: "{uri}/sse",
                headers: {{}},
                body: "my-request-body",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output.result.expect("POST with body should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    assert!(
        json.contains("\"data\":\"ok\""),
        "should contain event data: {json}"
    );
}

#[tokio::test]
async fn sse_custom_headers() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .and(header("authorization", "Bearer test-token-123"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: authed\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let headers: map<string, string> = {{}};
            headers["authorization"] = "Bearer test-token-123";
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: headers,
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output
        .result
        .expect("request with custom header should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    assert!(
        json.contains("\"data\":\"authed\""),
        "should contain authed event: {json}"
    );
}

#[tokio::test]
async fn sse_empty_stream_returns_null() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(""),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let first = sse.next();
            sse.close();
            if (first == null) {{ return "got-null"; }}
            "unexpected-data"
        }}
    "#
    ));

    let result = output.result.expect("empty stream should succeed");
    let BexExternalValue::String(s) = result else {
        panic!("expected string, got {result:?}");
    };
    assert_eq!(s, "got-null", "first next() on empty stream should be null");
}

#[tokio::test]
async fn sse_next_after_close_returns_null() {
    // close() drops the resource handle, so subsequent next() should return null.
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: hello\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let first = sse.next();
            sse.close();
            let after_close = sse.next();
            if (after_close == null) {{ return "null-after-close"; }}
            "unexpected-data"
        }}
    "#
    ));

    let result = output.result.expect("next after close should succeed");
    let BexExternalValue::String(s) = result else {
        panic!("expected string, got {result:?}");
    };
    assert_eq!(s, "null-after-close");
}

#[tokio::test]
async fn sse_event_fields_in_json() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("id: 42\nevent: custom\ndata: payload\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output.result.expect("should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    // Verify JSON structure: array of objects with event, data, id fields.
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON array");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed[0]["event"], "custom");
    assert_eq!(parsed[0]["data"], "payload");
    assert_eq!(parsed[0]["id"], "42");
}

#[tokio::test]
async fn sse_event_id_null_when_not_set() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string("data: no-id\n\n"),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output.result.expect("should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json).expect("valid JSON array");
    assert_eq!(parsed.len(), 1);
    assert!(parsed[0]["id"].is_null(), "id should be null when not set");
}

#[tokio::test]
async fn sse_http_4xx_error() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(ResponseTemplate::new(403).set_body_string("Forbidden"))
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            "should not reach"
        }}
    "#
    ));

    assert!(output.result.is_err(), "Expected error for 403 response");
    let err = output.result.unwrap_err().to_string();
    assert!(
        err.contains("403"),
        "Error should mention status code: {err}"
    );
}

#[tokio::test]
async fn sse_large_event_payload() {
    let large_data = "x".repeat(50_000);
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/sse"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "text/event-stream")
                .set_body_string(format!("data: {large_data}\n\n")),
        )
        .mount(&server)
        .await;
    let uri = server.uri();

    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let request = baml.http.Request {{
                method: "GET",
                url: "{uri}/sse",
                headers: {{}},
                body: "",
            }};
            let sse = baml.http.fetch_sse(request);
            let events = sse.next();
            sse.close();
            if (events == null) {{ return "null"; }}
            events
        }}
    "#
    ));

    let result = output.result.expect("large payload should succeed");
    let BexExternalValue::String(json) = result else {
        panic!("expected string, got {result:?}");
    };
    assert!(
        json.contains(&large_data),
        "should contain the full 50KB payload (got {} bytes)",
        json.len()
    );
}
