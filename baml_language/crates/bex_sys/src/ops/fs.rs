//! File system operations.
//!
//! Implements `baml.fs.open`, `baml.fs.File.read`, and `baml.fs.File.close`.

use std::sync::Arc;

use tokio::{fs::File, io::AsyncReadExt};

use crate::{FileHandle, OpContext, OpError, ResolvedArgs, ResolvedValue};

// ============================================================================
// baml.fs.open
// ============================================================================

/// Opens a file and returns a resource ID.
///
/// Signature: `fn open(path: String) -> File`
pub async fn open(ctx: Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the path argument
    let path = match args.args.into_iter().next() {
        Some(ResolvedValue::String(s)) => s,
        other => {
            let msg = format!("Expected string path argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Open the file
    let file = File::open(&path)
        .await
        .map_err(|e| OpError::Other(format!("Failed to open file '{path}': {e}")))?;

    // Store in resources and return the ID
    let handle = FileHandle::new(file, path);
    let id = ctx.add_resource(handle);

    Ok(ResolvedValue::ResourceId(id))
}

// ============================================================================
// baml.fs.File.read
// ============================================================================

/// Reads the contents of a file.
///
/// Signature: `fn read(self: File) -> String`
pub async fn read(ctx: Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the file resource ID from the first argument
    // Note: ResourceId is passed as Int from the VM
    let file_id = match args.args.into_iter().next() {
        Some(ResolvedValue::Int(id)) => id.cast_unsigned(),
        Some(ResolvedValue::ResourceId(id)) => id,
        other => {
            let msg = format!("Expected file resource ID as first argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Get the file handle from resources
    // Clone the Arc<Mutex<File>> so we can release the lock before awaiting
    let file_mutex = {
        let guard = ctx.resources.lock().unwrap();
        let file_handle = guard
            .get_file(file_id)
            .ok_or(OpError::ResourceNotFound(file_id))?;
        Arc::clone(&file_handle.file)
    }; // lock guard dropped here

    // Now we can safely await the file mutex
    let mut file = file_mutex.lock().await;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .await
        .map_err(|e| OpError::Other(format!("Failed to read file: {e}")))?;

    Ok(ResolvedValue::String(contents))
}

// ============================================================================
// baml.fs.File.close
// ============================================================================

/// Closes a file, releasing the resource.
///
/// Signature: `fn close(self: File)`
pub fn close(ctx: &Arc<OpContext>, args: ResolvedArgs) -> Result<ResolvedValue, OpError> {
    // Extract the file resource ID from the first argument
    let file_id = match args.args.into_iter().next() {
        Some(ResolvedValue::Int(id)) => id.cast_unsigned(),
        Some(ResolvedValue::ResourceId(id)) => id,
        other => {
            let msg = format!("Expected file resource ID as first argument, got: {other:?}");
            return Err(OpError::Other(msg));
        }
    };

    // Remove the resource from the registry
    // This drops the Arc<Mutex<File>>, closing it when the last reference is dropped
    ctx.remove_resource(file_id)
        .ok_or(OpError::ResourceNotFound(file_id))?;

    Ok(ResolvedValue::Null)
}
