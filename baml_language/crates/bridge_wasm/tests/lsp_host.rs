//! The browser language server, end to end under Node's wasm runtime.
//!
//! Run with:
//!   `cd baml_language/crates/bridge_wasm && wasm-pack test --node`
//!
//! This is the only place the wasm host is *executed* rather than merely
//! compiled, and the distinction matters: `cargo check --target
//! wasm32-unknown-unknown` happily accepts code that traps on the first call
//! (`std::time::Instant::now` is "not implemented on this platform" there,
//! which used to take the tab down on the first edit).

use bridge_wasm::{BamlWasmRuntime, LspNotification, LspRequest};
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::wasm_bindgen_test;

/// A minimal host filesystem: one project, one source file.
const VFS_SOURCE: &str = r#"(() => {
  const files = new Map([
    ["/proj/baml.toml", "[package]\nname = \"web_proj\"\n"],
    ["/proj/baml_src/main.baml", "/// Adds two numbers.\nfunction add(a: int, b: int) -> int {\n    a + b\n}\n\nfunction answer() -> int {\n    add(40, 2)\n}\n\ntest \"adds\" {\n    assert.is_true(add(2, 3) == 5)\n}\n"],
  ]);
  const dirs = new Set(["/proj", "/proj/baml_src"]);
  const encoder = new TextEncoder();
  const children = (dir) => {
    const prefix = dir.endsWith("/") ? dir : dir + "/";
    const seen = new Set();
    const out = [];
    for (const key of [...files.keys(), ...dirs]) {
      if (!key.startsWith(prefix) || key === dir) continue;
      const name = key.slice(prefix.length).split("/")[0];
      if (seen.has(name)) continue;
      seen.add(name);
      out.push({ name, file_type: dirs.has(prefix + name) ? "directory" : "file", is_symlink: false });
    }
    return out;
  };
  return {
    readDir: (p) => children(p).map((c) => c.name),
    readDirEntries: children,
    createDir: () => {},
    exists: (p) => files.has(p) || dirs.has(p),
    readFile: (p) => {
      if (!files.has(p)) throw new Error("ENOENT " + p);
      return encoder.encode(files.get(p));
    },
    writeFile: () => {},
    metadata: (p) => {
      if (files.has(p)) return { file_type: "file", len: files.get(p).length };
      if (dirs.has(p)) return { file_type: "directory", len: 0 };
      throw new Error("ENOENT " + p);
    },
    removeFile: () => {},
    removeDir: () => {},
    setTime: () => {},
    copyFile: () => {},
    moveFile: () => {},
    moveDir: () => {},
    readMany: () => [],
  };
})()"#;

/// Callbacks that record what the server sent, so a test can read it back.
/// The platform callbacks are inert: these tests exercise analysis, not the
/// operations the runtime delegates to the host.
const CALLBACKS_SOURCE: &str = r#"(() => {
  const notifications = [];
  const playground = [];
  const responses = new Map();
  return {
    fetch: () => {},
    env: () => {},
    input: () => {},
    exec: () => {},
    shell: () => {},
    host_dispatch: () => {},
    lsp_send_notification: (n) => notifications.push(n),
    lsp_send_response: (r) => responses.set(String(r.id), r),
    playground_send_notification: (n) => playground.push(n),
    takeNotifications: (method) => JSON.stringify(notifications.filter((n) => n.method === method)),
    takePlayground: (type) => JSON.stringify(playground.filter((n) => n.type === type)),
    response: (id) => JSON.stringify(responses.get(String(id)) ?? null),
  };
})()"#;

const MAIN_URI: &str = "file:///proj/baml_src/main.baml";

struct Host {
    runtime: BamlWasmRuntime,
    callbacks: js_sys::Object,
}

impl Host {
    fn start() -> Self {
        let vfs = js_sys::eval(VFS_SOURCE).expect("the mock filesystem evaluates");
        let callbacks: js_sys::Object = js_sys::eval(CALLBACKS_SOURCE)
            .expect("the callbacks evaluate")
            .unchecked_into();
        let runtime = BamlWasmRuntime::create(callbacks.unchecked_ref(), vfs.unchecked_into());
        Self { runtime, callbacks }
    }

    fn call_js(&self, name: &str, argument: &JsValue) -> String {
        let function: js_sys::Function = js_sys::Reflect::get(&self.callbacks, &name.into())
            .expect("the recorder exposes its readers")
            .unchecked_into();
        function
            .call1(&JsValue::NULL, argument)
            .expect("the recorder answers")
            .as_string()
            .expect("the recorder answers with JSON")
    }

    fn request(&self, id: i32, method: &str, params: serde_json::Value) -> serde_json::Value {
        let request = LspRequest {
            id: lsp_server::RequestId::from(id),
            method: method.to_owned(),
            params,
        };
        self.runtime.handle_lsp_request(request);
        let response = self.call_js("response", &JsValue::from_f64(f64::from(id)));
        serde_json::from_str(&response).expect("the response is JSON")
    }

    fn notify(&self, method: &str, params: serde_json::Value) {
        self.runtime.handle_lsp_notification(LspNotification {
            method: method.to_owned(),
            params,
        });
    }

    /// Playground notifications of one type, in order.
    fn playground(&self, kind: &str) -> Vec<serde_json::Value> {
        let raw = self.call_js("takePlayground", &JsValue::from_str(kind));
        serde_json::from_str(&raw).expect("playground notifications are JSON")
    }

    fn published(&self) -> Vec<serde_json::Value> {
        let raw = self.call_js(
            "takeNotifications",
            &JsValue::from_str("textDocument/publishDiagnostics"),
        );
        serde_json::from_str(&raw).expect("publications are JSON")
    }

    /// Bring a session up against the mock project.
    fn initialized() -> Self {
        let host = Self::start();
        let result = host.request(
            1,
            "initialize",
            serde_json::json!({
                "processId": null,
                "capabilities": { "general": { "positionEncodings": ["utf-16"] } },
                "workspaceFolders": [{ "uri": "file:///proj", "name": "proj" }],
            }),
        );
        assert!(
            result["error"].is_null(),
            "initialize failed: {}",
            result["error"]
        );
        assert_eq!(result["result"]["serverInfo"]["name"], "baml-lsp");
        host.notify("initialized", serde_json::json!({}));
        host
    }
}

/// Discovery reads the host filesystem, the project checks clean, and the
/// diagnostics tail fires — all inline on the one thread, with no timer to
/// wake it later.
#[wasm_bindgen_test]
fn a_browser_session_discovers_its_project_and_publishes_diagnostics() {
    let host = Host::initialized();
    let published = host.published();
    assert_eq!(published.len(), 1, "one publication, got: {published:?}");
    assert_eq!(published[0]["params"]["uri"], MAIN_URI);
    assert_eq!(
        published[0]["params"]["diagnostics"]
            .as_array()
            .map(Vec::len),
        Some(0),
        "the fixture is clean"
    );
}

/// The analysis layer answers in the browser exactly as it does natively:
/// a resolved signature under its owning package, with the doc comment.
#[wasm_bindgen_test]
fn hover_answers_from_the_browser_database() {
    let host = Host::initialized();
    let response = host.request(
        2,
        "textDocument/hover",
        serde_json::json!({
            "textDocument": { "uri": MAIN_URI },
            "position": { "line": 1, "character": 10 },
        }),
    );
    let markdown = response["result"]["contents"]["value"]
        .as_str()
        .unwrap_or_default();
    assert!(
        markdown.contains("function add(a: int, b: int) -> int throws never"),
        "resolved signature, got: {markdown}"
    );
    assert!(
        markdown.contains("Adds two numbers."),
        "docs render, got: {markdown}"
    );
}

/// An editor buffer wins over the host filesystem, and the edit's diagnostics
/// reach the client without any further prompting.
#[wasm_bindgen_test]
fn an_edit_republishes_diagnostics_from_the_overlay() {
    let host = Host::initialized();
    host.notify(
        "textDocument/didOpen",
        serde_json::json!({
            "textDocument": {
                "uri": MAIN_URI,
                "languageId": "baml",
                "version": 1,
                "text": "function add(a: int, b: int) -> int {\n    a + b\n}\n",
            },
        }),
    );
    host.notify(
        "textDocument/didChange",
        serde_json::json!({
            "textDocument": { "uri": MAIN_URI, "version": 2 },
            "contentChanges": [{ "text": "function add(a: int, b: int) -> int {\n    \"nope\"\n}\n" }],
        }),
    );
    let published = host.published();
    let latest = published.last().expect("an edit publishes");
    let diagnostics = latest["params"]["diagnostics"]
        .as_array()
        .expect("diagnostics array");
    assert_eq!(diagnostics.len(), 1, "got: {diagnostics:?}");
    assert!(
        diagnostics[0]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("mismatched types"),
        "got: {}",
        diagnostics[0]["message"]
    );
    // The publication carries the edit's version, so the client can tell it
    // apart from the one for the text it replaced.
    assert_eq!(latest["params"]["version"], 2);
}

/// Yield to the event loop so `spawn_local` work (engine calls) can run.
async fn tick() {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        let global: js_sys::Object = js_sys::global().unchecked_into();
        let set_timeout: js_sys::Function = js_sys::Reflect::get(&global, &"setTimeout".into())
            .expect("the host has setTimeout")
            .unchecked_into();
        set_timeout
            .call2(&JsValue::NULL, &resolve, &JsValue::from_f64(0.0))
            .expect("setTimeout schedules");
    });
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .expect("the timeout resolves");
}

/// Await a playground notification of `kind`, or give up.
async fn await_playground(host: &Host, kind: &str) -> Vec<serde_json::Value> {
    for _ in 0..200 {
        let seen = host.playground(kind);
        if !seen.is_empty() {
            return seen;
        }
        tick().await;
    }
    panic!("no `{kind}` notification arrived");
}

/// The graph view's two synchronous questions, answered from the database.
#[wasm_bindgen_test]
fn the_graph_view_gets_a_control_flow_graph_and_cursor_context() {
    let host = Host::initialized();
    host.runtime
        .request_control_flow_graph("/proj", "add", Some(7));
    let graphs = host.playground("controlFlowGraphResult");
    assert_eq!(graphs.len(), 1);
    assert_eq!(graphs[0]["functionName"], "add");
    assert_eq!(graphs[0]["requestId"], 7);
    // Nodes are keyed by id, and the root is the workspace `add` — not the
    // stdlib function of the same name that shares this database.
    let nodes = graphs[0]["graph"]["nodes"]
        .as_object()
        .expect("a real graph, got: {graph}");
    let root = nodes.values().next().expect("at least one node");
    assert_eq!(root["label"], "add");
    assert_eq!(
        root["sourceSpan"]["filePath"], "/proj/baml_src/main.baml",
        "the workspace function, got: {root}"
    );

    // Inside `add`'s body.
    host.runtime.handle_cursor_position(MAIN_URI, 2, 6);
    let contexts = host.playground("cursorContext");
    assert_eq!(contexts.len(), 1);
    assert_eq!(contexts[0]["context"]["functionName"], "add");

    // One runtime hosts one workspace: a request naming another project is a
    // host bug, and answering it with this workspace's data would be worse
    // than answering nothing.
    host.runtime
        .request_control_flow_graph("/somewhere-else", "add", Some(8));
    assert_eq!(
        host.playground("controlFlowGraphResult").len(),
        1,
        "the request for another project went unanswered"
    );
}

/// Test collection runs against the engine the source built, and the tree
/// comes back under that engine's generation.
#[wasm_bindgen_test]
async fn tests_are_collected_from_the_installed_engine() {
    let host = Host::initialized();
    host.runtime.request_collect_tests("/proj");
    let results = await_playground(&host, "testCollectionResult").await;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results[0]["generation"], 1,
        "the first build's engine collected it"
    );
    let tree: serde_json::Value = serde_json::from_slice(
        &results[0]["data"]
            .as_array()
            .expect("the tree is a byte array")
            .iter()
            .map(|byte| u8::try_from(byte.as_u64().unwrap_or_default()).unwrap_or_default())
            .collect::<Vec<u8>>(),
    )
    .expect("the tree is JSON");
    assert_eq!(
        tree[0]["name"], "root::adds",
        "the fixture's test, got: {tree}"
    );
}

/// A run goes all the way through: the engine the browser built from its own
/// filesystem executes the function, and the run store's frames reach the host
/// on the playground callback.
#[wasm_bindgen_test]
async fn a_function_run_executes_and_completes() {
    let host = Host::initialized();
    // No arguments, so the run-args payload is empty.
    host.runtime
        .start_run(11, "/proj".to_owned(), "answer", &[])
        .expect("the engine is current, so the run starts");

    let started = await_playground(&host, "runStarted").await;
    assert_eq!(started[0]["requestId"], 11);
    assert_eq!(started[0]["run"]["target"]["functionName"], "answer");
    let boundary_id = started[0]["run"]["boundaryId"]
        .as_str()
        .expect("the run has an identity")
        .to_owned();

    // Patches arrive until the run reaches a terminal status.
    for _ in 0..400 {
        let terminal = host.playground("runPatch").into_iter().find(|frame| {
            frame["patch"]["boundaryId"] == serde_json::json!(boundary_id)
                && frame["patch"]["changes"].as_array().is_some_and(|changes| {
                    changes.iter().any(|change| {
                        change["type"] == "setStatus" && change["status"] != "running"
                    })
                })
        });
        if let Some(frame) = terminal {
            let status = frame["patch"]["changes"]
                .as_array()
                .and_then(|changes| {
                    changes
                        .iter()
                        .find(|change| change["type"] == "setStatus")
                        .and_then(|change| change["status"].as_str())
                })
                .unwrap_or_default()
                .to_owned();
            assert_eq!(status, "succeeded", "got frame: {frame}");
            return;
        }
        tick().await;
    }
    panic!("the run never reached a terminal status");
}
