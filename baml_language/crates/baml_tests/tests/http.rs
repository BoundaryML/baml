//! Tests for HTTP operations.
//!
//! Tests here use insta snapshots (bytecode and/or traceback text), which
//! cannot be expressed in BAML.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

struct MockEndpoint {
    path: &'static str,
    status: u16,
    body: Option<&'static str>,
}

/// Start a mock server with the given GET endpoints. Returns (server, uri).
async fn mock(endpoints: &[MockEndpoint]) -> (MockServer, String) {
    let server = MockServer::start().await;
    for ep in endpoints {
        let mut response = ResponseTemplate::new(ep.status);
        if let Some(b) = ep.body {
            response = response.set_body_string(b);
        }
        Mock::given(method("GET"))
            .and(path(ep.path))
            .respond_with(response)
            .mount(&server)
            .await;
    }
    let uri = server.uri();
    (server, uri)
}

/// Replace the mock server URI in bytecode with a stable placeholder.
fn stabilize_bytecode(bytecode: &str, uri: &str) -> String {
    bytecode.replace(uri, "{URI}")
}

#[tokio::test]
async fn http_fetch_and_text() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/data",
        status: 200,
        body: Some("Hello from HTTP!"),
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let response = baml.http.fetch("{uri}/data");
                response.text()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> string {
        load_const "{URI}/data"
        load_const <omitted>
        call baml.http.fetch
        sys_op baml.http.Response.text
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "Hello from HTTP!".to_string().into()
        ))
    );
}

#[tokio::test]
async fn http_response_status() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/status",
        status: 201,
        body: None,
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let response = baml.http.fetch("{uri}/status");
                response.status_code
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> int {
        load_const "{URI}/status"
        load_const <omitted>
        call baml.http.fetch
        load_field .status_code
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(201)));
}

/// Regression test: field access on a foreign class instance must compile
/// as `load_field`, NOT `load_map_element`.
#[tokio::test]
async fn foreign_class_field_access_compiles_correctly() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/test",
        status: 200,
        body: Some("ok"),
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> int {{
                let response = baml.http.fetch("{uri}/test");
                response.status_code
            }}
        "#
    ));

    // The bytecode MUST use load_field, not load_map_element.
    // If this shows load_map_element, the foreign class field access bug is present.
    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> int {
        load_const "{URI}/test"
        load_const <omitted>
        call baml.http.fetch
        load_field .status_code
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(200)));
}

#[tokio::test]
async fn http_response_ok_true() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/ok",
        status: 200,
        body: None,
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> bool {{
                let response = baml.http.fetch("{uri}/ok");
                response.ok()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> bool {
        load_const "{URI}/ok"
        load_const <omitted>
        call baml.http.fetch
        call baml.http.Response.ok
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn http_response_ok_false() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/notfound",
        status: 404,
        body: None,
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> bool {{
                let response = baml.http.fetch("{uri}/notfound");
                response.ok()
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> bool {
        load_const "{URI}/notfound"
        load_const <omitted>
        call baml.http.fetch
        call baml.http.Response.ok
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn http_response_url() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/endpoint",
        status: 200,
        body: None,
    }])
    .await;
    let expected_url = format!("{uri}/endpoint");

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let response = baml.http.fetch("{uri}/endpoint");
                response.url
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> string {
        load_const "{URI}/endpoint"
        load_const <omitted>
        call baml.http.fetch
        load_field .url
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(expected_url.into()))
    );
}

#[tokio::test]
#[ignore = "compiler2: HTTP fetch catch semantics not implemented - unhandled error from external op"]
async fn http_fetch_network_error() {
    let output = baml_test!(
        r#"
            function main() -> int {
                let response = baml.http.fetch("http://localhost:1");
                response.status_code
            }
        "#
    );

    insta::assert_snapshot!(output.bytecode, @r#"
    function main() -> int {
        load_const "http://localhost:1"
        schedule_future baml.http.fetch
        await
        load_field .status_code
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
}

// Kept in Rust (not the baml_src corpus): the silent peer must be a controlled
// Rust listener that accepts and holds the connection open. A corpus version
// using `baml.http.Server.bind` as the silent peer is flaky — the BAML
// listener object can be GC-collected between building the request and the
// throwing `fetch`, resetting the connection into an `Io` error instead of the
// expected `Timeout`, intermittently under load.
#[tokio::test]
async fn http_fetch_timeout_fires() {
    // A raw TCP listener that accepts connections but never writes an HTTP
    // response, so the request hangs after connecting. A short total timeout
    // must surface as baml.errors.Timeout.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap().to_string();
    let server = tokio::spawn(async move {
        loop {
            if let Ok((conn, _)) = listener.accept().await {
                // Hold the connection open, silent, past the client's deadline.
                tokio::spawn(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                    drop(conn);
                });
            }
        }
    });

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let response = baml.http.fetch(
                    "http://{addr}/",
                    timeout = baml.time.Duration.from_milliseconds(100n),
                );
                response.text()
            }}
        "#
    ));
    server.abort();

    let err = output
        .result
        .expect_err("fetch with a 100ms timeout against a silent server should time out")
        .to_string();
    assert!(
        err.contains("baml.errors.Timeout"),
        "expected a baml.errors.Timeout throw, got: {err}"
    );
}

#[tokio::test]
async fn http_response_text_consumed() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/once",
        status: 200,
        body: Some("body"),
    }])
    .await;

    let output = baml_test!(&format!(
        r#"
            function main() -> string {{
                let response = baml.http.fetch("{uri}/once");
                let first = response.text();
                let second = response.text();
                second
            }}
        "#
    ));

    insta::assert_snapshot!(stabilize_bytecode(&output.bytecode, &uri), @r#"
    function main() -> string {
        load_const "{URI}/once"
        load_const <omitted>
        call baml.http.fetch
        store_var response
        load_var response
        sys_op baml.http.Response.text
        store_var first
        load_var response
        sys_op baml.http.Response.text
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @r#"
    Traceback (most recent call last):
      File "test.baml", line 5, in user.main
    uncaught throw: baml.errors.Io {message: "Response body has already been consumed"}
    "#);
}
