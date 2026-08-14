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
use sys_ops::io::{self, BexExternalValue, CallId, SysOpContext, SysOpOutput, VmBamlError, owned};
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

fn js_err(e: &wasm_bindgen::JsValue) -> VmBamlError {
    VmBamlError::Io {
        message: e.as_string().unwrap_or_else(|| format!("{e:?}")),
    }
}

/// A child entry discovered while walking a directory for recursive removal:
/// its full path plus whether it must be recursed into (a real subdirectory).
///
/// In the rich `readDirEntries` path a symlink-to-dir is reported as a file
/// (`removeFile`, never descended into), matching native `remove_dir_all`, which
/// does not follow symlinks. The legacy `readDir` fallback has no symlink info
/// (the JS VFS exposes no `lstat`), so there a symlink-to-dir is
/// indistinguishable from a real directory and would be descended into;
/// `remove_tree`'s depth cap bounds the resulting walk against symlink cycles.
struct RemoveChild {
    path: String,
    is_dir: bool,
}

/// List the immediate children of `path`, preferring the rich `readDirEntries`
/// JS method (one round-trip with type + symlink info) and falling back to the
/// legacy `readDir` + per-entry `metadata` probe when a host lacks it.
///
/// Malformed host payloads map to `Io` so `remove_dir_all` only ever surfaces
/// `root.errors.Io` — the error type it declares in `fs.baml`.
fn dir_children(vfs: &WasmVfs, path: &str) -> Result<Vec<RemoveChild>, VmBamlError> {
    if let Ok(arr) = vfs.vfs_read_dir_entries(path) {
        let mut out = Vec::with_capacity(arr.length() as usize);
        for v in arr.iter() {
            let entry: crate::wasm_fs::WasmVfsDirEntry = serde_wasm_bindgen::from_value(v)
                .map_err(|e| VmBamlError::Io {
                    message: format!("Invalid readDirEntries payload for '{path}': {e}"),
                })?;
            out.push(RemoveChild {
                path: join_path(path, &entry.name),
                is_dir: entry.file_type == "directory" && !entry.is_symlink,
            });
        }
        return Ok(out);
    }

    let arr = vfs.vfs_read_dir(path).map_err(|e| js_err(&e))?;
    let mut out = Vec::with_capacity(arr.length() as usize);
    for v in arr.iter() {
        let Some(name) = v.as_string() else {
            return Err(VmBamlError::Io {
                message: format!("Invalid readDir payload for '{path}': entry was not a string"),
            });
        };
        let full = join_path(path, &name);
        let is_dir = vfs.vfs_metadata(&full).map_err(|e| js_err(&e))?.file_type == "directory";
        out.push(RemoveChild { path: full, is_dir });
    }
    Ok(out)
}

/// Upper bound on recursion depth for `remove_tree`. Real directory trees never
/// approach this; it exists only to turn a symlink cycle reached via the legacy
/// (lstat-less) `readDir` fallback into a bounded `Io` error instead of a stack
/// overflow.
const MAX_REMOVE_DEPTH: u32 = 1024;

/// Recursively remove the directory `path` and everything beneath it. `path` is
/// assumed to already be a directory — the top-level entry point validates that.
fn remove_tree(vfs: &WasmVfs, path: &str, depth: u32) -> Result<(), VmBamlError> {
    if depth > MAX_REMOVE_DEPTH {
        return Err(VmBamlError::Io {
            message: format!(
                "Failed to remove directory '{path}': exceeded max recursion depth {MAX_REMOVE_DEPTH} (possible symlink cycle)"
            ),
        });
    }
    for child in dir_children(vfs, path)? {
        if child.is_dir {
            remove_tree(vfs, &child.path, depth + 1)?;
        } else {
            vfs.vfs_remove_file(&child.path).map_err(|e| js_err(&e))?;
        }
    }
    vfs.vfs_remove_dir(path).map_err(|e| js_err(&e))
}

/// Entry point for `remove_dir_all`. Idempotent on missing paths (`force: true`
/// semantics) and — like the native `tokio::fs::remove_dir_all`, which fails
/// with `NotADirectory` on a file — refuses a non-directory target, so
/// `remove_dir_all("file.txt")` can never silently delete a regular file on
/// WASM. (The native side also avoids following a top-level symlink; the JS VFS
/// has no `lstat`, so that narrow case can't be distinguished here.)
fn remove_dir_all_recursive(vfs: &WasmVfs, path: &str) -> Result<(), VmBamlError> {
    match vfs.vfs_exists(path) {
        Ok(false) => return Ok(()),
        Ok(true) => {}
        Err(e) => return Err(js_err(&e)),
    }
    if vfs.vfs_metadata(path).map_err(|e| js_err(&e))?.file_type != "directory" {
        return Err(VmBamlError::Io {
            message: format!("Failed to remove directory '{path}': not a directory"),
        });
    }
    remove_tree(vfs, path, 0)
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<String> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn read_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _n: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<Vec<u8>> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn close(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _d: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
    }

    fn write_bytes(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _f: owned::fs::File,
        _d: Vec<u8>,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<i64> {
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
        SysOpOutput::err(VmBamlError::Unsupported {
            message: "Operation not supported on this platform".to_string(),
        })
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
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
            Err(e) => {
                // Mirror native: when the target is a directory, point at the
                // directory-removal APIs instead of surfacing the raw host error
                // (the B-232 hint). The JS VFS has no lstat, so a symlink-to-dir
                // is reported as a directory here — an accepted approximation.
                if matches!(self.vfs().vfs_metadata(&path), Ok(m) if m.file_type == "directory") {
                    return SysOpOutput::err(VmBamlError::Io {
                        message: format!(
                            "Failed to remove '{path}': it is a directory; use baml.fs.remove_dir or baml.fs.remove_dir_all to delete directories"
                        ),
                    });
                }
                SysOpOutput::err(VmBamlError::Io {
                    message: format!("{e:?}"),
                })
            }
        }
    }

    fn remove_dir(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        match self.vfs().vfs_remove_dir(&path) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
        }
    }

    fn remove_dir_all(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // The JS VFS contract only exposes `removeDir` (empty-directory removal),
        // so the recursive walk is driven Rust-side, mirroring how `mkdir`
        // recursion is handled here.
        match remove_dir_all_recursive(self.vfs(), &path) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(e),
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
                Err(_) => SysOpOutput::err(VmBamlError::Io {
                    message: format!("File '{path}' size exceeds i64::MAX"),
                }),
            },
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
                Err(e) => SysOpOutput::err(VmBamlError::ParseError {
                    message: format!("UTF-8 error: {e}"),
                }),
            },
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
            return SysOpOutput::err(VmBamlError::InvalidArgument {
                message: format!("write size {} exceeds i64::MAX", data.len()),
            });
        };
        let uint8 = Uint8Array::from(data.as_slice());
        match self.vfs().vfs_write_file(&path, &uint8) {
            Ok(()) => SysOpOutput::ok(len),
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
            return SysOpOutput::err(VmBamlError::InvalidArgument {
                message: format!("write size {} exceeds i64::MAX", content.len()),
            });
        };
        let uint8 = Uint8Array::from(content.as_slice());
        match self.vfs().vfs_write_file(&path, &uint8) {
            Ok(()) => SysOpOutput::ok(len),
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
                                return SysOpOutput::err(VmBamlError::ParseError {
                                    message: format!("readDirEntries returned invalid entry: {e}"),
                                });
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
                        return SysOpOutput::err(VmBamlError::DevOther {
                            message: "readDir entry is not a string".into(),
                        });
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
            Err(e) => SysOpOutput::err(VmBamlError::Io {
                message: format!("{e:?}"),
            }),
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
                    return SysOpOutput::err(VmBamlError::Io {
                        message: format!("Directory already exists: {path}"),
                    });
                }
                Ok(false) => {}
                Err(e) => return SysOpOutput::err(js_err(&e)),
            }

            if let Some(parent) = parent_path(&path) {
                match self.vfs().vfs_exists(&parent) {
                    Ok(true) => {}
                    Ok(false) => {
                        return SysOpOutput::err(VmBamlError::Io {
                            message: format!("Parent directory does not exist: {parent}"),
                        });
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
                    return SysOpOutput::err(VmBamlError::Io {
                        message: format!("Path exists and is not a directory: {path}"),
                    });
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

    // The JS VFS contract exposes neither permissions nor links, and both are
    // reported as `Io` rather than `Unsupported` because that is the only
    // category these two sysops declare — an `Unsupported` would escape every
    // typed `catch` arm the caller can write. Growing the VFS with `chmod` /
    // `symlink` methods is what would make these real here.
    fn chmod(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _path: String,
        _mode: i64,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "File permissions are not supported by the JavaScript filesystem".to_string(),
        })
    }

    fn symlink(
        &self,
        _h: &Arc<BexHeap>,
        _c: CallId,
        _target: String,
        _path: String,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        SysOpOutput::err(VmBamlError::Io {
            message: "Symbolic links are not supported by the JavaScript filesystem".to_string(),
        })
    }
}
