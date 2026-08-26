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
use sys_ops::io::{self, BexExternalValue, CallId, SysOpContext, SysOpOutput, VmBamlError, owned};
use sys_types::BexHeap;

use crate::{send_wrapper::SendWrapper, wasm_vfs::WasmVfs};

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

fn downcast_glob_handle(glob: &owned::glob::Glob) -> Result<Arc<GlobHandle>, VmBamlError> {
    glob._handle
        .clone()
        .downcast::<GlobHandle>()
        .map_err(|_| VmBamlError::DevOther {
            message: "Invalid glob handle type".into(),
        })
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
            Err(e) => SysOpOutput::err(VmBamlError::InvalidArgument { message: e }),
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
            Err(e) => return SysOpOutput::err(VmBamlError::InvalidArgument { message: e }),
        };

        let mut scanned_paths = Vec::new();
        if let Err(e) = collect_scan_paths(
            self.vfs(),
            &scan_args.cwd,
            scan_args.dot,
            scan_args.only_files,
            &mut scanned_paths,
        ) {
            return SysOpOutput::err(VmBamlError::Io { message: e });
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
                cwd: normalize_cwd(cwd),
                dot: false,
                absolute: false,
                only_files: true,
            }),
            BexExternalValue::Instance { fields, .. } => {
                let cwd = get_string_field(fields, "cwd", ".")?;
                let dot = get_bool_field(fields, "dot", false)?;
                let absolute = get_bool_field(fields, "absolute", false)?;
                let only_files = get_bool_field(fields, "only_files", true)?;
                // The WASM bridge has no symlink concept (entries come from a
                // VFS that doesn't model symlinks). Reject explicit non-default
                // values for these options instead of silently no-opping, so
                // users don't think the option took effect.
                if get_bool_field(fields, "follow_symlinks", false)? {
                    return Err(
                        "ScanOptions.follow_symlinks is not supported in the WASM bridge".into(),
                    );
                }
                if get_bool_field(fields, "throw_error_on_broken_symlink", false)? {
                    return Err(
                        "ScanOptions.throw_error_on_broken_symlink is not supported in the WASM bridge"
                            .into(),
                    );
                }
                Ok(Self {
                    cwd: normalize_cwd(&cwd),
                    dot,
                    absolute,
                    only_files,
                })
            }
            _ => Err("scan argument must be a string or ScanOptions".into()),
        }
    }
}

/// WASM has no process-cwd concept, so a relative `cwd` like `.` or `""`
/// has no natural anchor. The pragmatic interpretation: treat them as the
/// VFS root (`/`). Without this, `Glob.scan(ScanOptions { cwd: ".", absolute:
/// true })` would silently return paths that aren't absolute, and the
/// underlying VFS calls (`readDir(".")`) would also miss any host that
/// serves paths under `/`.
fn normalize_cwd(cwd: &str) -> String {
    if cwd == "." || cwd.is_empty() {
        "/".to_string()
    } else {
        cwd.to_string()
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
    // Prefer the rich `readDirEntries` method so we get name + type info in
    // one JS round-trip per directory. Hosts that haven't implemented it
    // surface an error from the binding; in that case fall back to the
    // legacy readDir + per-entry metadata loop.
    if let Ok(entries) = vfs.vfs_read_dir_entries(path) {
        for v in entries.iter() {
            let entry: crate::wasm_vfs::WasmVfsDirEntry = serde_wasm_bindgen::from_value(v)
                .map_err(|e| format!("readDirEntries returned invalid entry: {e}"))?;
            if !dot && entry.name.starts_with('.') {
                continue;
            }
            let full_path = join_path(path, &entry.name);
            match entry.file_type.as_str() {
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
        return Ok(());
    }

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
        Some(value) => value.as_string().map(|s| s.to_string()).ok_or_else(|| {
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
        Some(value) => value.as_bool().ok_or_else(|| {
            format!(
                "ScanOptions.{key} must be a bool, got {}",
                value.type_name()
            )
        }),
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
        // Unreachable in practice — `ScanArgs::from_root` normalizes `.`
        // and `""` to `/`, so the only way we'd get here is a relative
        // cwd like `foo/bar` that the caller built directly. There's no
        // process cwd in WASM to resolve it against; anchor to the VFS
        // root rather than silently returning a non-absolute path.
        format!("/{path}")
    }
}
