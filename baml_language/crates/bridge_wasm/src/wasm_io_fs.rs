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
use sys_ops::io::{self, owned, BexExternalValue, CallId, OpErrorKind, SysOpContext, SysOpOutput};
use sys_types::BexHeap;

use crate::send_wrapper::SendWrapper;
use crate::wasm_fs::WasmVfs;

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
        let len = i64::try_from(data.len()).unwrap_or(i64::MAX);
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
        let len = i64::try_from(content.len()).unwrap_or(i64::MAX);
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
        match self.vfs().vfs_read_dir(&path) {
            Ok(arr) => {
                let mut entries = Vec::new();
                for v in arr.iter() {
                    let Some(name) = v.as_string() else { continue };
                    // WasmVfs.readDir returns string[] of entry names.
                    // Probe metadata to distinguish files from directories.
                    let full = format!("{path}/{name}");
                    let (is_dir, is_file) = match self.vfs().vfs_metadata(&full) {
                        Ok(meta) => (meta.file_type == "directory", meta.file_type == "file"),
                        Err(_) => (false, true),
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
        _options: owned::fs::MkdirOptions,
        _ctx: &SysOpContext,
    ) -> SysOpOutput<()> {
        // WasmVfs.createDir doesn't expose a recursive flag — the JS host
        // is expected to handle recursive creation transparently.
        match self.vfs().vfs_create_dir(&path) {
            Ok(()) => SysOpOutput::ok(()),
            Err(e) => SysOpOutput::err(OpErrorKind::Other(format!("{e:?}"))),
        }
    }
}
