// This module previously contained handle_ffi_error() and free_error_string().
// These have been removed in favor of the unified Buffer-based FFI pattern.
// All FFI functions now return Buffer with protobuf InvocationResponse.
