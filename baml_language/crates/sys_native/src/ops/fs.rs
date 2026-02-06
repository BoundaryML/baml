//! File system operations.
//!
//! # Safety
//! This module uses `unsafe` for GC-protected heap access. All unsafe blocks
//! are guarded by `with_gc_protection` which ensures heap stability.
#![allow(
    unsafe_code,
    clippy::needless_pass_by_value,
    clippy::match_wildcard_for_single_variants
)]

use std::sync::Arc;

use bex_heap::{BexHeap, builtin_types};
use sys_types::{BexExternalValue, OpError, OpErrorKind, SysOp, SysOpResult};
use tokio::{fs::File, io::AsyncReadExt};

use crate::registry::REGISTRY;

// ============================================================================
// File System Operations
// ============================================================================

/// Opens a file and returns a resource.
///
/// Signature: `fn open(path: String) -> File`
pub(crate) fn open(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::FsOpen, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);

    let path = match heap.with_gc_protection(move |protected| arg0.as_string(&protected).cloned()) {
        Ok(path) => path,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        open_async(path)
            .await
            .map_err(|e| OpError::new(SysOp::FsOpen, e))
    }))
}

async fn open_async(path: String) -> Result<BexExternalValue, OpErrorKind> {
    let file = File::open(&path)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to open file '{path}': {e}")))?;

    let handle = REGISTRY.register_file(file, path);
    let owned = builtin_types::owned::FsFile { _handle: handle };
    Ok(owned.as_bex_external_value())
}

/// Reads the contents of a file.
///
/// Signature: `fn read(self: File) -> String`
pub(crate) fn read(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::FsRead, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let file = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::FsFile>(&protected)
            .and_then(|file| file.into_owned(&protected))
    }) {
        Ok(file) => file,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        read_async(file)
            .await
            .map_err(|e| OpError::new(SysOp::FsRead, e))
    }))
}

async fn read_async(file: builtin_types::owned::FsFile) -> Result<BexExternalValue, OpErrorKind> {
    let file_mutex = REGISTRY
        .get_file(file._handle.key())
        .ok_or_else(|| OpErrorKind::Other("File handle is invalid or has been closed".into()))?;

    let mut file = file_mutex.lock().await;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to read file: {e}")))?;

    Ok(BexExternalValue::String(contents))
}

/// Closes a file, releasing the resource.
///
/// Signature: `fn close(self: File)`
pub(crate) fn close(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::FsClose, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let file = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::FsFile>(&protected)
            .and_then(|file| file.into_owned(&protected))
    }) {
        Ok(file) => file,
        Err(e) => return err(e.into()),
    };
    let result = close_sync(file);
    SysOpResult::Ready(Ok(result))
}

fn close_sync(file: builtin_types::owned::FsFile) -> BexExternalValue {
    // This is a no-op for now since dropping the file handle is the only way to close it.
    // we should implement a proper close operation in the future.
    drop(file);
    BexExternalValue::Null
}
