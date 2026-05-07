// WASM integration tests for `baml.fs` and `baml.glob` operations.
//
// These tests verify that the `bridge_wasm` crate correctly wires up the
// fs and glob IO namespaces via mock JS objects under Node's WASM runtime.
//
// Run with:
//   cd baml_language/crates/bridge_wasm && wasm-pack test --node

use bridge_wasm::{
    BamlWasmRuntime, LspNotification,
    baml_core::cffi::{
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
    // Tests that need to mark specific paths as symlinks set
    // `vfs._symlinks[path] = true` after the mock is built. Default empty
    // so non-symlink tests see is_symlink=false everywhere.
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("_symlinks"),
        &js_sys::Object::new(),
    )
    .unwrap();

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

    // readDirEntries(path) -> Array<{ name, fileType, isSymlink }>
    // Rich form preferred by baml.fs.read_dir / baml.glob.scan; one
    // round-trip per directory instead of N+1.
    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("readDirEntries"),
        &js_sys::Function::new_with_args(
            "path",
            r#"
            var result = [];
            var files = this._files;
            var dirs = this._dirs;
            var symlinks = this._symlinks || {};
            var prefix = path.endsWith('/') ? path : path + '/';
            var keys = Object.keys(files);
            for (var i = 0; i < keys.length; i++) {
                if (keys[i].startsWith(prefix)) {
                    var rest = keys[i].slice(prefix.length);
                    if (rest.indexOf('/') === -1) {
                        result.push({
                            name: rest,
                            file_type: 'file',
                            is_symlink: !!symlinks[keys[i]],
                        });
                    }
                }
            }
            for (var j = 0; j < dirs.length; j++) {
                if (dirs[j].startsWith(prefix)) {
                    var rest2 = dirs[j].slice(prefix.length);
                    if (rest2.indexOf('/') === -1 && rest2.length > 0) {
                        result.push({
                            name: rest2,
                            file_type: 'directory',
                            is_symlink: !!symlinks[dirs[j]],
                        });
                    }
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
    runtime_with_symlinks(files, dirs, &[], source)
}

fn runtime_with_symlinks(
    files: &[(&str, &str)],
    dirs: &[&str],
    symlinks: &[&str],
    source: &str,
) -> BamlWasmRuntime {
    let vfs = mock_vfs(files, dirs);
    if !symlinks.is_empty() {
        let map = js_sys::Reflect::get(&vfs, &JsValue::from_str("_symlinks")).unwrap();
        for path in symlinks {
            js_sys::Reflect::set(&map, &JsValue::from_str(path), &JsValue::TRUE).unwrap();
        }
    }
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

async fn call_no_args_result(
    runtime: &BamlWasmRuntime,
    call_id: u32,
    name: &str,
) -> Result<BamlOutboundValue, JsValue> {
    let args = CallFunctionArgs::default().encode_to_vec();
    let bytes = runtime
        .call_function(call_id, "/workspace/baml_src".to_string(), name, &args)
        .await?;
    Ok(BamlOutboundValue::decode(bytes.as_slice()).unwrap())
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

const FS_GLOB_SOURCE: &str = r#"
function MkdirRecursive() -> bool {
  baml.fs.mkdir("/generated/nested", baml.fs.MkdirOptions { recursive: true });
  // Assert the parent was created too. Without this the test would still
  // pass if the bridge created only the leaf and silently dropped the
  // recursive parent-creation behavior.
  baml.fs.exists("/generated") && baml.fs.exists("/generated/nested")
}

function MkdirRecursiveExistingFile() -> null {
  baml.fs.mkdir("/workspace/data/a.txt", baml.fs.MkdirOptions { recursive: true })
}

function ReadDirNames() -> string[] {
  let entries = baml.fs.read_dir("/workspace/data");
  entries.map((entry) -> { entry.name })
}

function ReadDirSymlinkFlags() -> bool[] {
  let entries = baml.fs.read_dir("/workspace/data");
  entries.map((entry) -> { entry.is_symlink })
}

function ReadDirNamesAndSymlinks() -> string[] {
  let entries = baml.fs.read_dir("/workspace/data");
  entries.map((entry) -> {
    if (entry.is_symlink) { "L:" + entry.name } else { "F:" + entry.name }
  })
}

function GlobMatchesTxt() -> bool {
  let glob = baml.glob.new("**/*.txt");
  glob.matches("/workspace/data/a.txt") && !glob.matches("/workspace/data/b.rs")
}

function GlobScanTxt() -> string[] {
  let glob = baml.glob.new("**/*.txt");
  glob.scan("/workspace/data")
}

function GlobScanAbsoluteOptions() -> string[] {
  let glob = baml.glob.new("**/*.txt");
  glob.scan(baml.glob.ScanOptions { cwd: "/workspace/data", absolute: true })
}

function GlobScanDotCwdAbsolute() -> string[] {
  let glob = baml.glob.new("workspace/data/*.txt");
  glob.scan(baml.glob.ScanOptions { cwd: ".", absolute: true })
}

function GlobScanDotOptions() -> string[] {
  let glob = baml.glob.new("**/*.txt");
  glob.scan(baml.glob.ScanOptions { cwd: "/workspace/data", dot: true })
}

function GlobScanDirectoryOptions() -> string[] {
  let glob = baml.glob.new("**/*.txt");
  glob.scan(baml.glob.ScanOptions { cwd: "/workspace/data", only_files: false })
}
"#;

fn runtime_files() -> Vec<(&'static str, &'static str)> {
    vec![
        ("/workspace/baml_src/main.baml", FS_GLOB_SOURCE),
        ("/workspace/data/a.txt", "aaa"),
        ("/workspace/data/b.rs", "bbb"),
        ("/workspace/data/.hidden.txt", "hidden"),
        ("/workspace/data/sub/c.txt", "ccc"),
    ]
}

fn runtime_dirs() -> Vec<&'static str> {
    vec![
        "/workspace",
        "/workspace/baml_src",
        "/workspace/data",
        "/workspace/data/dir.txt",
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
async fn wasm_runtime_mkdir_recursive_errors_on_existing_file() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args_result(&runtime, 2, "MkdirRecursiveExistingFile").await;
    assert!(result.is_err());
}

#[wasm_bindgen_test]
async fn wasm_runtime_read_dir_returns_entries() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 3, "ReadDirNames").await;
    let mut names = string_list(result);
    names.sort();
    assert_eq!(
        names,
        vec![".hidden.txt", "a.txt", "b.rs", "dir.txt", "sub"]
    );
}

#[wasm_bindgen_test]
async fn wasm_runtime_read_dir_surfaces_symlink_flag() {
    // Mark `/workspace/data/a.txt` as a symlink in the mock VFS. The new
    // readDirEntries path returns the flag in one round-trip; baml.fs.read_dir
    // must propagate it through to the DirEntry.is_symlink field. Previously
    // is_symlink was hard-coded to false in the WASM bridge.
    let files = runtime_files();
    let dirs = runtime_dirs();
    let symlinks = vec!["/workspace/data/a.txt"];
    let runtime = runtime_with_symlinks(&files, &dirs, &symlinks, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 9, "ReadDirNamesAndSymlinks").await;
    let mut tagged = string_list(result);
    tagged.sort();
    // a.txt is the only entry tagged as a symlink.
    assert_eq!(
        tagged,
        vec!["F:.hidden.txt", "F:b.rs", "F:dir.txt", "F:sub", "L:a.txt",]
    );
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_matches_paths() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 4, "GlobMatchesTxt").await;
    assert!(bool_value(result));
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_filters_paths() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 5, "GlobScanTxt").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(paths, vec!["a.txt", "sub/c.txt"]);
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_absolute_options() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 6, "GlobScanAbsoluteOptions").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(
        paths,
        vec!["/workspace/data/a.txt", "/workspace/data/sub/c.txt"]
    );
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_dot_cwd_absolute() {
    // `cwd: "."` has no process-cwd in WASM; it must be normalized to the
    // VFS root so that `absolute: true` produces actual absolute paths
    // (previously this silently returned relative-looking strings).
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 10, "GlobScanDotCwdAbsolute").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(paths, vec!["/workspace/data/a.txt"]);
    for p in &paths {
        assert!(p.starts_with('/'), "expected absolute path, got: {p}");
    }
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_dot_options() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 7, "GlobScanDotOptions").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(paths, vec![".hidden.txt", "a.txt", "sub/c.txt"]);
}

#[wasm_bindgen_test]
async fn wasm_runtime_glob_scan_directory_options() {
    let files = runtime_files();
    let dirs = runtime_dirs();
    let runtime = runtime(&files, &dirs, FS_GLOB_SOURCE);

    let result = call_no_args(&runtime, 8, "GlobScanDirectoryOptions").await;
    let mut paths = string_list(result);
    paths.sort();
    assert_eq!(paths, vec!["a.txt", "dir.txt", "sub/c.txt"]);
}
