//! Network operations.

use std::sync::Arc;

use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::{BexExternalValue, OpError, ResourceKind, SocketHandle};

/// Connect to a TCP address and return a resource.
///
/// Signature: `fn connect(addr: String) -> Socket`
pub async fn connect(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let addr = match args.into_iter().next() {
        Some(BexExternalValue::String(s)) => s,
        other => {
            return Err(OpError::TypeError {
                expected: "string address",
                actual: format!("{other:?}"),
            });
        }
    };

    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| OpError::Other(format!("Failed to connect to '{addr}': {e}")))?;

    let handle = SocketHandle::new(stream, addr);
    Ok(BexExternalValue::Resource(Arc::new(ResourceKind::Socket(
        handle,
    ))))
}

/// Read data from a socket.
///
/// Signature: `fn read(self: Socket) -> String`
pub async fn read(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let socket_arc = match args.into_iter().next() {
        Some(BexExternalValue::Resource(arc)) => arc,
        other => {
            return Err(OpError::TypeError {
                expected: "socket resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let ResourceKind::Socket(socket_handle) = socket_arc.as_ref() else {
        return Err(OpError::ResourceTypeMismatch { expected: "socket" });
    };

    let mut stream = socket_handle.stream.lock().await;
    let mut buffer = vec![0u8; 4096];
    let n = stream
        .read(&mut buffer)
        .await
        .map_err(|e| OpError::Other(format!("Failed to read from socket: {e}")))?;

    let contents = String::from_utf8_lossy(&buffer[..n]).into_owned();
    Ok(BexExternalValue::String(contents))
}

/// Closes a socket, releasing the resource.
///
/// Signature: `fn close(self: Socket)`
pub fn close(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let socket_arc = match args.into_iter().next() {
        Some(BexExternalValue::Resource(arc)) => arc,
        other => {
            return Err(OpError::TypeError {
                expected: "socket resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let ResourceKind::Socket(_) = socket_arc.as_ref() else {
        return Err(OpError::ResourceTypeMismatch { expected: "socket" });
    };

    drop(socket_arc);
    Ok(BexExternalValue::Null)
}
