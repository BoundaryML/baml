// WASM integration tests for `baml.fs` and `baml.glob` operations.
//
// These tests verify that the `bridge_wasm` crate correctly wires up the
// fs and glob IO namespaces via mock JS objects under Node's WASM runtime.
//
// Run with:
//   cd baml_language/crates/bridge_wasm && wasm-pack test --node

use bridge_wasm::{
    BamlWasmRuntime, LspNotification,
    baml::cffi::{
        BamlOutboundValue, CallFunctionArgs, baml_outbound_value::Value as OutboundValue,
    },
};
use prost::Message;
use wasm_bindgen::{JsCast, prelude::*};
use wasm_bindgen_test::*;

wasm_bindgen_test_configure!(run_in_node_experimental);

/// Helper: build a mock VFS JS object with all required methods.
///
/// Implements an in-memory filesystem backed by a JS object so we can test
/// that the Rust WASM bridge correctly calls through to the JS VFS layer.
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

    // readDir(path) -> string[]
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readDir"),
        &js_sys::Function::new_with_args(
            "path",
            r#"
            var result = [];
            var files = this._files;
            var dirs = this._dirs;
            var prefix = path.endsWith('/') ? path : path + '/';
            var keys = Object.keys(files);
            for (var i = 0; i < keys.length; i++) {
                if (keys[i].startsWith(prefix)) {
                    var rest = keys[i].slice(prefix.length);
                    if (rest.indexOf('/') === -1) result.push(rest);
                }
            }
            for (var j = 0; j < dirs.length; j++) {
                if (dirs[j].startsWith(prefix)) {
                    var rest = dirs[j].slice(prefix.length);
                    if (rest.indexOf('/') === -1 && rest.length > 0) result.push(rest);
                }
            }
            return result;
            "#,
        ),
    )
    .unwrap();

    // createDir(path) -> void
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("createDir"),
        &js_sys::Function::new_with_args("path", "this._dirs.push(path);"),
    )
    .unwrap();

    // exists(path) -> boolean
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("exists"),
        &js_sys::Function::new_with_args(
            "path",
            r#"
            if (this._files.hasOwnProperty(path)) return true;
            for (var i = 0; i < this._dirs.length; i++) {
                if (this._dirs[i] === path) return true;
            }
            return false;
            "#,
        ),
    )
    .unwrap();

    // readFile(path) -> Uint8Array
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readFile"),
        &js_sys::Function::new_with_args(
            "path",
            r#"
            var content = this._files[path];
            if (content === undefined) throw new Error('File not found: ' + path);
            return new TextEncoder().encode(content);
            "#,
        ),
    )
    .unwrap();

    // writeFile(path, data) -> void
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("writeFile"),
        &js_sys::Function::new_with_args(
            "path, data",
            "this._files[path] = new TextDecoder().decode(data);",
        ),
    )
    .unwrap();

    // metadata(path) -> { fileType, len, ... }
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("metadata"),
        &js_sys::Function::new_with_args(
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
        ),
    )
    .unwrap();

    // removeFile(path) -> void
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("removeFile"),
        &js_sys::Function::new_with_args("path", "delete this._files[path];"),
    )
    .unwrap();

    // removeDir(path) -> void
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("removeDir"),
        &js_sys::Function::new_with_args(
            "path",
            "this._dirs = this._dirs.filter(function(d) { return d !== path; });",
        ),
    )
    .unwrap();

    // setTime, copyFile, moveFile, moveDir — stubs
    let noop2 = js_sys::Function::new_with_args("a, b", "");
    let noop3 = js_sys::Function::new_with_args("a, b, c", "");
    js_sys::Reflect::set(&obj, &JsValue::from_str("setTime"), &noop3).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("copyFile"), &noop2).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("moveFile"), &noop2).unwrap();
    js_sys::Reflect::set(&obj, &JsValue::from_str("moveDir"), &noop2).unwrap();

    // readMany(glob) -> [string, Uint8Array][]
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readMany"),
        &js_sys::Function::new_with_args(
            "glob",
            r#"
            function matches(path) {
                if (glob === '/workspace/baml_src/**/*.baml') {
                    return path.startsWith('/workspace/baml_src/') && path.endsWith('.baml');
                }
                if (glob === '/workspace/data/**/*.txt') {
                    return path.startsWith('/workspace/data/') && path.endsWith('.txt');
                }
                if (glob === '**/*.txt') return path.endsWith('.txt');
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
        ),
    )
    .unwrap();

    obj.into()
}

fn callbacks() -> JsValue {
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
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("playground_send_notification"),
        &noop,
    )
    .unwrap();

    obj.into()
}

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

fn runtime(files: &[(&str, &str)], dirs: &[&str], source: &str) -> BamlWasmRuntime {
    let vfs = mock_vfs(files, dirs);
    let callbacks = callbacks();
    let runtime = BamlWasmRuntime::create(callbacks.unchecked_ref(), vfs.unchecked_into()).unwrap();
    open_project(&runtime, source);
    runtime
}

async fn call_no_args(runtime: &BamlWasmRuntime, call_id: u32, name: &str) -> BamlOutboundValue {
    let args = CallFunctionArgs::default().encode_to_vec();
    let bytes = runtime
        .call_function(call_id, "/workspace/baml_src".to_string(), name, &args)
        .await
        .unwrap();
    BamlOutboundValue::decode(bytes.as_slice()).unwrap()
}

fn bool_value(value: BamlOutboundValue) -> bool {
    match value.value {
        Some(OutboundValue::BoolValue(v)) => v,
        other => panic!("expected bool result, got {other:?}"),
    }
}

fn string_list(value: BamlOutboundValue) -> Vec<String> {
    let Some(OutboundValue::ListValue(list)) = value.value else {
        panic!("expected list result, got {:?}", value.value);
    };
    list.items
        .into_iter()
        .map(|item| match item.value {
            Some(OutboundValue::StringValue(s)) => s,
            other => panic!("expected string list item, got {other:?}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Mock VFS unit tests — verify the JS mock itself behaves correctly.
// ---------------------------------------------------------------------------

#[wasm_bindgen_test]
fn mock_vfs_read_dir_returns_entries() {
    let vfs = mock_vfs(
        &[("/root/a.txt", "aaa"), ("/root/b.txt", "bbb")],
        &["/root", "/root/subdir"],
    );

    let read_dir_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("readDir"))
        .unwrap()
        .unchecked_into();
    let result = read_dir_fn
        .call1(&vfs, &JsValue::from_str("/root"))
        .unwrap();
    let arr: js_sys::Array = result.unchecked_into();

    let mut names: Vec<String> = arr.iter().map(|v| v.as_string().unwrap()).collect();
    names.sort();
    assert_eq!(names, vec!["a.txt", "b.txt", "subdir"]);
}

#[wasm_bindgen_test]
fn mock_vfs_exists_works() {
    let vfs = mock_vfs(&[("/root/file.txt", "content")], &["/root"]);

    let exists_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("exists"))
        .unwrap()
        .unchecked_into();

    assert_eq!(
        exists_fn
            .call1(&vfs, &JsValue::from_str("/root/file.txt"))
            .unwrap(),
        JsValue::TRUE
    );
    assert_eq!(
        exists_fn
            .call1(&vfs, &JsValue::from_str("/root/nope.txt"))
            .unwrap(),
        JsValue::FALSE
    );
    assert_eq!(
        exists_fn.call1(&vfs, &JsValue::from_str("/root")).unwrap(),
        JsValue::TRUE
    );
}

#[wasm_bindgen_test]
fn mock_vfs_read_write_roundtrip() {
    let vfs = mock_vfs(&[], &[]);

    let write_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("writeFile"))
        .unwrap()
        .unchecked_into();
    let data = js_sys::Uint8Array::from("hello world".as_bytes());
    let args = js_sys::Array::new();
    args.push(&JsValue::from_str("/test.txt"));
    args.push(&data);
    write_fn.apply(&vfs, &args).unwrap();

    let read_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("readFile"))
        .unwrap()
        .unchecked_into();
    let result = read_fn
        .call1(&vfs, &JsValue::from_str("/test.txt"))
        .unwrap();
    let arr: js_sys::Uint8Array = result.unchecked_into();
    assert_eq!(String::from_utf8(arr.to_vec()).unwrap(), "hello world");
}

#[wasm_bindgen_test]
fn mock_vfs_metadata_file_vs_dir() {
    let vfs = mock_vfs(&[("/root/file.txt", "hi")], &["/root"]);

    let meta_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("metadata"))
        .unwrap()
        .unchecked_into();

    let result = meta_fn
        .call1(&vfs, &JsValue::from_str("/root/file.txt"))
        .unwrap();
    assert_eq!(
        js_sys::Reflect::get(&result, &JsValue::from_str("fileType")).unwrap(),
        JsValue::from_str("file")
    );
    assert_eq!(
        js_sys::Reflect::get(&result, &JsValue::from_str("len")).unwrap(),
        JsValue::from_f64(2.0)
    );

    let result = meta_fn.call1(&vfs, &JsValue::from_str("/root")).unwrap();
    assert_eq!(
        js_sys::Reflect::get(&result, &JsValue::from_str("fileType")).unwrap(),
        JsValue::from_str("directory")
    );
}

#[wasm_bindgen_test]
fn mock_vfs_create_dir_then_exists() {
    let vfs = mock_vfs(&[], &[]);

    let create_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("createDir"))
        .unwrap()
        .unchecked_into();
    create_fn
        .call1(&vfs, &JsValue::from_str("/newdir"))
        .unwrap();

    let exists_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("exists"))
        .unwrap()
        .unchecked_into();
    assert_eq!(
        exists_fn
            .call1(&vfs, &JsValue::from_str("/newdir"))
            .unwrap(),
        JsValue::TRUE
    );
}

#[wasm_bindgen_test]
fn mock_vfs_read_many_returns_pairs() {
    let vfs = mock_vfs(&[("/root/a.txt", "aaa"), ("/root/b.rs", "bbb")], &[]);

    let read_many_fn: js_sys::Function = js_sys::Reflect::get(&vfs, &JsValue::from_str("readMany"))
        .unwrap()
        .unchecked_into();
    let result = read_many_fn
        .call1(&vfs, &JsValue::from_str("**/*.txt"))
        .unwrap();
    let arr: js_sys::Array = result.unchecked_into();

    assert!(arr.length() >= 1);
    let first_pair: js_sys::Array = arr.get(0).unchecked_into();
    assert!(first_pair.get(0).as_string().is_some());
    let data: js_sys::Uint8Array = first_pair.get(1).unchecked_into();
    assert!(data.length() > 0);
}

const FS_GLOB_SOURCE: &str = r#"
function MkdirRecursive() -> bool {
  baml.fs.mkdir("/generated/nested", baml.fs.MkdirOptions { recursive: true });
  baml.fs.exists("/generated/nested")
}

function ReadDirNames() -> string[] {
  let entries = baml.fs.read_dir("/workspace/data");
  entries.map((entry) -> { entry.name })
}

function GlobMatchesTxt() -> bool {
  let glob = baml.glob.new("**/*.txt");
  glob.matches("/workspace/data/a.txt") && !glob.matches("/workspace/data/b.rs")
}

function GlobScanTxt() -> string[] {
  let glob = baml.glob.new("/workspace/data/**/*.txt");
  glob.scan("/")
}
"#;

fn runtime_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("/workspace/baml_src/main.baml", FS_GLOB_SOURCE),
        ("/workspace/data/a.txt", "aaa"),
        ("/workspace/data/b.rs", "bbb"),
        ("/workspace/data/sub/c.txt", "ccc"),
    ]
}

fn runtime_dirs() -> Vec<&'static str> {
    vec![
        "/workspace",
        "/workspace/baml_src",
        "/workspace/data",
        "/workspace/data/sub",
    ]
}

#[wasm_bindgen_test]
async fn wasm_runtime_mkdir_recursive_then_exists() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 1, "MkdirRecursive").await;
    assert!(bool_value(result));
}

#[wasm_bindgen_test]
async fn wasm_runtime_read_dir_returns_entries() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 2, "ReadDirNames").await;
    let mut names = string_list(result);
    names.sort();
    assert_eq!(names, vec!["a.txt", "b.rs", "sub"]);
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_matches_paths() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 3, "GlobMatchesTxt").await;
    assert!(bool_value(result));
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_filters_paths() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 4, "GlobScanTxt").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(
        paths,
        vec!["/workspace/data/a.txt", "/workspace/data/sub/c.txt"]
    );
}
