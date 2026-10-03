//! Every `baml.http` request is a span of its own: its request, the events of
//! its response, and how it ended, recorded by the runtime.
#![cfg(not(target_arch = "wasm32"))]

use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_reader::cas::{CasLimits, CasStore};
use btel_recorder::{RecordingConfig, proto};
use btel_snapshot::{DecodedObject, DecodedRoot, DecodedSnapshot, DecodedValue};
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
        let base = "http://" + canned.server.addr;

        let ok = baml.http.send(request(base, "/ok")).text();
        let bytes = baml.http.send(request(base, "/bytes")).bytes();
        let missing = baml.http.send(request(base, "/missing")).text();
        // The program sees the error as `reqwest` wrote it, raw URL included.
        let refused = {
            baml.http.send(request("http://127.0.0.1:1", "/refused"));
            "sent"
        } catch (e) {
            baml.errors.Io => if (e.message.includes("key=sk-query")) { "refused" } else { "redacted" },
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
            baml.errors.Io => if (e.message.includes("/Refused%20Path?key=sk-norm&q=a%20b")) { "normalized" } else { e.message },
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
        BexExternalValue::String(r#"{"ok":true}|not here|refused|normalized|cancelled"#.into())
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
        for event in file
            .spans
            .iter()
            .flat_map(|spans| &spans.sections)
            .flat_map(|section| &section.events)
        {
            let (id, minor) = match &event.event {
                Some(Event::NetworkAnnouncement(span)) => {
                    spans.entry(span.id).or_default().announcement = Some(span.clone());
                    (span.id, true)
                }
                Some(Event::NetworkEvent(network)) => {
                    let span = spans.entry(network.span_id).or_default();
                    span.events.push(network.clone());
                    span.written.push(network.name.clone());
                    (network.span_id, true)
                }
                Some(Event::NetworkCompletion(done)) => {
                    let span = spans.entry(done.span_id).or_default();
                    assert!(span.completion.is_none(), "a span ends once");
                    span.completion = Some(*done);
                    span.written.push("completion".to_owned());
                    (done.span_id, true)
                }
                _ => (0, false),
            };
            if minor {
                assert_ne!(id, 0);
                // Other additive features, such as process launch context, can
                // require a newer minor than network spans alone.
                assert!(
                    file.header.as_ref().unwrap().format_minor
                        >= btel_settings::encoding::NETWORK_FORMAT_MINOR
                );
            }
        }
    }
    // One span's events can be written by different VM threads, into
    // different sections: order them by time, as `baml query` does.
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

fn json(cas: &CasStore, snapshot: &DecodedSnapshot) -> Json {
    let DecodedRoot::Value(value) = &snapshot.root else {
        panic!("network snapshots are values")
    };
    value_json(cas, snapshot, value)
}

fn value_json(cas: &CasStore, snapshot: &DecodedSnapshot, value: &DecodedValue) -> Json {
    let entries = |entries: &btel_snapshot::Entries| {
        entries
            .iter()
            .map(|(key, value)| (key.to_string(), value_json(cas, snapshot, value)))
            .collect::<serde_json::Map<_, _>>()
            .into()
    };
    match value {
        DecodedValue::Null => Json::Null,
        DecodedValue::Int(value) => (*value).into(),
        DecodedValue::String(text) => text.as_ref().into(),
        DecodedValue::Object(id) => match snapshot.object(*id) {
            DecodedObject::Map { entries: map, .. } => entries(map),
            DecodedObject::Instance { fields, .. } => entries(fields),
            DecodedObject::Uint8Array { data, .. } => data.clone().into(),
            object => format!("{object:?}").into(),
        },
        // A long string, or a large part of a payload, is a blob of its own.
        DecodedValue::External(child) => json(cas, &blob(cas, snapshot.children[child.0 as usize])),
        value => format!("{value:?}").into(),
    }
}

fn blob(cas: &CasStore, id: btel_snapshot::CasId) -> std::sync::Arc<DecodedSnapshot> {
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
    let read = btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    (
        spans(&read.files),
        CasStore::new(directory.join("cas"), CasLimits::default()),
    )
}

fn load(cas: &CasStore, id: Option<proto::CasId>) -> Json {
    json(cas, &blob(cas, id.expect("captured").into()))
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
            "sse-close"
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
    assert_eq!(done.completed_at_ticks, span.event("await").at_ticks);

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
    // sees, quoting the sanitized URL where the program's quotes the raw one.
    let span = &spans["refused"];
    assert!(span.events.is_empty());
    assert_eq!(outcome(span), (proto::InvocationOutcome::Errored, false));
    let error = load(span.completion.as_ref().unwrap().error_cas_id);
    let message = error["error"]["message"].as_str().unwrap_or_default();
    assert!(message.contains("HTTP send failed"), "{error}");
    let sanitized = &span.announcement.as_ref().unwrap().url;
    assert!(message.contains(&format!("({sanitized})")), "{error}");

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

    // Never read: open for good, with what arrived.
    let span = &spans["never"];
    assert_eq!(span.names(), ["connection"]);
    assert!(span.completion.is_none());

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

/// `BAML_TELEMETRY_HTTP_BODIES=off` keeps bodies out of the recording; their
/// events are still there, with their times.
#[test]
fn bodies_off_records_events_without_bodies() {
    const CHILD: &str = "BAML_TEST_NETWORK_BODIES_OFF";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "bodies_off_records_events_without_bodies",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env(btel_settings::network::BODIES_ENV_VAR, "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let engine = record(directory.path()).await;
        let (spans, cas) = read(directory.path(), &engine);
        let span = &spans["ok"];
        let request = load(&cas, span.announcement.as_ref().unwrap().request_cas_id);
        assert_eq!(request["request"]["method"], "POST");
        assert!(request["request"].get("body").is_none());
        assert_eq!(span.names(), ["connection", "data", "await"]);
        assert_eq!(
            load(&cas, span.event("connection").payload_cas_id)["status"],
            200
        );
        assert!(span.event("data").payload_cas_id.is_none());
        let sse = &spans["sse"];
        assert_eq!(sse.names(), ["connection", "data", "data", "end", "await"]);
        assert!(
            sse.events[1..]
                .iter()
                .all(|event| event.payload_cas_id.is_none())
        );
    });
}
