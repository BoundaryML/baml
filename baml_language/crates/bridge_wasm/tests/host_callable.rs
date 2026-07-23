// WASM integration tests for host-callable dispatch.
//
// Exercises the JS-side host-callable bridge: `registerHostCallable` returns
// a key, `BamlHandle{key, HOST_VALUE_CALLABLE}` flows through the playground
// run argument envelope into the engine, and the BAML→host invocation
// round-trips via the JS `host_dispatch` callback + the wasm-exported
// `completeHostCall` symbol.
//
// The JS `host_dispatch` body in these tests is built from a Rust closure
// (wasm_bindgen::closure::Closure) so the helper code that decodes args,
// invokes the user callable, and forwards `completeHostCall` is plain Rust.
// Production bridges (e.g. typescript_src/proto.ts in Node) ship the
// equivalent code in pure TypeScript.
//
// Run with:
//   cd baml_language/crates/bridge_wasm && wasm-pack test --node -- host_callable

use base64::Engine;
use bridge_wasm::{
    BamlWasmRuntime, LspNotification,
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, BamlOutboundValue, BamlToHostCall, BamlTy, BamlTyClass,
        InboundClassValue, InboundMapEntry, InboundValue,
        baml_outbound_value::Value as OutboundValue, baml_ty::Ty as BamlTyVariant,
        inbound_map_entry::Key as MapKeyVariant, inbound_value::Value as InboundVariant,
    },
};
// Sentinel `_handle` key used in test fixtures that synthesize a
// `baml.errors.HostCallable` Instance without an actual JS exception
// object registered in any host-value table. The engine's structural
// check requires `_handle` to be present; same-host decoders look up
// `0`, find nothing, and fall back to metadata. `next_key()` mints
// from `1` upward (skipping `0`), so a real registered key can never
// collide with this sentinel.
const UNRESOLVED_HOST_ERROR_KEY: u64 = 0;
use prost::Message;
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_futures::JsFuture;
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_node_experimental);

// ---------------------------------------------------------------------------
// Mocks
// ---------------------------------------------------------------------------

fn mock_vfs(files: &[(&str, &str)], dirs: &[&str]) -> JsValue {
    let obj = js_sys::Object::new();
    let files_obj = js_sys::Object::new();
    for (path, content) in files {
        js_sys::Reflect::set(
            &files_obj,
            &JsValue::from_str(path),
            &JsValue::from_str(content),
        )
        .unwrap();
    }
    let dirs_arr = js_sys::Array::new();
    for d in dirs {
        dirs_arr.push(&JsValue::from_str(d));
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("_files"), &files_obj).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("_dirs"), &dirs_arr).unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("_symlinks"),
        &js_sys::Object::new(),
    )
    .unwrap();

    let read_file = js_sys::Function::new_with_args(
        "path",
        r#"
        var content = this._files[path];
        if (content === undefined) throw new Error('File not found: ' + path);
        return new TextEncoder().encode(content);
        "#,
    );
    js_sys::Reflect::set(&obj, &JsValue::from_str("readFile"), &read_file).unwrap();

    let exists = js_sys::Function::new_with_args(
        "path",
        r#"
        if (this._files.hasOwnProperty(path)) return true;
        for (var i = 0; i < this._dirs.length; i++) {
            if (this._dirs[i] === path) return true;
        }
        return false;
        "#,
    );
    js_sys::Reflect::set(&obj, &JsValue::from_str("exists"), &exists).unwrap();

    let read_many = js_sys::Function::new_with_args(
        "glob",
        r#"
        function matches(path) {
            if (glob === '/workspace/baml_src/**/*.baml') {
                return path.startsWith('/workspace/baml_src/') && path.endsWith('.baml');
            }
            return true;
        }
        var result = [];
        var keys = Object.keys(this._files);
        for (var i = 0; i < keys.length; i++) {
            if (!matches(keys[i])) continue;
            result.push([keys[i], new TextEncoder().encode(this._files[keys[i]])]);
        }
        return result;
        "#,
    );
    js_sys::Reflect::set(&obj, &JsValue::from_str("readMany"), &read_many).unwrap();

    let read_dir_entries = js_sys::Function::new_with_args(
        "path",
        r#"
        var result = [];
        var prefix = path.endsWith('/') ? path : path + '/';
        var keys = Object.keys(this._files);
        for (var i = 0; i < keys.length; i++) {
            if (keys[i].startsWith(prefix)) {
                var rest = keys[i].slice(prefix.length);
                if (rest.indexOf('/') === -1) {
                    result.push({ name: rest, file_type: 'file', is_symlink: false });
                }
            }
        }
        return result;
        "#,
    );
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readDirEntries"),
        &read_dir_entries,
    )
    .unwrap();

    let metadata = js_sys::Function::new_with_args(
        "path",
        r#"
        if (this._files.hasOwnProperty(path)) {
            var len = new TextEncoder().encode(this._files[path]).length;
            return { fileType: 'file', file_type: 'file', len: len };
        }
        for (var i = 0; i < this._dirs.length; i++) {
            if (this._dirs[i] === path) {
                return { fileType: 'directory', file_type: 'directory', len: 0 };
            }
        }
        throw new Error('Not found: ' + path);
        "#,
    );
    js_sys::Reflect::set(&obj, &JsValue::from_str("metadata"), &metadata).unwrap();

    let noop1 = js_sys::Function::new_with_args("a", "");
    let noop2 = js_sys::Function::new_with_args("a, b", "");
    let noop3 = js_sys::Function::new_with_args("a, b, c", "");
    js_sys::Reflect::set(&obj, &JsValue::from_str("readDir"), &noop1).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("createDir"), &noop1).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("writeFile"), &noop2).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("removeFile"), &noop1).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("removeDir"), &noop1).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("setTime"), &noop3).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("copyFile"), &noop2).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("moveFile"), &noop2).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("moveDir"), &noop2).unwrap();

    obj.into()
}

/// Build a callbacks object pointing `host_dispatch` at the given JS function
/// and collecting playground notifications into `notifications`.
fn callbacks_with_host_dispatch(
    host_dispatch: &js_sys::Function,
    notifications: &js_sys::Array,
) -> JsValue {
    let obj = js_sys::Object::new();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("fetch"),
        &js_sys::Function::new_with_args(
            "callId, method, url, headersJson, body",
            "return Promise.reject(new Error('fetch is not available in this test'));",
        ),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("env"),
        &js_sys::Function::new_with_args("variable", "return undefined;"),
    )
    .unwrap();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("input"),
        &js_sys::Function::new_with_args("callId, prompt", "return '';"),
    )
    .unwrap();
    let noop = js_sys::Function::new_with_args("value", "");
    js_sys::Reflect::set(&obj, &JsValue::from_str("lsp_send_notification"), &noop).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("lsp_send_response"), &noop).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("lsp_make_request"), &noop).unwrap();
    let captured_notifications = notifications.clone();
    let playground = Closure::wrap(Box::new(move |value: JsValue| {
        captured_notifications.push(&value);
    }) as Box<dyn FnMut(JsValue)>);
    let playground_fn = playground
        .as_ref()
        .unchecked_ref::<js_sys::Function>()
        .clone();
    playground.forget();
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("playground_send_notification"),
        &playground_fn,
    )
    .unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("host_dispatch"), host_dispatch).unwrap();
    obj.into()
}

/// Directories that must exist for `/workspace/baml_src` to resolve as a BAML
/// project root. `get_baml_project_root` walks up from the opened
/// `main.baml` and requires `baml_src` to report as a directory via the VFS
/// `metadata`/`exists` mocks, so the mock VFS must register these.
const PROJECT_DIRS: &[&str] = &["/workspace", "/workspace/baml_src"];

fn open_project(runtime: &BamlWasmRuntime, source: &str) {
    runtime.handle_notification(LspNotification {
        method: "textDocument/didOpen".to_string(),
        params: serde_json::json!({
            "textDocument": {
                "uri": "file:///workspace/baml_src/main.baml",
                "languageId": "baml",
                "version": 1,
                "text": source,
            }
        }),
    });
}

/// Build a runtime backed by the `CALLBACK_BAML` project and open it, wiring up
/// the given `host_dispatch` JS function. Mirrors the project setup used by the
/// passing `wasm_fs_glob.rs` tests: the mock VFS must register the project
/// directories so `/workspace/baml_src` resolves as a BAML project root.
fn runtime_with_dispatch(host_dispatch: &js_sys::Function) -> (BamlWasmRuntime, js_sys::Array) {
    let vfs = mock_vfs(
        &[("/workspace/baml_src/main.baml", CALLBACK_BAML)],
        PROJECT_DIRS,
    );
    let notifications = js_sys::Array::new();
    let callbacks = callbacks_with_host_dispatch(host_dispatch, &notifications);
    let runtime = BamlWasmRuntime::create(callbacks.unchecked_ref(), vfs.unchecked_into()).unwrap();
    open_project(&runtime, CALLBACK_BAML);
    (runtime, notifications)
}

// ---------------------------------------------------------------------------
// Args / result helpers
// ---------------------------------------------------------------------------

/// Build playground run argument bytes with a `callback` `HOST_VALUE` handle + `x`.
fn build_callback_args(callback_key: u64, x: i64) -> Vec<u8> {
    let cb_handle_value = InboundValue {
        value_type: None,
        value: Some(InboundVariant::Handle(BamlHandle {
            key: callback_key,
            handle_type: BamlHandleType::HostValueCallable as i32,
        })),
    };
    let x_value = InboundValue {
        value_type: None,
        value: Some(InboundVariant::IntValue(x)),
    };
    encode_run_args([
        InboundMapEntry {
            key: Some(MapKeyVariant::StringKey("callback".to_string())),
            value: Some(cb_handle_value),
        },
        InboundMapEntry {
            key: Some(MapKeyVariant::StringKey("x".to_string())),
            value: Some(x_value),
        },
    ])
}

fn encode_run_args(entries: impl IntoIterator<Item = InboundMapEntry>) -> Vec<u8> {
    let mut out = Vec::new();
    for entry in entries {
        entry
            .encode_length_delimited(&mut out)
            .expect("entry encodes");
    }
    out
}

async fn run_function(
    runtime: &BamlWasmRuntime,
    notifications: &js_sys::Array,
    call_id: u32,
    name: &str,
    args: &[u8],
) -> Result<BamlOutboundValue, JsValue> {
    let start_index = notifications.length();
    runtime.start_run(call_id, "/workspace/baml_src".to_string(), name, args)?;
    wait_for_run_result(notifications, start_index).await
}

async fn wait_for_run_result(
    notifications: &js_sys::Array,
    start_index: u32,
) -> Result<BamlOutboundValue, JsValue> {
    for _ in 0..100 {
        for idx in start_index..notifications.length() {
            let notification = notifications.get(idx);
            if let Some(outcome) = complete_outcome(&notification) {
                return decode_outcome(&outcome);
            }
        }
        JsFuture::from(js_sys::Promise::resolve(&JsValue::UNDEFINED)).await?;
    }
    Err(JsValue::from_str(
        "timed out waiting for RunStore completion notification",
    ))
}

fn complete_outcome(notification: &JsValue) -> Option<JsValue> {
    if get_string(notification, "type")? != "runPatch" {
        return None;
    }
    let patch = js_sys::Reflect::get(notification, &JsValue::from_str("patch")).ok()?;
    let changes = js_sys::Reflect::get(&patch, &JsValue::from_str("changes"))
        .ok()?
        .dyn_into::<js_sys::Array>()
        .ok()?;
    for idx in 0..changes.length() {
        let change = changes.get(idx);
        if get_string(&change, "type").as_deref() == Some("complete") {
            return js_sys::Reflect::get(&change, &JsValue::from_str("outcome")).ok();
        }
    }
    None
}

fn decode_outcome(outcome: &JsValue) -> Result<BamlOutboundValue, JsValue> {
    match get_string(outcome, "status").as_deref() {
        Some("succeeded") => {
            let result = js_sys::Reflect::get(outcome, &JsValue::from_str("result"))?;
            let encoded = get_string(&result, "value")
                .ok_or_else(|| JsValue::from_str("RunStore success missing result value"))?;
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .map_err(|e| JsValue::from_str(&format!("invalid base64 run result: {e}")))?;
            BamlOutboundValue::decode(bytes.as_slice())
                .map_err(|e| JsValue::from_str(&format!("invalid outbound run result: {e}")))
        }
        Some("failed") => {
            let error = js_sys::Reflect::get(outcome, &JsValue::from_str("error"))?;
            let message = get_string(&error, "message").unwrap_or_else(|| "run failed".to_string());
            Err(JsValue::from_str(&message))
        }
        Some(other) => Err(JsValue::from_str(&format!("run completed with {other}"))),
        None => Err(JsValue::from_str("RunStore completion missing status")),
    }
}

fn get_string(value: &JsValue, key: &str) -> Option<String> {
    js_sys::Reflect::get(value, &JsValue::from_str(key))
        .ok()
        .and_then(|value| value.as_string())
}

// ---------------------------------------------------------------------------
// Closure-backed `host_dispatch` helpers
// ---------------------------------------------------------------------------

/// Decode the first int arg's value from a `BamlToHostCall`.
fn decode_first_int(args_bytes: &[u8]) -> i64 {
    let to_host_call = BamlToHostCall::decode(args_bytes).expect("decode to-host call");
    match to_host_call
        .args
        .into_iter()
        .next()
        .and_then(|arg| arg.value)
    {
        Some(item) => match item.value {
            Some(OutboundValue::IntValue(i)) => i,
            other => panic!("expected first arg to be int, got {other:?}"),
        },
        None => panic!("empty args"),
    }
}

/// Build a `host_dispatch` JS function that doubles the first int arg and
/// completes the call with the resulting string. The returned [`Closure`] is
/// leaked (kept alive for the duration of the test) so the JS handle stays
/// valid.
fn make_dispatch_double_to_string() -> js_sys::Function {
    let closure = Closure::wrap(Box::new(
        move |_key: JsValue, call_id: f64, args: js_sys::Uint8Array| {
            let bytes = args.to_vec();
            let x = decode_first_int(&bytes);
            let s = format!("{}", x * 2);
            let inbound = InboundValue {
                value_type: None,
                value: Some(InboundVariant::StringValue(s)),
            };
            let payload = inbound.encode_to_vec();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "call ids are small monotonic counters"
            )]
            bridge_wasm::complete_host_call(call_id as u32, 0, &payload);
        },
    ) as Box<dyn FnMut(JsValue, f64, js_sys::Uint8Array)>);
    let func: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
    closure.forget();
    func
}

/// Build a `host_dispatch` JS function that multiplies the first int arg by
/// `factor` and completes the call with the resulting string. Used by the
/// two-runtime routing test to give each runtime a *distinct* wrapper so we
/// can tell whose dispatch actually ran. The returned [`Closure`] is leaked so
/// the JS handle stays valid for the test's duration.
fn make_dispatch_multiply_to_string(factor: i64) -> js_sys::Function {
    let closure = Closure::wrap(Box::new(
        move |_key: JsValue, call_id: f64, args: js_sys::Uint8Array| {
            let bytes = args.to_vec();
            let x = decode_first_int(&bytes);
            let s = format!("{}", x * factor);
            let inbound = InboundValue {
                value_type: None,
                value: Some(InboundVariant::StringValue(s)),
            };
            let payload = inbound.encode_to_vec();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "call ids are small monotonic counters"
            )]
            bridge_wasm::complete_host_call(call_id as u32, 0, &payload);
        },
    ) as Box<dyn FnMut(JsValue, f64, js_sys::Uint8Array)>);
    let func: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
    closure.forget();
    func
}

/// Build a `host_dispatch` JS function that always completes with a
/// thrown `baml.errors.HostCallable` Instance.
fn make_dispatch_error() -> js_sys::Function {
    let closure = Closure::wrap(Box::new(
        move |_key: JsValue, call_id: f64, _args: js_sys::Uint8Array| {
            fn field(key: &str, value: &str) -> InboundMapEntry {
                InboundMapEntry {
                    key: Some(MapKeyVariant::StringKey(key.to_string())),
                    value: Some(InboundValue {
                        value_type: None,
                        value: Some(InboundVariant::StringValue(value.to_string())),
                    }),
                }
            }
            let handle_field = InboundMapEntry {
                key: Some(MapKeyVariant::StringKey("_handle".to_string())),
                value: Some(InboundValue {
                    value_type: None,
                    value: Some(InboundVariant::Handle(BamlHandle {
                        key: UNRESOLVED_HOST_ERROR_KEY,
                        handle_type: BamlHandleType::HostValueOpaque as i32,
                    })),
                }),
            };
            let inbound = InboundValue {
                value_type: Some(BamlTy {
                    ty: Some(BamlTyVariant::ClassTy(BamlTyClass {
                        name: "baml.errors.HostCallable".to_string(),
                        type_args: vec![],
                    })),
                }),
                value: Some(InboundVariant::ClassValue(InboundClassValue {
                    fields: vec![
                        field("message", "test boom"),
                        field("class_name", "RuntimeError"),
                        field("language", "javascript"),
                        handle_field,
                    ],
                })),
            };
            let payload = inbound.encode_to_vec();
            #[expect(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "call ids are small monotonic counters"
            )]
            bridge_wasm::complete_host_call(call_id as u32, 1, &payload);
        },
    ) as Box<dyn FnMut(JsValue, f64, js_sys::Uint8Array)>);
    let func: js_sys::Function = closure.as_ref().unchecked_ref::<js_sys::Function>().clone();
    closure.forget();
    func
}

// ---------------------------------------------------------------------------
// BAML programs
// ---------------------------------------------------------------------------

const CALLBACK_BAML: &str = r#"
function CallCb(callback: (int) -> string, x: int) -> string {
    callback(x)
}
"#;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
async fn host_callable_round_trip_success() {
    let dispatch = make_dispatch_double_to_string();
    let (runtime, notifications) = runtime_with_dispatch(&dispatch);

    // Register a stub callable; the dispatch closure bypasses the registry
    // and decodes/completes the call directly. The engine still expects a
    // valid HostValue key in the inbound payload.
    let stub = js_sys::Function::new_with_args("x", "return x * 2");
    let key = bridge_wasm::register_host_callable(stub);

    let args = build_callback_args(key, 5);
    let outbound = run_function(&runtime, &notifications, 1, "CallCb", &args)
        .await
        .unwrap();

    match outbound.value {
        Some(OutboundValue::StringValue(s)) => assert_eq!(s, "10"),
        other => panic!("expected string result, got {other:?}"),
    }
}

#[wasm_bindgen_test]
async fn host_callable_error_surfaces_as_baml_error() {
    let dispatch = make_dispatch_error();
    let (runtime, notifications) = runtime_with_dispatch(&dispatch);

    let stub = js_sys::Function::new_with_args("x", "return ''");
    let key = bridge_wasm::register_host_callable(stub);

    let args = build_callback_args(key, 1);
    let result = run_function(&runtime, &notifications, 2, "CallCb", &args).await;
    assert!(
        result.is_err(),
        "expected error from throwing host callable, got success"
    );
}

/// Two runtimes, each given a DIFFERENT `host_dispatch` wrapper, must dispatch
/// host calls through *their own* wrapper — they must not clobber each other.
///
/// Because the `host_dispatch` callback lives on the per-runtime `WasmHost`
/// rather than a process-global slot, each runtime dispatches through the
/// wrapper it was created with regardless of creation order. Here runtime A
/// multiplies by 2 and runtime B multiplies by 3; A returns A's result and B
/// returns B's, in either order. If the callback were stored globally, the
/// runtime created *second* (B, ×3) would win for both and A would return "10"
/// instead of "6".
#[wasm_bindgen_test]
async fn two_runtimes_dispatch_through_their_own_wrapper() {
    // Runtime A doubles; runtime B triples. B is created *after* A — this is
    // exactly the ordering that triggered the old global-clobber bug.
    let dispatch_a = make_dispatch_multiply_to_string(2);
    let dispatch_b = make_dispatch_multiply_to_string(3);
    let (runtime_a, notifications_a) = runtime_with_dispatch(&dispatch_a);
    let (runtime_b, notifications_b) = runtime_with_dispatch(&dispatch_b);

    // Register a stub callable per runtime (the dispatch closures bypass the
    // registry and complete the call directly; the engine still needs a valid
    // HostValue key in the inbound payload).
    let key_a =
        bridge_wasm::register_host_callable(js_sys::Function::new_with_args("x", "return x * 2"));
    let key_b =
        bridge_wasm::register_host_callable(js_sys::Function::new_with_args("x", "return x * 3"));

    // Call on B first, then A, to make sure neither order leaks the other's
    // wrapper.
    let out_b = run_function(
        &runtime_b,
        &notifications_b,
        10,
        "CallCb",
        &build_callback_args(key_b, 5),
    )
    .await
    .unwrap();
    let out_a = run_function(
        &runtime_a,
        &notifications_a,
        11,
        "CallCb",
        &build_callback_args(key_a, 5),
    )
    .await
    .unwrap();

    match out_a.value {
        Some(OutboundValue::StringValue(s)) => {
            assert_eq!(
                s, "10",
                "runtime A must dispatch through its own (×2) wrapper"
            );
        }
        other => panic!("expected string result from runtime A, got {other:?}"),
    }
    match out_b.value {
        Some(OutboundValue::StringValue(s)) => {
            assert_eq!(
                s, "15",
                "runtime B must dispatch through its own (×3) wrapper"
            );
        }
        other => panic!("expected string result from runtime B, got {other:?}"),
    }
}
