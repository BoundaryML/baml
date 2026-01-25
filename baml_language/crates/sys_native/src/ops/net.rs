//! Network operations.

use bex_external_types::BexExternalValue;
use sys_types::{OpError, SysOpResult};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::registry::REGISTRY;

/// Connect to a TCP address and return a resource.
///
/// Signature: `fn connect(addr: String) -> Socket`
pub(crate) fn connect(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(connect_async(args)))
}

async fn connect_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
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

    let handle = REGISTRY.register_socket(stream, addr);
    Ok(BexExternalValue::Resource(handle))
}

/// Read data from a socket.
///
/// Signature: `fn read(self: Socket) -> String`
pub(crate) fn read(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(read_async(args)))
}

async fn read_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "socket resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let stream_mutex = REGISTRY
        .get_socket(handle.key())
        .ok_or_else(|| OpError::Other("Socket handle is invalid or has been closed".into()))?;

    let mut stream = stream_mutex.lock().await;
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
pub(crate) fn close(args: Vec<BexExternalValue>) -> SysOpResult {
    let result = close_sync(args);
    SysOpResult::Ready(result)
}

fn close_sync(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    use sys_resource_types::ResourceType;

    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "socket resource",
                actual: format!("{other:?}"),
            });
        }
    };

    if handle.kind() != ResourceType::Socket {
        return Err(OpError::ResourceTypeMismatch { expected: "socket" });
    }

    // Resource closes when handle is dropped (cleanup callback removes from registry)
    drop(handle);
    Ok(BexExternalValue::Null)
}
