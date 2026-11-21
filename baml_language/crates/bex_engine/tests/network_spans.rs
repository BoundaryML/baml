//! Every `baml.http` request is a span of its own: its request, the events of
//! its response, and how it ended, recorded by the runtime.
#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_core::{
    cas::{DecodedBlob, DecodedObject, DecodedRoot, DecodedValue},
    reader::store::{CasLimits, CasStore},
};
use btel_recorder::{RecordingConfig, proto};
use serde_json::{Value as Json, json};
use sys_native::SysOpsExt;

const SOURCE: &str = r##"
    class Flag {
        set: bool,
    }

    class Canned {
        server: baml.http.Server,
        slow: Flag,

        function handle(self, req: baml.http.ServerRequest) -> baml.http.Response {
            if (req.url.starts_with("/unread-500")) {
                return baml.http.Response.new(500, { "set-cookie": "session=sk-cookie" }, "private-unread-error".to_utf8());
            }
            if (req.url.starts_with("/unread-504")) {
                return baml.http.Response.new(504, {}, "private-unread-error".to_utf8());
            }
            if (req.url.starts_with("/missing")) {
                return baml.http.Response.new(404, { "content-type": "text/plain" }, "not here".to_utf8());
            }
            if (req.url.starts_with("/sse-close")) {
                // Its event arrives after the program has closed the stream,
                // which is still read on the wire.
                let resp = baml.http.Response.new_streaming(200, { "content-type": "text/event-stream" });
                spawn {
                    {
                        baml.sys.sleep(baml.time.Duration.from_milliseconds(300n));
                        resp.write("event: message\ndata: late\n\n".to_utf8());
                        resp.end();
                        null
                    } catch (e) {
                        _ => null
                    }
                };
                return resp;
            }
            if (req.url.starts_with("/sse")) {
                let events = "event: message\ndata: one\n\nevent: message\ndata: two\n\n";
                return baml.http.Response.new(200, { "content-type": "text/event-stream" }, events.to_utf8());
            }
            if (req.url.starts_with("/slow")) {
                self.slow.set = true;
                let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(3000n)) catch (e) { _ => null };
            }
            baml.http.Response.new(
                200,
                { "content-type": "application/json", "x-request-id": "req_1", "set-cookie": "session=sk-cookie" },
                `{"ok":true}`.to_utf8(),
            )
        }
    }

    function request(base: string, path: string) -> baml.http.Request {
        baml.http.Request {
            method: "POST",
            url: base + path + "?key=sk-query&alt=json",
            headers: { "Content-Type": "application/json", "Authorization": "Bearer sk-header" },
            body: `{"model":"m"}`,
        }
    }

    function slow_text(base: string) -> string {
        baml.http.send(request(base, "/slow")).text()
    }

    function main() -> string {
        let canned = Canned { server: baml.http.Server.bind("127.0.0.1:0"), slow: Flag { set: false } };
        let task = spawn { canned.server.serve(canned.handle) };
        let base = "http://" + canned.server.addr();

        let ok = baml.http.send(request(base, "/ok")).text();
        let bytes = baml.http.send(request(base, "/bytes")).bytes();
        let missing = baml.http.send(request(base, "/missing")).text();
        // The program sees the URL with its credential-named query values
        // redacted: errors reach logs, and the query carries the key.
        let refused = {
            baml.http.send(request("http://127.0.0.1:1", "/refused"));
            "sent"
        } catch (e) {
            baml.errors.Io => if (e.message.includes("key=sk-query")) { "leaked" } else { "redacted" },
            _ => "other"
        };

        // `reqwest` prints this URL parsed: `http://`, the spaces encoded.
        let normalized = {
            baml.http.send(baml.http.Request {
                method: "GET",
                url: "HTTP://127.0.0.1:1/Refused Path?key=sk-norm&q=a b",
                headers: {},
                body: "",
            });
            "sent"
        } catch (e) {
            baml.errors.Io => if (e.message.includes("/Refused%20Path?key=REDACTED") && !e.message.includes("sk-norm")) { "normalized" } else { e.message },
            _ => "other"
        };

        let sse = baml.http.send_sse(request(base, "/sse"));
        let batches = 0;
        while (sse.next() != null) {
            batches = batches + 1;
        }
        let early = baml.http.send_sse(request(base, "/sse-close"));
        early.close();
        // Let the late event arrive and be drained by later sys-ops; the
        // second close keeps the stream alive until then.
        baml.sys.sleep(baml.time.Duration.from_milliseconds(600n));
        early.close();

        let never = baml.http.send(request(base, "/never"));
        let alias = never;
        let unread500 = baml.http.send(request(base, "/unread-500"));
        let unread504 = baml.http.send(request(base, "/unread-504"));
        // Parsed data can arrive before the consumer abandons an SSE stream.
        let partial = baml.http.send_sse(request(base, "/sse-partial"));
        let first = partial.next();

        let slow = spawn { slow_text(base) };
        while (!canned.slow.set) {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(5n));
        }
        let _ = slow.cancel();
        let cancelled = (await slow) catch (e) { baml.panics.Cancelled => "cancelled" };

        let _ = task.cancel();
        ok + "|" + missing + "|" + refused + "|" + normalized + "|" + cancelled
    }
"##;

async fn record(directory: &Path) -> Arc<BexEngine> {
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory, RecordingConfig::default()),
        )
        .unwrap(),
    );
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        engine.call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        result,
        BexExternalValue::String(r#"{"ok":true}|not here|redacted|normalized|cancelled"#.into())
    );
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .unwrap();
    engine
}

/// One request's span as the recording files hold it.
#[derive(Default)]
struct Span {
    announcement: Option<proto::NetworkAnnouncement>,
    events: Vec<proto::NetworkEvent>,
    completion: Option<proto::NetworkCompletion>,
    /// Event names and `completion`, in the order the files hold them.
    written: Vec<String>,
}

impl Span {
    fn names(&self) -> Vec<&str> {
        self.events
            .iter()
            .map(|event| event.name.as_str())
            .collect()
    }

    fn event(&self, name: &str) -> &proto::NetworkEvent {
        self.events.iter().find(|event| event.name == name).unwrap()
    }
}

fn spans(files: &[proto::RecordingFile]) -> BTreeMap<String, Span> {
    use proto::span_event::Event;
    let mut spans = BTreeMap::<u64, Span>::new();
    for file in files {
        for (section, event) in file
            .spans
            .iter()
            .flat_map(|spans| &spans.sections)
            .flat_map(|section| section.events.iter().map(move |event| (section, event)))
        {
            match &event.event {
                // A local recording's slim start record: the end record is
                // the whole span.
                Some(Event::NetworkAnnouncement(span)) => {
                    assert!(span.request_cas_id.is_none());
                }
                Some(Event::NetworkEvent(_)) => panic!("events ride their request's completion"),
                Some(Event::NetworkCompletion(done)) => {
                    assert_ne!(done.span_id, 0);
                    assert_eq!(section.thread_id, done.thread_id);
                    assert!(
                        file.header.as_ref().unwrap().format_minor
                            >= btel_core::recording::minor::WHOLE_SPAN
                    );
                    let span = spans.entry(done.span_id).or_default();
                    assert!(span.completion.is_none(), "a span ends once");
                    span.announcement = Some(proto::NetworkAnnouncement {
                        id: done.span_id,
                        parent_id: done.parent_id,
                        call_path_id: done.call_path_id,
                        started_at_ticks: done.started_at_ticks,
                        method: done.method.clone(),
                        url: done.url.clone(),
                        request_cas_id: done.request_cas_id,
                    });
                    span.events.clone_from(&done.events);
                    span.written = done.events.iter().map(|event| event.name.clone()).collect();
                    span.written.push("completion".to_owned());
                    span.completion = Some(done.clone());
                }
                _ => {}
            }
        }
    }
    // One span's events can be written by different VM threads: order them
    // by time, as `baml query` does.
    for span in spans.values_mut() {
        span.events.sort_by_key(|event| event.at_ticks);
    }
    // Keyed by path, the part of each request's URL that differs.
    spans
        .into_values()
        .map(|span| {
            let url = &span.announcement.as_ref().expect("announced").url;
            let path = url.split('?').next().unwrap().rsplit('/').next().unwrap();
            (path.to_owned(), span)
        })
        .collect()
}

fn json(cas: &CasStore, snapshot: &DecodedBlob) -> Json {
    let DecodedRoot::Value(value) = &snapshot.root else {
        panic!("network snapshots are values")
    };
    value_json(cas, snapshot, value)
}

fn value_json(cas: &CasStore, snapshot: &DecodedBlob, value: &DecodedValue) -> Json {
    match value {
        DecodedValue::Null => Json::Null,
        DecodedValue::Int(value) => (*value).into(),
        DecodedValue::String(text) => text.as_ref().into(),
        DecodedValue::Object(id) => match snapshot.object(*id) {
            DecodedObject::Map { entries, .. } => entries
                .iter()
                .map(|(key, value)| {
                    let key = value_json(cas, snapshot, key);
                    (
                        key.as_str()
                            .expect("network map keys are strings")
                            .to_owned(),
                        value_json(cas, snapshot, value),
                    )
                })
                .collect::<serde_json::Map<_, _>>()
                .into(),
            DecodedObject::Instance { fields, .. } => fields
                .iter()
                .map(|(key, value)| (key.to_string(), value_json(cas, snapshot, value)))
                .collect::<serde_json::Map<_, _>>()
                .into(),
            DecodedObject::Uint8Array { data, .. } => data.clone().into(),
            object => format!("{object:?}").into(),
        },
        // A long string, or a large part of a payload, is a blob of its own.
        DecodedValue::External(child) => json(cas, &blob(cas, snapshot.children[child.0 as usize])),
        value => format!("{value:?}").into(),
    }
}

fn blob(cas: &CasStore, id: btel_snapshot::CasId) -> std::sync::Arc<DecodedBlob> {
    cas.load(id)
        .snapshot
        .unwrap_or_else(|error| panic!("network capture was not persisted: {error:?}"))
}

fn is_hash(value: &Json) -> bool {
    value
        .as_str()
        .is_some_and(|value| value.len() == 23 && value.starts_with("sha256:"))
}

fn files_under(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            files_under(&path, found);
        } else {
            found.push(path);
        }
    }
}

/// The recording's spans by path, and its CAS.
fn read(directory: &Path, engine: &BexEngine) -> (BTreeMap<String, Span>, CasStore) {
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let read =
        btel_core::recording::read_directory(engine.telemetry_recording_directory().unwrap())
            .unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    (
        spans(&read.files),
        CasStore::new(directory.join("cas"), CasLimits::default()),
    )
}

fn load(cas: &CasStore, id: Option<proto::CasId>) -> Json {
    json(
        cas,
        &blob(cas, btel_core::recording::cas_id(id.expect("captured"))),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_request_records_its_span_events_and_end() {
    let directory = tempfile::tempdir().unwrap();
    let engine = record(directory.path()).await;
    let (spans, cas) = read(directory.path(), &engine);
    let load = |id| load(&cas, id);
    let outcome = |span: &Span| {
        let done = span.completion.as_ref().expect("completed");
        (
            proto::InvocationOutcome::try_from(done.outcome).unwrap(),
            done.panicked,
        )
    };
    let ok = (proto::InvocationOutcome::Ok, false);
    assert_eq!(
        spans.keys().map(String::as_str).collect::<Vec<_>>(),
        [
            "Refused Path",
            "bytes",
            "missing",
            "never",
            "ok",
            "refused",
            "slow",
            "sse",
            "sse-close",
            "sse-partial",
            "unread-500",
            "unread-504"
        ]
    );

    // send + text(): the request, its response's status and body, the read.
    let span = &spans["ok"];
    let announcement = span.announcement.as_ref().unwrap();
    assert_eq!(announcement.method, "POST");
    let url = &announcement.url;
    assert!(url.contains("/ok?key=sha256:") && url.contains("&alt=sha256:"));
    assert!(!url.contains("sk-query"));
    let request = load(announcement.request_cas_id);
    let fields = &request["request"];
    assert_eq!(fields["method"], "POST");
    assert_eq!(&fields["url"], url.as_str());
    assert_eq!(fields["headers"]["content-type"], "application/json");
    assert!(is_hash(&fields["headers"]["authorization"]));
    assert_eq!(fields["body"], r#"{"model":"m"}"#);
    assert_eq!(span.names(), ["connection", "data", "await"]);
    let connection = load(span.event("connection").payload_cas_id);
    assert_eq!(connection["status"], 200);
    assert_eq!(connection["headers"]["x-request-id"], "req_1");
    assert_eq!(connection["headers"]["content-type"], "application/json");
    assert!(is_hash(&connection["headers"]["set-cookie"]));
    assert_eq!(
        load(span.event("data").payload_cas_id),
        json!(r#"{"ok":true}"#)
    );
    assert!(span.event("await").payload_cas_id.is_none());
    assert_eq!(outcome(span), ok);
    let done = span.completion.as_ref().unwrap();
    assert!(done.error_cas_id.is_none());
    assert!(announcement.started_at_ticks <= done.completed_at_ticks);
    assert!(done.completed_at_ticks <= span.event("await").at_ticks);

    // bytes() reads the same way.
    let span = &spans["bytes"];
    assert_eq!(span.names(), ["connection", "data", "await"]);
    assert_eq!(outcome(span), ok);

    // A non-2xx response is still a request that succeeded.
    let span = &spans["missing"];
    assert_eq!(span.names(), ["connection", "data", "await"]);
    assert_eq!(load(span.event("connection").payload_cas_id)["status"], 404);
    assert_eq!(load(span.event("data").payload_cas_id), json!("not here"));
    assert_eq!(outcome(span), ok);

    // A transport error ends the span errored, with the error a handler
    // sees, quoting the sanitized URL where the program's quotes it redacted
    // for logs.
    let span = &spans["refused"];
    assert!(span.events.is_empty());
    assert_eq!(outcome(span), (proto::InvocationOutcome::Errored, false));
    let error = load(span.completion.as_ref().unwrap().error_cas_id);
    let message = error["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("HTTP send failed"), "{error}");
    let sanitized = &span.announcement.as_ref().unwrap().url;
    assert!(message.contains(&format!("({sanitized})")), "{error}");
    assert!(!message.contains("REDACTED"), "{error}");

    // The same, when `reqwest` prints the URL differently from how the
    // program wrote it: the capture quotes the parsed URL, sanitized.
    let span = &spans["Refused Path"];
    assert_eq!(outcome(span), (proto::InvocationOutcome::Errored, false));
    let error = load(span.completion.as_ref().unwrap().error_cas_id);
    let message = error["error"]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("(http://127.0.0.1:1/Refused%20Path?key=sha256:"),
        "{error}"
    );
    assert!(!message.contains("sk-norm"), "{error}");
    assert!(!message.contains("q=a%20b"), "{error}");

    // A stream read to its end: one data per event, its end, then the read.
    let span = &spans["sse"];
    assert_eq!(span.names(), ["connection", "data", "data", "end", "await"]);
    assert_eq!(
        load(span.events[1].payload_cas_id),
        json!({"event": "message", "data": "one", "id": null})
    );
    assert_eq!(load(span.events[2].payload_cas_id)["data"], "two");
    assert_eq!(outcome(span), ok);

    // Closed early: a close, and nothing after its completion, though the
    // stream's event arrived on the wire later.
    let span = &spans["sse-close"];
    assert_eq!(span.written, ["connection", "close", "completion"]);
    assert_eq!(outcome(span), ok);

    // Small unread bodies finish at EOF, independently of status or app reads.
    // No await is invented, and the buffered bytes remain capturable.
    for (path, status) in [("never", 200), ("unread-500", 500), ("unread-504", 504)] {
        let span = &spans[path];
        assert_eq!(span.names(), ["connection", "data"]);
        assert_eq!(
            load(span.event("connection").payload_cas_id)["status"],
            status
        );
        assert_eq!(outcome(span), ok);
        let done = span.completion.as_ref().unwrap();
        assert!(done.error_cas_id.is_none());
        assert_eq!(done.completed_at_ticks, span.event("data").at_ticks);
    }
    // Even wire-complete SSE data is abandoned until the program reads EOF.
    let span = &spans["sse-partial"];
    assert_eq!(span.names(), ["connection", "data", "data", "end", "drop"]);
    assert_eq!(outcome(span), (proto::InvocationOutcome::Cancelled, false));

    // Cancelled while waiting for the response.
    let span = &spans["slow"];
    assert!(span.events.is_empty());
    assert_eq!(outcome(span), (proto::InvocationOutcome::Cancelled, false));

    // No header, cookie or URL secret reached the disk: not the recording,
    // not the CAS, not the transport error's message.
    let mut files = Vec::new();
    files_under(directory.path(), &mut files);
    let holding = |secret: &str| {
        files
            .iter()
            .filter(|file| {
                std::fs::read(file)
                    .unwrap()
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes())
            })
            .count()
    };
    assert!(!files.is_empty());
    for secret in ["sk-query", "sk-norm", "sk-header", "sk-cookie"] {
        assert_eq!(holding(secret), 0, "{secret} reached the disk");
    }
}

// Only the native recording can show when an unread transfer completed while
// BAML was idle, and whether artifact permissions suppressed the body capture.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn eager_eof_precedes_host_read_and_respects_body_capture_permissions() {
    use bex_engine::EngineConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    for omit_http_bodies in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/idle", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0; 4096];
            assert!(socket.read(&mut request).await.unwrap() > 0);
            socket
                .write_all(b"HTTP/1.1 500 Error\r\nContent-Length: 14\r\n\r\neager-private!")
                .await
                .unwrap();
        });
        let engine = Arc::new(
            BexEngine::new_with_config(
                baml_test_support::compile_source(
                    r#"
                function get(url: string) -> baml.http.Response { baml.http.fetch(url) }
                function text(response: baml.http.Response) -> int { response.text().length() }
            "#,
                ),
                Arc::new(sys_native::SysOps::native()),
                vec![],
                EngineConfig {
                    clock_mode: btel_clock::ClockMode::Monotonic,
                    artifact_telemetry: Some(btel_settings::artifact::ArtifactTelemetry {
                        omit_http_bodies,
                        ..Default::default()
                    }),
                    recording: Some(TelemetryRecording::local_files_in(
                        directory.path(),
                        RecordingConfig::default(),
                    )),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        let response = engine
            .call_function(
                "get",
                vec![BexExternalValue::String(url.into())],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
        server.await.unwrap();
        // No BAML execution or GC drives recording during this interval.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let result = engine
            .call_function(
                "text",
                vec![response],
                FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                true,
            )
            .await
            .unwrap();
        assert_eq!(result, BexExternalValue::Int(14));
        engine.shutdown().await;
        let (spans, cas) = read(directory.path(), &engine);
        let span = &spans["idle"];
        // File sections from different OS producers may interleave; queries
        // order the owned event timestamps, while terminal time stays at EOF.
        assert_eq!(span.names(), ["connection", "data", "await"]);
        let done = span.completion.as_ref().unwrap();
        assert_eq!(done.outcome, proto::InvocationOutcome::Ok as i32);
        assert_eq!(done.completed_at_ticks, span.event("data").at_ticks);
        assert!(done.completed_at_ticks < span.event("await").at_ticks);
        assert_eq!(
            span.event("data").payload_cas_id.is_none(),
            omit_http_bodies
        );
        if !omit_http_bodies {
            assert_eq!(
                load(&cas, span.event("data").payload_cas_id),
                json!("eager-private!")
            );
        } else {
            let mut files = Vec::new();
            files_under(directory.path(), &mut files);
            assert!(files.iter().all(|file| {
                !std::fs::read(file)
                    .unwrap()
                    .windows(b"eager-private!".len())
                    .any(|bytes| bytes == b"eager-private!")
            }));
        }
    }
}

const HANDLER_SOURCE: &str = r##"
    function work() -> int {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(20n));
        1
    }

    function handle(req: baml.http.ServerRequest) -> baml.http.Response throws never {
        let _ = work($trace = trace.span()) catch (e) { _ => 0 };
        baml.http.Response.new(200, {}, "ok".to_utf8())
    }

    function serve_once() -> string {
        let server = baml.http.Server.bind("127.0.0.1:0");
        let task = spawn { server.serve(handle) };
        let text = baml.http.fetch("http://" + server.addr() + "/").text();
        let _ = task.cancel();
        text
    }

    function main() -> string {
        serve_once($trace = trace.context(distinct_id = "caller"))
    }
"##;

/// A request handler runs on a task the server spawns for it. That task
/// starts running when its body does, not when it completes: the handler's
/// own calls come after its start. It runs in its caller's context from its
/// first event on.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_request_task_starts_running_before_its_handler_calls() {
    use proto::span_event::Event;
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(HANDLER_SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        engine.call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(result, BexExternalValue::String("ok".into()));
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let read =
        btel_core::recording::read_directory(engine.telemetry_recording_directory().unwrap())
            .unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);

    // Per thread: when it began running, when it completed, and when each
    // function it ran directly was entered.
    let mut running = BTreeMap::<u64, u64>::new();
    let mut completed = BTreeMap::<u64, u64>::new();
    let mut entered = BTreeMap::<u64, Vec<u64>>::new();
    let mut contexts = BTreeMap::<u64, Vec<_>>::new();
    for section in read
        .files
        .iter()
        .flat_map(|file| file.spans.iter())
        .flat_map(|spans| &spans.sections)
    {
        contexts
            .entry(section.thread_id)
            .or_default()
            .push(section.context);
        for event in &section.events {
            match &event.event {
                Some(Event::ThreadRunning(_)) => panic!("a thread's completion says when it ran"),
                Some(Event::ThreadCompletion(done)) => {
                    if let Some(at) = done.running_at_ticks {
                        running.insert(section.thread_id, at);
                    }
                    completed.insert(section.thread_id, done.completed_at_ticks);
                }
                Some(Event::FunctionAnnouncement(call)) if call.parent_id == section.thread_id => {
                    entered
                        .entry(section.thread_id)
                        .or_default()
                        .push(call.entered_at_ticks);
                }
                _ => {}
            }
        }
    }
    // The handler task is the one that entered `work`, directly under it.
    let entered = entered.into_iter().collect::<Vec<_>>();
    let [(thread, calls)] = &entered[..] else {
        panic!("one thread called `work` directly: {entered:?}")
    };
    let started = running[thread];
    assert!(
        calls.iter().all(|&at| started <= at),
        "began running at {started}, after its call entered at {calls:?}"
    );
    assert!(started < completed[thread]);
    // Every section of the handler task, its announcement's included, is in
    // the context `main` gave `serve_once`.
    let handler = &contexts[thread];
    assert!(
        handler
            .iter()
            .all(|context| context.is_some() && *context == handler[0]),
        "{handler:?}"
    );
}

// The native read-cancellation guard has its own alias-retention regression;
// this verifies its owned terminal notification's actual recording contract.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn application_read_cancellation_stays_cancelled_without_drop_on_the_wire() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let directory = tempfile::tempdir().unwrap();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/cancelled", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut request = [0; 4096];
        assert!(socket.read(&mut request).await.unwrap() > 0);
        socket
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000000000\r\n\r\n")
            .await
            .unwrap();
        let _ = socket.read(&mut request).await;
    });
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(
                "function get(url: string) -> baml.http.Response { baml.http.fetch(url) }",
            ),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let response = engine
        .call_function(
            "get",
            vec![BexExternalValue::String(url.into())],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    let BexExternalValue::Instance { fields, .. } = &response else {
        panic!("response");
    };
    // The body is a `@@rust_type` instance the host holds by reference: its
    // payload reports the request it belongs to.
    let BexExternalValue::Adt(bex_external_types::BexExternalAdt::TaggedHeapHandle {
        heap_handle,
        ..
    }) = &fields["_body"]
    else {
        panic!("native body");
    };
    {
        use bex_heap::HeapPermit as _;
        let permit = engine
            .heap_permit_manager()
            .new_permit(())
            .await
            .acquire()
            .await;
        let body = engine.resolve_handle(permit.proof(), heap_handle).unwrap();
        // SAFETY: the permit keeps the body live and in place while it is read.
        #[expect(unsafe_code, reason = "read the body the permit keeps in place")]
        let bex_vm_types::Object::RustInstance(instance) = (unsafe { body.get() }) else {
            panic!("a payload-backed body");
        };
        instance.payload.network().unwrap().cancel_read();
    }
    engine.shutdown().await;
    tokio::time::timeout(Duration::from_secs(2), server)
        .await
        .unwrap()
        .unwrap();
    let (spans, _) = read(directory.path(), &engine);
    let span = &spans["cancelled"];
    assert_eq!(span.names(), ["connection"]);
    let done = span.completion.as_ref().unwrap();
    assert_eq!(done.outcome, proto::InvocationOutcome::Cancelled as i32);
    assert!(!done.panicked);
}
