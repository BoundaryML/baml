//! In-process end-to-end transcript through the real runtime: the owner
//! thread, the read pool, and the ingress scheduler are all live; only the
//! stdio pipe is replaced by a capturing sink.

use std::{
    path::Path,
    sync::Arc,
    time::{Duration, Instant},
};

use baml_lsp::{
    GlobalState,
    discovery::NativeFs,
    executor::{Executors, ThreadPool},
};
use baml_lsp_server::{
    lsp_ingress::TransportKind,
    lsp_runtime::{LspRuntime, Sink, SinkDelivery, SubmitResult},
};
use lsp_server::{Message, Notification, Request, RequestId};
use lsp_types::Url;
use parking_lot::Mutex;

/// Generous on purpose: every wait below is for *behavior*, and the first
/// one covers a cold type-check of the whole stdlib in a debug build, which
/// is ~20s on a developer machine. The budget exists to fail a hang, not to
/// measure speed.
const WAIT: Duration = Duration::from_secs(60);

/// The transcript's live wire: everything the server sent, in order.
#[derive(Clone, Default)]
struct Wire {
    messages: Arc<Mutex<Vec<Message>>>,
}

impl Wire {
    fn sink(&self) -> Sink {
        let messages = Arc::clone(&self.messages);
        Arc::new(move |message| {
            messages.lock().push(message);
            SinkDelivery::Sent
        })
    }

    fn snapshot(&self) -> Vec<Message> {
        self.messages.lock().clone()
    }

    /// Block until `pick` finds something in the transcript, or fail loudly
    /// with the whole transcript.
    fn wait_for<T>(&self, what: &str, pick: impl Fn(&[Message]) -> Option<T>) -> T {
        let started = Instant::now();
        loop {
            if let Some(found) = pick(&self.snapshot()) {
                return found;
            }
            assert!(
                started.elapsed() < WAIT,
                "timed out waiting for {what}; transcript so far:\n{}",
                self.render()
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    fn render(&self) -> String {
        self.snapshot()
            .iter()
            .map(|message| match message {
                Message::Request(request) => {
                    format!("-> request {} {}", request.id, request.method)
                }
                Message::Response(response) => format!(
                    "<- response {} ok={} err={:?}",
                    response.id,
                    response.result.is_some(),
                    response.error.as_ref().map(|e| (e.code, e.message.clone()))
                ),
                Message::Notification(notification) => {
                    format!(
                        "<- notification {} {}",
                        notification.method, notification.params
                    )
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn response(&self, id: i32) -> lsp_server::Response {
        self.wait_for(&format!("response {id}"), |messages| {
            messages.iter().find_map(|message| match message {
                Message::Response(response) if response.id == RequestId::from(id) => {
                    Some(response.clone())
                }
                Message::Response(_) | Message::Request(_) | Message::Notification(_) => None,
            })
        })
    }

    /// The newest `publishDiagnostics` for `uri` whose diagnostics satisfy
    /// `accept`.
    fn diagnostics(
        &self,
        what: &str,
        uri: &Url,
        accept: impl Fn(&[lsp_types::Diagnostic]) -> bool,
    ) -> lsp_types::PublishDiagnosticsParams {
        self.wait_for(what, |messages| {
            messages.iter().rev().find_map(|message| match message {
                Message::Notification(notification)
                    if notification.method == "textDocument/publishDiagnostics" =>
                {
                    let params: lsp_types::PublishDiagnosticsParams =
                        serde_json::from_value(notification.params.clone()).ok()?;
                    (params.uri == *uri && accept(&params.diagnostics)).then_some(params)
                }
                Message::Notification(_) | Message::Request(_) | Message::Response(_) => None,
            })
        })
    }
}

fn request(id: i32, method: &str, params: serde_json::Value) -> Message {
    Message::Request(Request {
        id: RequestId::from(id),
        method: method.to_string(),
        params,
    })
}

fn notification(method: &str, params: serde_json::Value) -> Message {
    Message::Notification(Notification::new(method.to_string(), params))
}

fn file_uri(path: &Path) -> Url {
    Url::from_file_path(path).expect("absolute path")
}

const VALID: &str = "class Point {\n    x int\n    y int\n}\n";
const BROKEN: &str = "class Broken {\n    v NoSuchType\n}\n";
const FIXED: &str = "class Broken {\n    v int\n}\n";

#[test]
fn stdio_transcript_end_to_end() {
    let temp = tempfile::tempdir().unwrap();
    let workspace = temp.path().canonicalize().unwrap();
    let src = workspace.join("baml_src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(src.join("main.baml"), VALID).unwrap();
    let broken_path = src.join("broken.baml");
    std::fs::write(&broken_path, BROKEN).unwrap();
    let broken_uri = file_uri(&broken_path);

    let executors = Executors::single(Arc::new(ThreadPool::new(2)));
    let state = GlobalState::with_fs(executors, None, Arc::new(NativeFs));
    let runtime = LspRuntime::new(state).unwrap();

    let wire = Wire::default();
    let session = runtime
        .open_session(TransportKind::Stdio, wire.sink(), Arc::new(|| {}), None)
        .session_id;
    let submit = |message: Message| {
        let started = Instant::now();
        loop {
            match runtime.submit(session, message.clone()) {
                SubmitResult::Accepted => return SubmitResult::Accepted,
                SubmitResult::Backpressure => {
                    assert!(started.elapsed() < WAIT, "backpressure never cleared");
                    std::thread::sleep(Duration::from_millis(2));
                }
                other => return other,
            }
        }
    };

    // initialize → initialized
    submit(request(
        1,
        "initialize",
        serde_json::json!({
            "processId": null,
            "capabilities": {
                "general": { "positionEncodings": ["utf-16"] },
            },
            "workspaceFolders": [
                { "uri": file_uri(&workspace), "name": "ws" }
            ],
        }),
    ));
    let init = wire.response(1);
    assert!(init.error.is_none(), "initialize failed: {:?}", init.error);
    assert!(
        init.result
            .as_ref()
            .and_then(|r| r.get("capabilities"))
            .is_some(),
        "initialize result carries capabilities: {init:?}"
    );
    submit(notification("initialized", serde_json::json!({})));

    // didOpen the broken file → a non-empty publish for it.
    submit(notification(
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": broken_uri,
                "languageId": "baml",
                "version": 1,
                "text": BROKEN,
            }
        }),
    ));
    let published = wire.diagnostics(
        "diagnostics for the broken file",
        &broken_uri,
        |diagnostics| !diagnostics.is_empty(),
    );
    assert_eq!(
        published.version,
        Some(1),
        "publication echoes the document version"
    );
    assert!(
        published
            .diagnostics
            .iter()
            .any(|d| d.severity == Some(lsp_types::DiagnosticSeverity::ERROR)),
        "the unresolved type is an error: {published:?}"
    );

    // didChange fixing the error → an empty publish for it.
    submit(notification(
        "textDocument/didChange",
        serde_json::json!({
            "textDocument": { "uri": broken_uri, "version": 2 },
            "contentChanges": [ { "text": FIXED } ],
        }),
    ));
    let cleared = wire.diagnostics(
        "empty diagnostics after the fix",
        &broken_uri,
        <[lsp_types::Diagnostic]>::is_empty,
    );
    assert_eq!(cleared.version, Some(2));

    // formatting → a response (a document already formatted yields no edits).
    submit(request(
        2,
        "textDocument/formatting",
        serde_json::json!({
            "textDocument": { "uri": broken_uri },
            "options": { "tabSize": 4, "insertSpaces": true },
        }),
    ));
    let formatted = wire.response(2);
    assert!(
        formatted.error.is_none(),
        "formatting failed: {:?}",
        formatted.error
    );

    // shutdown → exit
    submit(request(3, "shutdown", serde_json::Value::Null));
    let shutdown = wire.response(3);
    assert!(
        shutdown.error.is_none(),
        "shutdown failed: {:?}",
        shutdown.error
    );
    assert!(matches!(
        submit(notification("exit", serde_json::Value::Null)),
        SubmitResult::Exited { normal: true }
    ));

    // Responses arrived in request order, and nothing was logged to the
    // client at ERROR level.
    let transcript = wire.snapshot();
    let response_ids: Vec<RequestId> = transcript
        .iter()
        .filter_map(|message| match message {
            Message::Response(response) => Some(response.id.clone()),
            Message::Request(_) | Message::Notification(_) => None,
        })
        .collect();
    assert_eq!(
        response_ids,
        vec![RequestId::from(1), RequestId::from(2), RequestId::from(3)],
        "{}",
        wire.render()
    );
    let error_logs: Vec<&Message> = transcript
        .iter()
        .filter(|message| match message {
            Message::Notification(notification) => {
                notification.method == "window/logMessage"
                    && notification
                        .params
                        .get("type")
                        .and_then(|value| serde_json::from_value(value.clone()).ok())
                        == Some(lsp_types::MessageType::ERROR)
            }
            Message::Request(_) | Message::Response(_) => false,
        })
        .collect();
    assert!(
        error_logs.is_empty(),
        "no ERROR logMessage may reach the client: {error_logs:?}"
    );

    runtime.close_session(session);
}
