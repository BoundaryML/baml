//! WASM `baml.glob` namespace implementation via `WasmVfs` JS callbacks.
//!
//! `WasmIoGlob` implements `IoNamespaceGlob` and `IoClassGlobGlob` for WASM:
//!
//! - `new(pattern)` — compiles the pattern via `sys_glob::GlobPattern` and
//!   stores the resulting compiled regex in the glob handle so subsequent
//!   `scan` and `matches` calls reuse it without recompilation.
//! - `Glob.scan(root)` — walks the VFS via `WasmVfs.readDir` and filters with
//!   the compiled `GlobPattern` from the handle.
//! - `Glob.matches(path)` — pure-Rust glob matching via the compiled
//!   `GlobPattern` from the handle.

use std::sync::Arc;

use sys_glob::GlobPattern;
use sys_ops::io::{self, BexExternalValue, CallId, OpErrorKind, SysOpContext, SysOpOutput, owned};
use sys_types::BexHeap;

use crate::{send_wrapper::SendWrapper, wasm_fs::WasmVfs};

/// WASM implementation of `baml.glob` namespace + `Glob` class.
pub(crate) struct WasmIoGlob {
    vfs: SendWrapper<Arc<WasmVfs>>,
}

impl WasmIoGlob {
    /// Create a new `WasmIoGlob` from a shared VFS reference.
    pub(crate) fn new(vfs: Arc<WasmVfs>) -> Self {
        Self {
            vfs: SendWrapper::new(vfs),
        }
    }

    fn vfs(&self) -> &WasmVfs {
        self.vfs.as_ref()
    }
}

// ============================================================================
// IoNamespaceGlob — `baml.glob.new(pattern)` creates a Glob handle.
// ============================================================================

type GlobHandle = GlobPattern;

fn downcast_glob_handle(glob: &owned::glob::Glob) -> Result<Arc<GlobHandle>, OpErrorKind> {
    glob._handle
        .clone()
        .downcast::<GlobHandle>()
        .map_err(|_| OpErrorKind::Other("Invalid glob handle type".into()))
}

impl io::IoNamespaceGlob for WasmIoGlob {
    fn new(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        pattern: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::glob::Glob> {
        match GlobPattern::new(&pattern) {
            Ok(compiled) => {
                let handle: Arc<dyn std::any::Any + Send + Sync> = Arc::new(compiled);
                SysOpOutput::ok(owned::glob::Glob { _handle: handle })
            }
            Err(e) => SysOpOutput::err(OpErrorKind::Other(e)),
        }
    }
}

// ============================================================================
// IoClassGlobGlob — `Glob.scan` and `Glob.matches` method implementations.
// ============================================================================

impl io::IoClassGlobGlob for WasmIoGlob {
    fn scan(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        glob: owned::glob::Glob,
        root: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<String>> {
        let compiled = match downcast_glob_handle(&glob) {
            Ok(c) => c,
            Err(e) => return SysOpOutput::err(e),
        };
        let scan_args = match ScanArgs::from_root(&root) {
            Ok(args) => args,
            Err(e) => return SysOpOutput::err(OpErrorKind::Other(e)),
        };

        let mut scanned_paths = Vec::new();
        if let Err(e) = collect_scan_paths(
            self.vfs(),
            &scan_args.cwd,
            scan_args.dot,
            scan_args.only_files,
            &mut scanned_paths,
        ) {
            return SysOpOutput::err(OpErrorKind::Other(e));
        }

        let mut paths = Vec::new();
        for path in scanned_paths {
            let Some(rel_path) = relative_to_root(&path, &scan_args.cwd) else {
                continue;
            };
            if rel_path.is_empty() {
                continue;
            }
            if !scan_args.dot && rel_path.split('/').any(|seg| seg.starts_with('.')) {
                continue;
            }

            if !compiled.is_match_entry(&rel_path, &path) {
                continue;
            }

            if scan_args.absolute {
                paths.push(absolute_path(&scan_args.cwd, &path));
            } else {
                paths.push(rel_path);
            }
        }
        SysOpOutput::ok(paths)
    }

    fn matches(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        glob: owned::glob::Glob,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        match downcast_glob_handle(&glob) {
            Ok(compiled) => SysOpOutput::ok(compiled.is_match(&path)),
            Err(e) => SysOpOutput::err(e),
        }
    }
}

struct ScanArgs {
    cwd: String,
    dot: bool,
    absolute: bool,
    only_files: bool,
}

impl ScanArgs {
    fn from_root(root: &BexExternalValue) -> Result<Self, String> {
        match root {
            BexExternalValue::String(cwd) => Ok(Self {
                cwd: cwd.clone(),
                dot: false,
                absolute: false,
                only_files: true,
            }),
            BexExternalValue::Instance { fields, .. } => {
                let cwd = get_string_field(fields, "cwd", ".")?;
                let dot = get_bool_field(fields, "dot", false)?;
                let absolute = get_bool_field(fields, "absolute", false)?;
                let only_files = get_bool_field(fields, "only_files", true)?;
                let _follow_symlinks = get_bool_field(fields, "follow_symlinks", false)?;
                let _throw_on_broken =
                    get_bool_field(fields, "throw_error_on_broken_symlink", false)?;
                Ok(Self {
                    cwd,
                    dot,
                    absolute,
                    only_files,
                })
            }
            _ => Err("scan argument must be a string or ScanOptions".into()),
        }
    }
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn js_err(e: &wasm_bindgen::JsValue) -> String {
    e.as_string().unwrap_or_else(|| format!("{e:?}"))
}

fn collect_scan_paths(
    vfs: &WasmVfs,
    path: &str,
    dot: bool,
    only_files: bool,
    out: &mut Vec<String>,
) -> Result<(), String> {
    let entries = vfs.vfs_read_dir(path).map_err(|e| js_err(&e))?;
    for entry in entries.iter() {
        let name = entry
            .as_string()
            .ok_or_else(|| "readDir entry is not a string".to_string())?;
        if !dot && name.starts_with('.') {
            continue;
        }

        let full_path = join_path(path, &name);
        let meta = vfs.vfs_metadata(&full_path).map_err(|e| js_err(&e))?;
        match meta.file_type.as_str() {
            "file" => out.push(full_path),
            "directory" => {
                if !only_files {
                    out.push(full_path.clone());
                }
                collect_scan_paths(vfs, &full_path, dot, only_files, out)?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn get_string_field(
    fields: &indexmap::IndexMap<String, BexExternalValue>,
    key: &str,
    default: &str,
) -> Result<String, String> {
    match fields.get(key) {
        None | Some(BexExternalValue::Null) => Ok(default.to_string()),
        Some(value) => external_as_string(value).ok_or_else(|| {
            format!(
                "ScanOptions.{key} must be a string, got {}",
                value.type_name()
            )
        }),
    }
}

fn get_bool_field(
    fields: &indexmap::IndexMap<String, BexExternalValue>,
    key: &str,
    default: bool,
) -> Result<bool, String> {
    match fields.get(key) {
        None | Some(BexExternalValue::Null) => Ok(default),
        Some(value) => external_as_bool(value).ok_or_else(|| {
            format!(
                "ScanOptions.{key} must be a bool, got {}",
                value.type_name()
            )
        }),
    }
}

fn external_as_string(value: &BexExternalValue) -> Option<String> {
    match value {
        BexExternalValue::String(value) => Some(value.clone()),
        BexExternalValue::Union { value, .. } => external_as_string(value),
        _ => None,
    }
}

fn external_as_bool(value: &BexExternalValue) -> Option<bool> {
    match value {
        BexExternalValue::Bool(value) => Some(*value),
        BexExternalValue::Union { value, .. } => external_as_bool(value),
        _ => None,
    }
}

fn normalize_path(path: &str) -> String {
    let path = path.replace('\\', "/");
    if path == "/" {
        path
    } else {
        path.trim_end_matches('/').to_string()
    }
}

fn relative_to_root(path: &str, root: &str) -> Option<String> {
    let path = normalize_path(path);
    let root = normalize_path(root);

    if root == "." || root.is_empty() {
        return Some(path.strip_prefix("./").unwrap_or(&path).to_string());
    }
    if root == "/" {
        return Some(path.strip_prefix('/').unwrap_or(&path).to_string());
    }
    if path == root {
        return Some(String::new());
    }

    let prefix = format!("{root}/");
    path.strip_prefix(&prefix).map(ToString::to_string)
}

fn absolute_path(root: &str, path: &str) -> String {
    let path = normalize_path(path);
    if path.starts_with('/') {
        return path;
    }

    let root = normalize_path(root);
    if root.starts_with('/') {
        if root == "/" {
            format!("/{path}")
        } else {
            format!("{root}/{path}")
        }
    } else {
        path
    }
}
