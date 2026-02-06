//! Network operations.
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
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::registry::REGISTRY;

// ============================================================================
// Network Operations
// ============================================================================

/// Connect to a TCP address and return a resource.
///
/// Signature: `fn connect(addr: String) -> Socket`
pub(crate) fn connect(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::NetConnect, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let addr = match heap.with_gc_protection(move |protected| arg0.as_string(&protected).cloned()) {
        Ok(addr) => addr,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        connect_async(addr)
            .await
            .map_err(|e| OpError::new(SysOp::NetConnect, e))
    }))
}

async fn connect_async(addr: String) -> Result<BexExternalValue, OpErrorKind> {
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to connect to '{addr}': {e}")))?;

    let handle = REGISTRY.register_socket(stream, addr);
    let owned = builtin_types::owned::NetSocket { _handle: handle };
    Ok(owned.as_bex_external_value())
}

/// Read data from a socket.
///
/// Signature: `fn read(self: Socket) -> String`
pub(crate) fn read(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::NetRead, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let socket = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::NetSocket>(&protected)
            .and_then(|s| s.into_owned(&protected))
    }) {
        Ok(socket) => socket,
        Err(e) => return err(e.into()),
    };

    SysOpResult::Async(Box::pin(async move {
        read_async(socket)
            .await
            .map_err(|e| OpError::new(SysOp::NetRead, e))
    }))
}

async fn read_async(
    socket: builtin_types::owned::NetSocket,
) -> Result<BexExternalValue, OpErrorKind> {
    let stream_mutex = REGISTRY
        .get_socket(socket._handle.key())
        .ok_or_else(|| OpErrorKind::Other("Socket handle is invalid or has been closed".into()))?;

    let mut stream = stream_mutex.lock().await;
    let mut buffer = vec![0u8; 4096];
    let n = stream
        .read(&mut buffer)
        .await
        .map_err(|e| OpErrorKind::Other(format!("Failed to read from socket: {e}")))?;

    let contents = String::from_utf8_lossy(&buffer[..n]).into_owned();
    Ok(BexExternalValue::String(contents))
}

/// Closes a socket, releasing the resource.
///
/// Signature: `fn close(self: Socket)`
pub(crate) fn close(heap: &Arc<BexHeap>, mut args: Vec<bex_heap::BexValue<'_>>) -> SysOpResult {
    let err = |kind: OpErrorKind| SysOpResult::Ready(Err(OpError::new(SysOp::NetClose, kind)));

    if args.len() != 1 {
        return err(OpErrorKind::InvalidArgumentCount {
            expected: 1,
            actual: args.len(),
        });
    }

    let arg0 = args.remove(0);
    let socket = match heap.with_gc_protection(move |protected| {
        arg0.as_builtin_class::<builtin_types::NetSocket>(&protected)
            .and_then(|s| s.into_owned(&protected))
    }) {
        Ok(socket) => socket,
        Err(e) => return err(e.into()),
    };

    let result = close_sync(socket);
    SysOpResult::Ready(Ok(result))
}

fn close_sync(socket: builtin_types::owned::NetSocket) -> BexExternalValue {
    // This is a no-op for now since dropping the socket handle is the only way to close it.
    drop(socket);
    BexExternalValue::Null
}
