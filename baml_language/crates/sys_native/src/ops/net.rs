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

use bex_heap::BexHeap;
use bex_vm_types::{Object, Value};
use sys_resource_types::ResourceHandle;
use sys_types::{BexExternalValue, BexValue, OpError, SysOpResult};
use tokio::{io::AsyncReadExt, net::TcpStream};

use crate::registry::REGISTRY;

// ============================================================================
// Wrapper Types for GC-Safe Instance Access
// ============================================================================

/// Wrapper for Socket instance that provides GC-safe access to fields.
struct SocketRef {
    /// The resource handle extracted from the `_handle` field.
    resource_handle: ResourceHandle,
}

impl SocketRef {
    /// Extract a `SocketRef` from a `BexValue` that should be a Socket instance.
    fn from_value(heap: &Arc<BexHeap>, value: &BexValue) -> Result<Self, OpError> {
        match value {
            BexValue::Opaque(handle) => {
                // Access heap object with GC protection
                heap.with_gc_protection(|protected| {
                    let ptr = protected
                        .resolve_handle(handle.slab_key())
                        .ok_or_else(|| OpError::Other("invalid handle".into()))?;

                    let obj = unsafe { ptr.get() };
                    let Object::Instance(inst) = obj else {
                        return Err(OpError::TypeError {
                            expected: "Socket instance",
                            actual: format!("{obj:?}"),
                        });
                    };

                    let Object::Class(class) = (unsafe { inst.class.get() }) else {
                        return Err(OpError::Other("bad class ptr".into()));
                    };

                    if class.name != "baml.net.Socket" {
                        return Err(OpError::TypeError {
                            expected: "Socket instance",
                            actual: format!("Instance of {}", class.name),
                        });
                    }

                    let idx = class
                        .fields
                        .iter()
                        .position(|f| f.name == "_handle")
                        .ok_or_else(|| OpError::Other("missing _handle field".into()))?;

                    match inst.fields.get(idx) {
                        Some(Value::Object(h)) => match unsafe { h.get() } {
                            Object::Resource(rh) => Ok(Self {
                                resource_handle: rh.clone(),
                            }),
                            other => Err(OpError::TypeError {
                                expected: "Resource",
                                actual: format!("{other:?}"),
                            }),
                        },
                        other => Err(OpError::Other(format!("invalid _handle field: {other:?}"))),
                    }
                })
            }
            BexValue::External(BexExternalValue::Instance { class_name, fields }) => {
                if class_name != "baml.net.Socket" {
                    return Err(OpError::TypeError {
                        expected: "Socket instance",
                        actual: format!("Instance of {class_name}"),
                    });
                }
                match fields.get("_handle") {
                    Some(BexExternalValue::Resource(h)) => Ok(Self {
                        resource_handle: h.clone(),
                    }),
                    _ => Err(OpError::TypeError {
                        expected: "Resource in _handle field",
                        actual: "missing or invalid _handle".to_string(),
                    }),
                }
            }
            other => Err(OpError::TypeError {
                expected: "Socket instance",
                actual: format!("{other:?}"),
            }),
        }
    }

    fn key(&self) -> usize {
        self.resource_handle.key()
    }

    fn kind(&self) -> sys_resource_types::ResourceType {
        self.resource_handle.kind()
    }
}

// ============================================================================
// Network Operations
// ============================================================================

/// Connect to a TCP address and return a resource.
///
/// Signature: `fn connect(addr: String) -> Socket`
pub(crate) fn connect(_heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let addr = match extract_string(args.first()) {
        Ok(a) => a,
        Err(e) => return SysOpResult::Ready(Err(e)),
    };
    SysOpResult::Async(Box::pin(connect_async(addr)))
}

fn extract_string(value: Option<&BexValue>) -> Result<String, OpError> {
    match value {
        Some(BexValue::External(BexExternalValue::String(s))) => Ok(s.clone()),
        other => Err(OpError::TypeError {
            expected: "string address",
            actual: format!("{other:?}"),
        }),
    }
}

async fn connect_async(addr: String) -> Result<BexExternalValue, OpError> {
    let stream = TcpStream::connect(&addr)
        .await
        .map_err(|e| OpError::Other(format!("Failed to connect to '{addr}': {e}")))?;

    let handle = REGISTRY.register_socket(stream, addr);
    Ok(bex_external_types::builtins::new_socket(handle))
}

/// Read data from a socket.
///
/// Signature: `fn read(self: Socket) -> String`
pub(crate) fn read(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let socket_ref = match args.first() {
        Some(value) => match SocketRef::from_value(&heap, value) {
            Ok(r) => r,
            Err(e) => return SysOpResult::Ready(Err(e)),
        },
        None => {
            return SysOpResult::Ready(Err(OpError::TypeError {
                expected: "Socket instance",
                actual: "no arguments".to_string(),
            }));
        }
    };

    let key = socket_ref.key();
    SysOpResult::Async(Box::pin(read_async(key)))
}

async fn read_async(key: usize) -> Result<BexExternalValue, OpError> {
    let stream_mutex = REGISTRY
        .get_socket(key)
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
pub(crate) fn close(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let result = close_sync(&heap, args);
    SysOpResult::Ready(result)
}

fn close_sync(heap: &Arc<BexHeap>, args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    use sys_resource_types::ResourceType;

    let socket_ref = match args.first() {
        Some(value) => SocketRef::from_value(heap, value)?,
        None => {
            return Err(OpError::TypeError {
                expected: "Socket instance",
                actual: "no arguments".to_string(),
            });
        }
    };

    if socket_ref.kind() != ResourceType::Socket {
        return Err(OpError::ResourceTypeMismatch { expected: "socket" });
    }

    // Resource closes when handle is dropped (cleanup callback removes from registry)
    drop(socket_ref);
    Ok(BexExternalValue::Null)
}
