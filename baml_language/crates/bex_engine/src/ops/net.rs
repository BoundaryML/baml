//! Network operations.
//!
//! Implements `baml.net.connect` and `baml.net.Socket.read`.

use std::sync::Arc;

use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::{OpContext, OpError, ResolvedArgs, ResolvedValue};

/// A socket handle stored in the resource registry.
struct SocketHandle {
    /// The stream, wrapped in Arc for cloning out of the resource registry.
    stream: Arc<tokio::sync::Mutex<TcpStream>>,
    #[allow(dead_code)]
    addr: String,
}

impl SocketHandle {
    fn new(stream: TcpStream, addr: String) -> Self {
        Self {
            stream: Arc::new(tokio::sync::Mutex::new(stream)),
            addr,
        }
    }
}

// ============================================================================
// baml.net.connect
// ============================================================================

/// Connect to a TCP address and return a resource ID.
///
/// Signature: `fn connect(addr: String) -> Socket`
pub async fn connect(ctx: Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the address argument
    let addr = match args.args.into_iter().next() {
        Some(ResolvedValue::String(s)) => s,
        other => {
            let msg = format!("Expected string address argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Connect to the address
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| OpError::Other(format!("Failed to connect to '{addr}': {e}")))?;

    // Store in resources and return the ID
    let handle = SocketHandle::new(stream, addr);
    let id = ctx.add_resource(handle);

    Ok(ResolvedValue::ResourceId(id))
}

// ============================================================================
// baml.net.Socket.read
// ============================================================================

/// Read data from a socket.
///
/// Signature: `fn read(self: Socket) -> String`
pub async fn read(ctx: Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the socket resource ID from the first argument
    let socket_id = match args.args.into_iter().next() {
        Some(ResolvedValue::Int(id)) => id.cast_unsigned(),
        Some(ResolvedValue::ResourceId(id)) => id,
        other => {
            let msg = format!("Expected socket resource ID as first argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Get the socket handle from resources
    // Clone the Arc<Mutex<TcpStream>> so we can release the RwLock before awaiting
    let stream_mutex = {
        let guard = ctx.resources.read();
        let socket_handle = guard
            .get::<SocketHandle>(socket_id)
            .ok_or(OpError::ResourceNotFound(socket_id))?;
        Arc::clone(&socket_handle.stream)
    }; // RwLock guard dropped here

    // Read available data from the socket
    let mut stream = stream_mutex.lock().await;
    let mut buffer = vec![0u8; 4096];
    let n = stream
        .read(&mut buffer)
        .await
        .map_err(|e| OpError::Other(format!("Failed to read from socket: {e}")))?;

    // Convert to string (lossy for non-UTF8 data)
    let contents = String::from_utf8_lossy(&buffer[..n]).into_owned();
    Ok(ResolvedValue::String(contents))
}
