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

use bex_heap::BexHeap;
use bex_vm_types::{Object, Value};
use sys_resource_types::ResourceHandle;
use sys_types::{BexExternalValue, BexValue, OpError, SysOpResult};
use tokio::{fs::File, io::AsyncReadExt};

use crate::registry::REGISTRY;

// ============================================================================
// Wrapper Types for GC-Safe Instance Access
// ============================================================================

/// Wrapper for File instance that provides GC-safe access to fields.
struct FileRef {
    /// The resource handle extracted from the `_handle` field.
    resource_handle: ResourceHandle,
}

impl FileRef {
    /// Extract a `FileRef` from a `BexValue` that should be a File instance.
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
                            expected: "File instance",
                            actual: format!("{obj:?}"),
                        });
                    };

                    let Object::Class(class) = (unsafe { inst.class.get() }) else {
                        return Err(OpError::Other("bad class ptr".into()));
                    };

                    if class.name != "baml.fs.File" {
                        return Err(OpError::TypeError {
                            expected: "File instance",
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
                if class_name != "baml.fs.File" {
                    return Err(OpError::TypeError {
                        expected: "File instance",
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
                expected: "File instance",
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
// File System Operations
// ============================================================================

/// Opens a file and returns a resource.
///
/// Signature: `fn open(path: String) -> File`
pub(crate) fn open(_heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let path = match extract_string(args.first()) {
        Ok(p) => p,
        Err(e) => return SysOpResult::Ready(Err(e)),
    };
    SysOpResult::Async(Box::pin(open_async(path)))
}

fn extract_string(value: Option<&BexValue>) -> Result<String, OpError> {
    match value {
        Some(BexValue::External(BexExternalValue::String(s))) => Ok(s.clone()),
        other => Err(OpError::TypeError {
            expected: "string path",
            actual: format!("{other:?}"),
        }),
    }
}

async fn open_async(path: String) -> Result<BexExternalValue, OpError> {
    let file = File::open(&path)
        .await
        .map_err(|e| OpError::Other(format!("Failed to open file '{path}': {e}")))?;

    let handle = REGISTRY.register_file(file, path);
    Ok(bex_external_types::builtins::new_file(handle))
}

/// Reads the contents of a file.
///
/// Signature: `fn read(self: File) -> String`
pub(crate) fn read(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let file_ref = match args.first() {
        Some(value) => match FileRef::from_value(&heap, value) {
            Ok(r) => r,
            Err(e) => return SysOpResult::Ready(Err(e)),
        },
        None => {
            return SysOpResult::Ready(Err(OpError::TypeError {
                expected: "File instance",
                actual: "no arguments".to_string(),
            }));
        }
    };

    let key = file_ref.key();
    SysOpResult::Async(Box::pin(read_async(key)))
}

async fn read_async(key: usize) -> Result<BexExternalValue, OpError> {
    let file_mutex = REGISTRY
        .get_file(key)
        .ok_or_else(|| OpError::Other("File handle is invalid or has been closed".into()))?;

    let mut file = file_mutex.lock().await;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .await
        .map_err(|e| OpError::Other(format!("Failed to read file: {e}")))?;

    Ok(BexExternalValue::String(contents))
}

/// Closes a file, releasing the resource.
///
/// Signature: `fn close(self: File)`
pub(crate) fn close(heap: Arc<BexHeap>, args: &[BexValue]) -> SysOpResult {
    let result = close_sync(&heap, args);
    SysOpResult::Ready(result)
}

fn close_sync(heap: &Arc<BexHeap>, args: &[BexValue]) -> Result<BexExternalValue, OpError> {
    use sys_resource_types::ResourceType;

    let file_ref = match args.first() {
        Some(value) => FileRef::from_value(heap, value)?,
        None => {
            return Err(OpError::TypeError {
                expected: "File instance",
                actual: "no arguments".to_string(),
            });
        }
    };

    if file_ref.kind() != ResourceType::File {
        return Err(OpError::ResourceTypeMismatch { expected: "file" });
    }

    // Resource closes when handle is dropped (cleanup callback removes from registry)
    drop(file_ref);
    Ok(BexExternalValue::Null)
}
