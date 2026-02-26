//! Handle lifecycle FFI entry points.

use bridge_ctypes::HANDLE_TABLE;

/// Clone a handle — creates a new key pointing to the same underlying value.
#[unsafe(no_mangle)]
pub extern "C" fn clone_handle(key: u64) -> u64 {
    HANDLE_TABLE.clone_handle(key).unwrap_or(0)
}

/// Release a handle — removes the entry from the table.
/// The value is dropped when the last Arc clone is released.
#[unsafe(no_mangle)]
pub extern "C" fn release_handle(key: u64) {
    HANDLE_TABLE.release(key);
}
