//! Unified tests for HTTP operations.

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
        dispatch_future baml.http.fetch
        await
        dispatch_future baml.http.Response.text
        await
        return
    }
    "#);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Hello from HTTP!".to_string()))
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
        dispatch_future baml.http.fetch
        await
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
        dispatch_future baml.http.fetch
        await
        dispatch_future baml.http.Response.ok
        await
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
        dispatch_future baml.http.fetch
        await
        dispatch_future baml.http.Response.ok
        await
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
        dispatch_future baml.http.fetch
        await
        load_field .url
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::String(expected_url)));
}

#[tokio::test]
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
        dispatch_future baml.http.fetch
        await
        load_field .status_code
        return
    }
    "#);
    assert_eq!(output.result, Ok(BexExternalValue::Int(0)));
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
        dispatch_future baml.http.fetch
        await
        store_var response
        load_var response
        dispatch_future baml.http.Response.text
        await
        store_var first
        load_var response
        dispatch_future baml.http.Response.text
        await
        return
    }
    "#);
    insta::assert_snapshot!(output.result.unwrap_err().to_string(), @"failed to call baml.http.Response.text: Response body has already been consumed");
}
