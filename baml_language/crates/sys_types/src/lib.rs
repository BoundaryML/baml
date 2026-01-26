//! BEX Sys - System operations for the BEX runtime.
//!
//! This crate provides external I/O operations (file system, network, shell)
//! that the BEX engine can dispatch to. Operations receive and return
//! `BexExternalValue` directly.

use std::{future::Future, pin::Pin};

// Re-export BexExternalValue for ops
pub use bex_external_types::BexExternalValue;
// Re-export SysOp for convenience
pub use bex_vm_types::SysOp;
// Re-export resource types
pub use sys_resource_types::{ResourceHandle, ResourceType};

// ============================================================================
// Operation Errors
// ============================================================================

/// Errors that can occur during external operation execution.
#[derive(Debug, thiserror::Error)]
pub enum OpError {
    #[error("{0}")]
    Other(String),

    #[error("Expected {expected}, got {actual}")]
    TypeError {
        expected: &'static str,
        actual: String,
    },

    #[error("Expected resource of type {expected}")]
    ResourceTypeMismatch { expected: &'static str },

    #[error("Operation not supported: {operation:?}")]
    Unsupported { operation: SysOp },
}

// ============================================================================
// Operation Results
// ============================================================================

/// A boxed future for async operations.
pub type OpFuture = Pin<Box<dyn Future<Output = Result<BexExternalValue, OpError>> + Send>>;

/// Result of a system operation - either immediate or async.
pub enum SysOpResult {
    /// Operation completed synchronously with this result.
    Ready(Result<BexExternalValue, OpError>),
    /// Operation is async and needs to be awaited.
    Async(OpFuture),
}

// ============================================================================
// System Operations Table
// ============================================================================

/// Function pointer type for system operations.
///
/// Each operation takes a vector of arguments and returns a `SysOpResult`
/// which is either an immediate result or a future to await.
pub type SysOpFn = fn(Vec<BexExternalValue>) -> SysOpResult;

/// Table of system operation implementations.
///
/// This struct is passed to `BexEngine::new()` and determines how system
/// operations are executed. Different providers (native Tokio, WASM, FFI)
/// can supply different implementations.
///
/// # Example
///
/// ```ignore
/// // Using the native Tokio provider
/// let sys_ops = sys_types_native::SysOps::native();
/// let engine = BexEngine::new(program, env_vars, sys_ops)?;
/// ```
#[derive(Clone)]
pub struct SysOps {
    // File system operations
    pub fs_open: SysOpFn,
    pub fs_read: SysOpFn,
    pub fs_close: SysOpFn,

    // Network operations
    pub net_connect: SysOpFn,
    pub net_read: SysOpFn,
    pub net_close: SysOpFn,

    // System operations
    pub shell: SysOpFn,

    // HTTP operations
    pub http_fetch: SysOpFn,
    pub http_response_text: SysOpFn,
    pub http_response_status: SysOpFn,
    pub http_response_ok: SysOpFn,
    pub http_response_url: SysOpFn,
    pub http_response_headers: SysOpFn,
}

impl SysOps {
    /// Create a function that always returns `OpError::Unsupported`.
    ///
    /// Useful for providers that don't support certain operations.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use bex_vm_types::SysOp;
    ///
    /// let sys_ops = SysOps {
    ///     shell: SysOps::unsupported(SysOp::Shell),  // WASM can't run shell commands
    ///     // ... other ops
    /// };
    /// ```
    pub fn unsupported(operation: SysOp) -> SysOpFn {
        // Match on the enum variant to return the appropriate function pointer.
        // Each closure captures nothing, so they can be coerced to fn pointers.
        match operation {
            SysOp::FsOpen => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::FsOpen,
                }))
            },
            SysOp::FsRead => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::FsRead,
                }))
            },
            SysOp::FsClose => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::FsClose,
                }))
            },
            SysOp::NetConnect => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::NetConnect,
                }))
            },
            SysOp::NetRead => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::NetRead,
                }))
            },
            SysOp::NetClose => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::NetClose,
                }))
            },
            SysOp::Shell => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::Shell,
                }))
            },
            SysOp::HttpFetch => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpFetch,
                }))
            },
            SysOp::HttpResponseText => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpResponseText,
                }))
            },
            SysOp::HttpResponseStatus => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpResponseStatus,
                }))
            },
            SysOp::HttpResponseOk => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpResponseOk,
                }))
            },
            SysOp::HttpResponseUrl => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpResponseUrl,
                }))
            },
            SysOp::HttpResponseHeaders => |_| {
                SysOpResult::Ready(Err(OpError::Unsupported {
                    operation: SysOp::HttpResponseHeaders,
                }))
            },
        }
    }

    /// Create a `SysOps` table where all operations return `Unsupported`.
    ///
    /// Useful as a base for providers that only implement some operations.
    pub fn all_unsupported() -> Self {
        Self {
            fs_open: Self::unsupported(SysOp::FsOpen),
            fs_read: Self::unsupported(SysOp::FsRead),
            fs_close: Self::unsupported(SysOp::FsClose),
            net_connect: Self::unsupported(SysOp::NetConnect),
            net_read: Self::unsupported(SysOp::NetRead),
            net_close: Self::unsupported(SysOp::NetClose),
            shell: Self::unsupported(SysOp::Shell),
            http_fetch: Self::unsupported(SysOp::HttpFetch),
            http_response_text: Self::unsupported(SysOp::HttpResponseText),
            http_response_status: Self::unsupported(SysOp::HttpResponseStatus),
            http_response_ok: Self::unsupported(SysOp::HttpResponseOk),
            http_response_url: Self::unsupported(SysOp::HttpResponseUrl),
            http_response_headers: Self::unsupported(SysOp::HttpResponseHeaders),
        }
    }
}

// ============================================================================
// Async Completion Utilities
// ============================================================================

/// Handle for completing an async operation from external code.
///
/// This is used for FFI async bridging - the host language receives this handle
/// and calls `complete()` when the operation finishes.
///
/// # Example
///
/// ```ignore
/// // In the binding code:
/// let (result, handle) = SysOpResult::pending();
/// spawn_python_task(move || {
///     let data = python_http_get(url);
///     handle.complete(Ok(BexExternalValue::String(data)));
/// });
/// return result;  // Returns the future to the engine
/// ```
pub struct CompletionHandle(tokio::sync::oneshot::Sender<Result<BexExternalValue, OpError>>);

impl CompletionHandle {
    /// Complete the async operation with the given result.
    ///
    /// This resolves the future returned by `SysOpResult::pending()`.
    pub fn complete(self, result: Result<BexExternalValue, OpError>) {
        // Ignore send error - receiver was dropped (operation cancelled)
        let _ = self.0.send(result);
    }
}

impl SysOpResult {
    /// Create a pending async result that can be completed externally.
    ///
    /// Returns a tuple of:
    /// - `SysOpResult::Async` containing the future
    /// - `CompletionHandle` to complete the operation
    ///
    /// The future will resolve when `handle.complete()` is called.
    pub fn pending() -> (Self, CompletionHandle) {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let future = Box::pin(async move {
            rx.await
                .unwrap_or(Err(OpError::Other("Operation cancelled".into())))
        });
        (SysOpResult::Async(future), CompletionHandle(tx))
    }
}

// ============================================================================
// Host Resource Abstraction
// ============================================================================

// Re-export ResourceType and ResourceHandle from sys_resource_types
// (already done above)

/// Callback trait for host to release resources when GC collects them.
///
/// Implementations receive notifications when the VM no longer references
/// a resource, allowing the host language to clean up the underlying handle.
pub trait HostResourceRef: Send + Sync {
    /// Called when a resource is no longer referenced by the VM.
    fn release_resource(&self, handle_id: u64, resource_type: ResourceType);
}

/// A no-op implementation for native Rust where Arc handles cleanup.
pub struct NoopHostRef;

impl HostResourceRef for NoopHostRef {
    fn release_resource(&self, _handle_id: u64, _resource_type: ResourceType) {
        // No-op - cleanup is handled by ResourceHandle's Drop
    }
}

#[cfg(test)]
mod tests {
    use bex_vm_types::SysOp;

    use super::*;

    #[test]
    fn test_unsupported_returns_error() {
        let op = SysOps::unsupported(SysOp::Shell);
        let result = op(vec![]);
        match result {
            SysOpResult::Ready(Err(OpError::Unsupported { operation })) => {
                assert_eq!(operation, SysOp::Shell);
            }
            _ => panic!("Expected Unsupported error"),
        }
    }

    #[test]
    fn test_all_unsupported() {
        let ops = SysOps::all_unsupported();

        // Test each operation returns Unsupported
        let result = (ops.fs_open)(vec![]);
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError::Unsupported {
                operation: SysOp::FsOpen
            }))
        ));

        let result = (ops.shell)(vec![]);
        assert!(matches!(
            result,
            SysOpResult::Ready(Err(OpError::Unsupported {
                operation: SysOp::Shell
            }))
        ));
    }

    #[tokio::test]
    async fn test_completion_handle() {
        let (result, handle) = SysOpResult::pending();

        // Complete in another task
        tokio::spawn(async move {
            handle.complete(Ok(BexExternalValue::String("done".into())));
        });

        // Await the result
        match result {
            SysOpResult::Async(fut) => {
                let value = fut.await.unwrap();
                assert!(matches!(value, BexExternalValue::String(s) if s == "done"));
            }
            SysOpResult::Ready(_) => panic!("Expected Async result"),
        }
    }
}
