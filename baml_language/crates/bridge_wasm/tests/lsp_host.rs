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
    ["/proj/baml_src/main.baml", "/// Adds two numbers.\nfunction add(a: int, b: int) -> int {\n    a + b\n}\n"],
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
const CALLBACKS_SOURCE: &str = r#"(() => {
  const notifications = [];
  const responses = new Map();
  return {
    lsp_send_notification: (n) => notifications.push(n),
    lsp_send_response: (r) => responses.set(String(r.id), r),
    takeNotifications: (method) => JSON.stringify(notifications.filter((n) => n.method === method)),
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
