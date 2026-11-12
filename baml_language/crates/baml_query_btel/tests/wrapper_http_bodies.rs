//! Exercise the wrapper's real HTTP callers against loopback, then query their spans.
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use baml_query_btel::{Index, IndexOptions, QueryRequest, Status};
use bex_engine::{
    BexEngine, BexExternalValue, CallId, FunctionCallContextBuilder, TelemetryRecording,
};
use serde_json::{Value as Json, json};
use sys_native::SysOpsExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const CALLERS: &str = r#"
    function check_download(url: string) -> string {
        {
            let data = Artifact { url: url, sha256: hash("ok".to_utf8()) }.download();
            ensure(hash(data) == hash("ok".to_utf8()), "unexpected archive bytes");
            "ok"
        } catch (e) {
            baml.errors.InvalidArgument => e.message,
        }
    }
    function check_manifest(url: string) -> string {
        fetch_text(url) catch (e) { baml.errors.InvalidArgument => e.message }
    }
    function check_delivery(path: string) -> string {
        match (TelemetryQueue { directory: "unused" }.send_file(path)) {
            TelemetryDelivery.Delivered => "delivered",
            TelemetryDelivery.Rejected => "rejected",
            TelemetryDelivery.Unavailable => "unavailable",
        }
    }
"#;

fn program() -> bex_vm_types::Program {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../wrapper/baml_src");
    let mut files: Vec<(String, String)> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "baml")
        })
        .map(|path| {
            let name = path.file_name().unwrap().to_str().unwrap().to_owned();
            let mut source = std::fs::read_to_string(&path).unwrap();
            if name == "build.baml" {
                // Development builds disable delivery. Enable it with a fixture label;
                // the injected environment below restricts every destination to loopback.
                source = source.replace(
                    "function telemetry_api_key() -> string {\n    \"\"\n}",
                    "function telemetry_api_key() -> string {\n    \"fixture\"\n}",
                );
            }
            (name, source)
        })
        .collect();
    files.sort_by(|a, b| a.0.cmp(&b.0));
    files.push(("http_test.baml".into(), CALLERS.into()));
    baml_test_support::compile_multi_file(
        &files
            .iter()
            .map(|(path, source)| (path.as_str(), source.as_str()))
            .collect::<Vec<_>>(),
    )
}

async fn call(engine: &Arc<BexEngine>, name: &str, value: String) -> String {
    let value = tokio::time::timeout(
        Duration::from_secs(30),
        engine.call_function(
            name,
            vec![BexExternalValue::String(value.into())],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    let BexExternalValue::String(value) = value else {
        panic!("expected a string: {value:?}");
    };
    value.to_string()
}

fn query(index: &mut Index, sql: &str) -> Vec<Vec<Json>> {
    let result = index
        .refresh_and_query(&QueryRequest {
            sql: sql.into(),
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
async fn wrapper_status_only_responses_are_consumed_without_changing_http_decisions() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        loop {
            let (mut socket, _) = listener.accept().await.unwrap();
            tokio::spawn(async move {
                let mut request = Vec::new();
                let mut byte = [0; 1];
                while !request.ends_with(b"\r\n\r\n") {
                    if socket.read(&mut byte).await.unwrap() == 0 {
                        return;
                    }
                    request.push(byte[0]);
                }
                let request = String::from_utf8(request).unwrap();
                let content_length = request
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .map(|length| length.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                let mut body = vec![0; content_length];
                socket.read_exact(&mut body).await.unwrap();
                let path = request.split_whitespace().nth(1).unwrap();
                let status = if path.contains("500") {
                    500
                } else if path.contains("504") {
                    504
                } else if path.contains("204") {
                    204
                } else {
                    200
                };
                let length = if path.contains("broken") || path.contains("timeout") {
                    10
                } else if status == 204 {
                    0
                } else {
                    2
                };
                let payload = if status == 204 { "" } else { "ok" };
                let response = format!(
                    "HTTP/1.1 {status} Fixture\r\nContent-Length: {length}\r\nConnection: close\r\n\r\n{payload}"
                );
                socket.write_all(response.as_bytes()).await.unwrap();
                if path.contains("timeout") {
                    tokio::time::sleep(Duration::from_secs(3)).await;
                }
            });
        }
    });
    let project = tempfile::tempdir().unwrap();
    let home = project.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::write(home.join("wrapper-telemetry.toml"), "enabled = true\n").unwrap();
    let host = Arc::new(Mutex::new(base.clone()));
    let mut ops = sys_native::SysOps::native();
    ops.baml_env_get = {
        let host = Arc::clone(&host);
        let home = home.to_str().unwrap().to_owned();
        Arc::new(move |heap, permit, mut args, _, _| {
            let key = args.pop().unwrap().as_string(heap, permit).unwrap();
            let value = match key.as_str() {
                "BAML_HOME" => Some(home.clone()),
                "DO_NOT_TRACK" => Some("0".into()),
                "BAML_WRAPPER_TELEMETRY_HOST" => Some(host.lock().unwrap().clone()),
                _ => None,
            };
            sys_types::SysOpResult::Ready(Ok(value.map_or(BexExternalValue::Null, |value| {
                BexExternalValue::String(value.into())
            })))
        })
    };
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program(),
            Arc::new(ops),
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
    let mut expected = BTreeMap::new();
    for (caller, function, statuses) in [
        (
            "download",
            "check_download",
            vec!["200", "500", "504", "broken500"],
        ),
        (
            "manifest",
            "check_manifest",
            vec!["200", "500", "504", "broken504"],
        ),
    ] {
        for case in statuses {
            let url = format!("{base}/{caller}/{case}");
            let result = call(&engine, function, url.clone()).await;
            let status = if case.contains("500") {
                500
            } else if case.contains("504") {
                504
            } else {
                200
            };
            if status == 200 {
                assert_eq!(result, "ok");
            } else {
                assert_eq!(
                    result,
                    format!(
                        "failed to {} {url}: HTTP {status}",
                        if caller == "download" {
                            "download"
                        } else {
                            "fetch"
                        }
                    )
                );
            }
            expected.insert(format!("GET {url}"), (status, case.starts_with("broken")));
        }
    }
    for case in [
        "200",
        "204",
        "500",
        "504",
        "broken200",
        "broken500",
        "timeout200",
    ] {
        *host.lock().unwrap() = format!("{base}/post/{case}");
        let queue = home.join(format!("{case}.jsonl"));
        let event = "{\"event\":\"fixture\"}\n";
        std::fs::write(&queue, event).unwrap();
        let result = call(&engine, "check_delivery", queue.to_str().unwrap().into()).await;
        let status = if case.contains("500") {
            500
        } else if case.contains("504") {
            504
        } else if case == "204" {
            204
        } else {
            200
        };
        assert_eq!(
            result,
            if (200..300).contains(&status) {
                "delivered"
            } else {
                "rejected"
            }
        );
        assert_eq!(
            std::fs::read_to_string(queue).unwrap(),
            if (200..300).contains(&status) {
                ""
            } else {
                event
            }
        );
        expected.insert(
            format!("POST {base}/post/{case}/capture/"),
            (
                status,
                case.starts_with("broken") || case.starts_with("timeout"),
            ),
        );
    }
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let mut index = Index::for_project(project.path(), IndexOptions::default()).unwrap();
    assert_eq!(
        query(
            &mut index,
            "SELECT count(*), sum(CASE WHEN is_complete THEN 1 ELSE 0 END) FROM span_announcements WHERE span_type = 'network_span'"
        ),
        vec![vec![json!(expected.len()), json!(expected.len())]]
    );
    let rows = query(
        &mut index,
        "SELECT span_name, status, network_event_values[0]['payload']['status'], network_event_values[-1]['event_name'] FROM spans WHERE span_type = 'network_span'",
    );
    assert_eq!(rows.len(), expected.len());
    for row in rows {
        let (status, broken) = expected.remove(row[0].as_str().unwrap()).unwrap();
        assert_eq!(
            row[1],
            if broken {
                json!("user_error")
            } else {
                json!("return")
            }
        );
        assert_eq!(row[2], json!(status));
        assert_eq!(
            row[3],
            if broken {
                json!("connection")
            } else {
                json!("await")
            }
        );
    }
    assert!(expected.is_empty());
    server.abort();
    server.await.unwrap_err();
}
