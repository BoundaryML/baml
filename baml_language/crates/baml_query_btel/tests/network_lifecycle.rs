//! Real loopback HTTP responses remain queryable when their bodies are abandoned.
use std::{sync::Arc, time::Duration};

use baml_query_btel::{Index, IndexOptions, QueryRequest, Status};
use bex_engine::{
    BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder, TelemetryRecording,
};
use serde_json::{Value as Json, json};
use sys_native::SysOpsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SOURCE: &str = r#"
    function unread(url: string) -> baml.http.Response { baml.http.fetch(url) }
    function text(url: string) -> string { baml.http.fetch(url).text() }
    function bytes(url: string) -> uint8array { baml.http.fetch(url).bytes() }
    function partial(url: string) -> baml.http.SseStream {
        let stream = baml.http.send_sse(baml.http.Request { method: "GET", url: url, headers: {}, body: "" });
        let first = stream.next();
        stream
    }
    function broken(url: string) -> string {
        baml.http.fetch(url).text() catch (e) { baml.errors.Io => "failed" }
    }
    function discard(url: string) -> int { let response = baml.http.fetch(url); response.status_code }
"#;

async fn call(engine: &Arc<BexEngine>, name: &str, url: String) -> BexExternalValue {
    tokio::time::timeout(
        Duration::from_secs(30),
        engine.call_function(
            name,
            vec![BexExternalValue::String(url.into())],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap()
}

fn query(index: &mut Index, sql: String) -> Vec<Vec<Json>> {
    let result = index
        .refresh_and_query(&QueryRequest {
            sql,
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        result.outcome.status,
        Status::Complete,
        "{:?}",
        result.outcome.diagnostics
    );
    result.rows
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn network_responses_discarded_or_host_retained_complete_in_both_query_views() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut request = vec![];
                let mut byte = [0; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if socket.read(&mut byte).await.unwrap() == 0 {
                        return;
                    }
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                let status = if request.contains("/500 ") {
                    500
                } else if request.contains("/504 ") {
                    504
                } else {
                    200
                };
                // The unread path never ends its body: finalization must not
                // wait for a socket, consume bytes, or rely on wire EOF.
                if request.contains("/pending ") {
                    socket
                        .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000000\r\n\r\n")
                        .await
                        .unwrap();
                    let _ = socket.read(&mut byte).await;
                    return;
                }
                if request.contains("/sse-pending ") {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: 1000000000\r\n\r\ndata: first\n\n").await.unwrap();
                    let _ = socket.read(&mut byte).await;
                    return;
                }
                if request.contains("/broken ") {
                    socket
                        .write_all(
                            b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\nConnection: close\r\n\r\nok",
                        )
                        .await
                        .unwrap();
                    return;
                }
                let response = format!(
                    "HTTP/1.1 {status} Test\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
            });
        }
    });
    let project = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(
                project.path(),
                btel_recorder::RecordingConfig::default(),
            ),
        )
        .unwrap(),
    );
    // A returned RustData carrier is retained outside the VM, even across GC.
    let retained = call(&engine, "unread", format!("{base}/pending")).await;
    for path in ["200", "500", "504"] {
        assert_eq!(
            call(&engine, "discard", format!("{base}/{path}")).await,
            BexExternalValue::Int(path.parse().unwrap())
        );
    }
    assert_eq!(
        call(&engine, "text", format!("{base}/text")).await,
        BexExternalValue::String("ok".into())
    );
    call(&engine, "bytes", format!("{base}/bytes")).await;
    let partial = call(&engine, "partial", format!("{base}/sse-pending")).await;
    assert_eq!(
        call(&engine, "broken", format!("{base}/broken")).await,
        BexExternalValue::String("failed".into())
    );
    engine.collect_garbage().await;
    tokio::time::timeout(Duration::from_secs(10), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    let announcements = query(&mut index, "SELECT count(*), sum(CASE WHEN is_complete THEN 1 ELSE 0 END) FROM span_announcements WHERE span_type = 'network_span'".into());
    assert_eq!(announcements, vec![vec![json!(8), json!(8)]]);
    let spans = query(&mut index, "SELECT span_name, status, network_event_values[0]['payload']['status'], network_event_values FROM spans WHERE span_type = 'network_span' ORDER BY span_name".into());
    assert_eq!(spans.len(), 8);
    for row in &spans {
        let name = row[0].as_str().unwrap();
        let consumed = name.ends_with("/text") || name.ends_with("/bytes");
        let broken = name.ends_with("/broken");
        assert_eq!(
            row[1],
            if consumed {
                json!("return")
            } else if broken {
                json!("user_error")
            } else {
                json!("cancel_error")
            }
        );
        let status = if name.ends_with("/500") {
            500
        } else if name.ends_with("/504") {
            504
        } else {
            200
        };
        assert_eq!(row[2], json!(status));
        let events = row[3].as_array().unwrap();
        let names: Vec<_> = events
            .iter()
            .map(|event| event["event_name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            if consumed {
                vec!["connection", "data", "await"]
            } else if broken {
                vec!["connection"]
            } else if name.ends_with("/sse-pending") {
                vec!["connection", "data", "drop"]
            } else {
                vec!["connection", "drop"]
            }
        );
    }
    // Late host release and repeated shutdown must add no second completion.
    drop(retained);
    drop(partial);
    engine.shutdown().await;
    assert_eq!(
        query(
            &mut index,
            "SELECT count(*) FROM spans WHERE span_type = 'network_span'".into()
        ),
        vec![vec![json!(8)]]
    );
    server.abort();
    server.await.unwrap_err();
}
