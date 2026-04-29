//! WASM `baml.fs` namespace implementation via `WasmVfs` JS callbacks.
//!
//! `WasmIoFs` wraps an `Arc<WasmVfs>` (itself wrapped in `SendWrapper` to
//! satisfy `Send + Sync` bounds on single-threaded wasm32) and implements
//! `IoNamespaceFs` by delegating each operation to the corresponding JS method
//! on the VFS object passed at runtime creation.
//!
//! File handle operations (`IoClassFsFile`) are not supported in WASM — they
//! all return `Unsupported` because the native tokio file-handle model doesn't
//! translate to the browser sandbox.

use std::sync::Arc;

use js_sys::Uint8Array;
use sys_ops::io::{self, BexExternalValue, CallId, OpErrorKind, SysOpContext, SysOpOutput, owned};
use sys_types::BexHeap;

use crate::{send_wrapper::SendWrapper, wasm_fs::WasmVfs};

/// WASM implementation of `baml.fs` namespace ops.
///
/// Holds a shared `Arc<WasmVfs>` (created once in `BamlWasmRuntime::create`)
/// wrapped in `SendWrapper` to satisfy `Send + Sync` in the generated trait
/// bounds while keeping WASM's single-threaded model safe.
pub(crate) struct WasmIoFs {
    vfs: SendWrapper<Arc<WasmVfs>>,
}

impl WasmIoFs {
    /// Create a new `WasmIoFs` from a shared VFS reference.
    pub(crate) fn new(vfs: Arc<WasmVfs>) -> Self {
        Self {
            vfs: SendWrapper::new(vfs),
        }
    }

    fn vfs(&self) -> &WasmVfs {
        self.vfs.as_ref()
    }
}

fn join_path(parent: &str, name: &str) -> String {
    if parent == "/" {
        format!("/{name}")
    } else {
        format!("{}/{}", parent.trim_end_matches('/'), name)
    }
}

fn parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    let (parent, _) = trimmed.rsplit_once('/')?;
    if parent.is_empty() {
        Some("/".to_string())
    } else {
        Some(parent.to_string())
    }
}

fn ancestor_paths(path: &str) -> Vec<String> {
    let absolute = path.starts_with('/');
    let parts: Vec<&str> = path
        .trim_matches('/')
        .split('/')
        .filter(|part| !part.is_empty())
        .collect();
    let mut ancestors = Vec::new();
    for len in 1..parts.len() {
        let joined = parts[..len].join("/");
        ancestors.push(if absolute {
            format!("/{joined}")
        } else {
            joined
        });
    }
    ancestors
}

fn js_err(e: &wasm_bindgen::JsValue) -> OpErrorKind {
    OpErrorKind::Other(e.as_string().unwrap_or_else(|| format!("{e:?}")))
}

// ============================================================================
// IoClassFsFile — all File handle methods return Unsupported in WASM.
// The native tokio file-handle model (open/read/write/seek/close) cannot be
// mapped to the browser sandbox.
// ============================================================================

impl io::IoClassFsFile for WasmIoFs {
    fn text(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn seek_from(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _w: BexExternalValue,
        _o: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _d: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _d: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(OpErrorKind::Unsupported)
    }
}

// ============================================================================
// IoNamespaceFs — delegates to WasmVfs JS callbacks.
// ============================================================================

impl io::IoNamespaceFs for WasmIoFs {
    fn open(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: BexExternalValue,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<owned::fs::File> {
        // File handle operations not supported in WASM.
        SysOpOutput::err(OpErrorKind::Unsupported)
    }

    fn exists(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<bool> {
        match self.vfs().vfs_exists(&path) {
            Ok(v) => SysOpOutput::ok(v),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn remove(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        match self.vfs().vfs_remove_file(&path) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn size(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        match self.vfs().vfs_metadata(&path) {
            Ok(meta) => match i64::try_from(meta.len) {
                Ok(n) => SysOpOutput::ok(n),
                Err(_) => SysOpOutput::err(OpErrorKind::Other(format!(
                    "File '{path}' size exceeds i64::MAX"
                ))),
            },
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        match self.vfs().vfs_read_file(&path) {
            Ok(bytes) => match String::from_utf8(bytes.to_vec()) {
                Ok(s) => SysOpOutput::ok(s),
                Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("UTF-8 error: {e}"))),
            },
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        content: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let data = content.into_bytes();
        let Ok(len) = i64::try_from(data.len()) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "write size {} exceeds i64::MAX",
                data.len()
            )));
        };
        let uint8 = Uint8Array::from(data.as_slice());
        match self.vfs().vfs_write_file(&path, &uint8) {
            Ok(()) => SysOpOutput::ok(len),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        content: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        let Ok(len) = i64::try_from(content.len()) else {
            return SysOpOutput::err(OpErrorKind::Other(format!(
                "write size {} exceeds i64::MAX",
                content.len()
            )));
        };
        let uint8 = Uint8Array::from(content.as_slice());
        match self.vfs().vfs_write_file(&path, &uint8) {
            Ok(()) => SysOpOutput::ok(len),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn read_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<owned::fs::DirEntry>> {
        // Try the rich `readDirEntries` JS method first — one round-trip
        // returns names + types + symlink info for the whole directory. Hosts
        // that haven't implemented it yet error here, and we fall back to the
        // legacy `readDir` (string[]) path that probes `metadata` per entry.
        match self.vfs().vfs_read_dir_entries(&path) {
            Ok(arr) => {
                let mut entries = Vec::with_capacity(arr.length() as usize);
                for v in arr.iter() {
                    let entry: crate::wasm_fs::WasmVfsDirEntry =
                        match serde_wasm_bindgen::from_value(v) {
                            Ok(e) => e,
                            Err(e) => {
                                return SysOpOutput::err(OpErrorKind::Other(format!(
                                    "readDirEntries returned invalid entry: {e}"
                                )));
                            }
                        };
                    entries.push(owned::fs::DirEntry {
                        is_dir: entry.file_type == "directory",
                        is_file: entry.file_type == "file",
                        name: entry.name,
                        is_symlink: entry.is_symlink,
                    });
                }
                return SysOpOutput::ok(entries);
            }
            Err(_) => { /* fall through to legacy path */ }
        }

        match self.vfs().vfs_read_dir(&path) {
            Ok(arr) => {
                let mut entries = Vec::with_capacity(arr.length() as usize);
                for v in arr.iter() {
                    let Some(name) = v.as_string() else {
                        return SysOpOutput::err(OpErrorKind::Other(
                            "readDir entry is not a string".into(),
                        ));
                    };
                    // Legacy readDir doesn't expose type info. Probe metadata
                    // per entry. Hosts that care about read_dir performance
                    // should implement `readDirEntries`.
                    let full = join_path(&path, &name);
                    let (is_dir, is_file) = match self.vfs().vfs_metadata(&full) {
                        Ok(meta) => (meta.file_type == "directory", meta.file_type == "file"),
                        Err(e) => return SysOpOutput::err(js_err(&e)),
                    };
                    entries.push(owned::fs::DirEntry {
                        name,
                        is_dir,
                        is_file,
                        is_symlink: false,
                    });
                }
                SysOpOutput::ok(entries)
            }
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }

    fn mkdir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        options: owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        if !options.recursive {
            match self.vfs().vfs_exists(&path) {
                Ok(true) => {
                    return SysOpOutput::err(OpErrorKind::Other(format!(
                        "Directory already exists: {path}"
                    )));
                }
                Ok(false) => {}
                Err(e) => return SysOpOutput::err(js_err(&e)),
            }

            if let Some(parent) = parent_path(&path) {
                match self.vfs().vfs_exists(&parent) {
                    Ok(true) => {}
                    Ok(false) => {
                        return SysOpOutput::err(OpErrorKind::Other(format!(
                            "Parent directory does not exist: {parent}"
                        )));
                    }
                    Err(e) => return SysOpOutput::err(js_err(&e)),
                }
            }

            return match self.vfs().vfs_create_dir(&path) {
                Ok(()) => SysOpOutput::ok(()),
                Err(e) => SysOpOutput::err(js_err(&e)),
            };
        }

        match self.vfs().vfs_exists(&path) {
            Ok(true) => match self.vfs().vfs_metadata(&path) {
                Ok(meta) if meta.file_type == "directory" => return SysOpOutput::ok(()),
                Ok(_) => {
                    return SysOpOutput::err(OpErrorKind::Other(format!(
                        "Path exists and is not a directory: {path}"
                    )));
                }
                Err(e) => return SysOpOutput::err(js_err(&e)),
            },
            Ok(false) => {}
            Err(e) => return SysOpOutput::err(js_err(&e)),
        }

        for ancestor in ancestor_paths(&path) {
            match self.vfs().vfs_exists(&ancestor) {
                Ok(true) => continue,
                Ok(false) => {}
                Err(e) => return SysOpOutput::err(js_err(&e)),
            }
            if let Err(e) = self.vfs().vfs_create_dir(&ancestor) {
                return SysOpOutput::err(js_err(&e));
            }
        }

        match self.vfs().vfs_create_dir(&path) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => {
                // Idempotency: only swallow the create error if the path
                // already exists *as a directory* (e.g. an external concurrent
                // mutator beat us to it). If something else exists at that
                // path — a regular file, a symlink, anything non-directory —
                // this is a real failure and we propagate. The previous code
                // checked `vfs_exists` and returned Ok on any existing entry,
                // turning real `mkdir` failures into silent success when
                // something happened to occupy the path.
                //
                // Long-term: the JS VFS contract should grow `createDir(path,
                // { recursive: bool })` so the host handles this atomically
                // and we don't need this many round-trips at all.
                match self.vfs().vfs_metadata(&path) {
                    Ok(meta) if meta.file_type == "directory" => SysOpOutput::ok(()),
                    _ => SysOpOutput::err(js_err(&e)),
                }
            }
        }
    }
}
