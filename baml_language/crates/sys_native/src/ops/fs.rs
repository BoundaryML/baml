//! File system operations.

use bex_external_types::BexExternalValue;
use sys_types::{OpError, SysOpResult};
use tokio::{fs::File, io::AsyncReadExt};

use crate::registry::REGISTRY;

/// Opens a file and returns a resource.
///
/// Signature: `fn open(path: String) -> File`
pub(crate) fn open(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(open_async(args)))
}

async fn open_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let path = match args.into_iter().next() {
        Some(BexExternalValue::String(s)) => s,
        other => {
            return Err(OpError::TypeError {
                expected: "string path",
                actual: format!("{other:?}"),
            });
        }
    };

    let file = File::open(&path)
        .await
        .map_err(|e| OpError::Other(format!("Failed to open file '{path}': {e}")))?;

    let handle = REGISTRY.register_file(file, path);
    Ok(BexExternalValue::Resource(handle))
}

/// Reads the contents of a file.
///
/// Signature: `fn read(self: File) -> String`
pub(crate) fn read(args: Vec<BexExternalValue>) -> SysOpResult {
    SysOpResult::Async(Box::pin(read_async(args)))
}

async fn read_async(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let handle = match args.into_iter().next() {
        Some(BexExternalValue::Resource(h)) => h,
        other => {
            return Err(OpError::TypeError {
                expected: "file resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let file_mutex = REGISTRY
        .get_file(handle.key())
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
                expected: "file resource",
                actual: format!("{other:?}"),
            });
        }
    };

    if handle.kind() != ResourceType::File {
        return Err(OpError::ResourceTypeMismatch { expected: "file" });
    }

    // Resource closes when handle is dropped (cleanup callback removes from registry)
    drop(handle);
    Ok(BexExternalValue::Null)
}
