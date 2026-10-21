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

/// Regression test: field access on a foreign class instance must compile
/// as `load_field`, NOT `load_map_element`.
#[tokio::test]
async fn foreign_class_field_access_compiles_correctly() {
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/test",
        // A non-default success status preserves the old status-code propagation
        // assertion while this regression pins the stronger bytecode contract.
        status: 201,
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
    assert_eq!(output.result, Ok(BexExternalValue::Int(201)));
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

/// A listener that accepts connections and then says nothing, so a request
/// hangs after connecting. Kept in Rust for the reason given on
/// `http_fetch_timeout_fires`.
async fn silent_server() -> (tokio::task::JoinHandle<()>, std::net::SocketAddr) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let mut held = Vec::new();
        loop {
            if let Ok((conn, _)) = listener.accept().await {
                held.push(conn);
            }
        }
    });
    (server, addr)
}

/// BAML that labels the `Timeout` thrown by `call`: which deadline fired and
/// the limit it reports.
fn timeout_label_program(call: &str) -> String {
    format!(
        r#"
        function main() -> string {{
            {call} catch_all (e) {{
                let t: baml.errors.Timeout => {{
                    return `${{t.timeout_type}}:${{t.duration?.to_milliseconds() ?? -1n}}`;
                }},
                _ => {{ return `other:${{e.to_string()}}`; }},
            }};
            "unexpected success"
        }}
        "#
    )
}

#[tokio::test]
async fn http_send_uses_request_timeout() {
    let (server, addr) = silent_server().await;
    let output = baml_test!(&timeout_label_program(&format!(
        r#"baml.http.send(
            baml.http.Request.from_url("http://{addr}/")
                .set_method("GET")
                .set_timeout(baml.time.Duration.from_milliseconds(100))
        )"#
    )));
    server.abort();
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("timeout:100".into()))
    );
}

#[tokio::test]
async fn http_send_timeout_argument_overrides_request_timeout() {
    let (server, addr) = silent_server().await;
    // The request's own limit is an hour; the argument wins.
    let output = baml_test!(&timeout_label_program(&format!(
        r#"baml.http.send(
            baml.http.Request.from_url("http://{addr}/")
                .set_method("GET")
                .set_timeout(baml.time.Duration.from_hours(1)),
            timeout = baml.time.Duration.from_milliseconds(150),
        )"#
    )));
    server.abort();
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("timeout:150".into()))
    );
}

#[tokio::test]
async fn http_send_sse_uses_request_timeout() {
    let (server, addr) = silent_server().await;
    let output = baml_test!(&timeout_label_program(&format!(
        r#"baml.http.send_sse(
            baml.http.Request {{
                method: "GET",
                url: "http://{addr}/",
                headers: {{}},
                body: "",
                timeout: baml.time.Duration.from_milliseconds(100),
            }}
        )"#
    )));
    server.abort();
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("timeout:100".into()))
    );
}

#[tokio::test]
async fn http_connect_timeout_includes_tls_handshake() {
    // TCP succeeds, but the peer never answers the TLS ClientHello. This
    // exercises connection establishment deterministically, without a
    // blackhole IP. The total is left at its default, so only the connect
    // limit can fire.
    let (server, addr) = silent_server().await;
    let output = baml_test!(&timeout_label_program(&format!(
        r#"baml.http.send(
            baml.http.Request.from_url("https://{addr}/")
                .set_method("GET")
                .set_connect_timeout(baml.time.Duration.from_milliseconds(100))
        )"#
    )));
    server.abort();
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("connect_timeout:100".into()))
    );
}

#[tokio::test]
async fn http_non_positive_timeout_is_no_limit() {
    // A zero total opts out of the default instead of failing immediately.
    let (_server, uri) = mock(&[MockEndpoint {
        path: "/ok",
        status: 200,
        body: Some("body"),
    }])
    .await;
    let output = baml_test!(&format!(
        r#"
        function main() -> string {{
            let zero = baml.http.fetch("{uri}/ok", timeout = baml.time.Duration.from_seconds(0)).text();
            let negative = baml.http.send(
                baml.http.Request.from_url("{uri}/ok")
                    .set_method("GET")
                    .set_timeout(baml.time.Duration.from_seconds(-1))
                    .set_connect_timeout(baml.time.Duration.from_seconds(0))
            ).text();
            `${{zero}}|${{negative}}`
        }}
        "#
    ));
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("body|body".into()))
    );
}

#[tokio::test]
async fn http_request_from_url_needs_a_method() {
    // `from_url` leaves the method unset, and sending that is an error.
    let output = baml_test!(
        r#"
        function main() -> string {
            baml.http.send(baml.http.Request.from_url("http://127.0.0.1:9/")) catch_all (e) {
                let invalid: baml.errors.InvalidArgument => { return invalid.message; },
                _ => { return `other:${e.to_string()}`; },
            };
            "unexpected success"
        }
        "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "Invalid HTTP method '': the request has no method set".into()
        ))
    );
}
