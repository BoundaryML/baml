//! File system operations.

use std::sync::Arc;

use tokio::{fs::File, io::AsyncReadExt};

use crate::{BexExternalValue, FileHandle, OpError, ResourceKind};

/// Opens a file and returns a resource.
///
/// Signature: `fn open(path: String) -> File`
pub async fn open(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
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

    let handle = FileHandle::new(file, path);
    Ok(BexExternalValue::Resource(Arc::new(ResourceKind::File(
        handle,
    ))))
}

/// Reads the contents of a file.
///
/// Signature: `fn read(self: File) -> String`
pub async fn read(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let file_arc = match args.into_iter().next() {
        Some(BexExternalValue::Resource(arc)) => arc,
        other => {
            return Err(OpError::TypeError {
                expected: "file resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let ResourceKind::File(file_handle) = file_arc.as_ref() else {
        return Err(OpError::ResourceTypeMismatch { expected: "file" });
    };

    let mut file = file_handle.file.lock().await;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .await
        .map_err(|e| OpError::Other(format!("Failed to read file: {e}")))?;

    Ok(BexExternalValue::String(contents))
}

/// Closes a file, releasing the resource.
///
/// Signature: `fn close(self: File)`
pub fn close(args: Vec<BexExternalValue>) -> Result<BexExternalValue, OpError> {
    let file_arc = match args.into_iter().next() {
        Some(BexExternalValue::Resource(arc)) => arc,
        other => {
            return Err(OpError::TypeError {
                expected: "file resource",
                actual: format!("{other:?}"),
            });
        }
    };

    let ResourceKind::File(_) = file_arc.as_ref() else {
        return Err(OpError::ResourceTypeMismatch { expected: "file" });
    };

    // Resource closes when Arc is dropped
    drop(file_arc);
    Ok(BexExternalValue::Null)
}
