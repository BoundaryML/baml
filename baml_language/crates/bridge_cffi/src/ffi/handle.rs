//! Handle lifecycle and media FFI entry points.

use std::{ffi::CStr, ptr};

use bex_project::MediaKind;
use bridge_ctypes::baml_bridge::cffi::MediaTypeEnum;

use crate::{
    Buffer,
    handle::{self, HandleError, HandleParts},
};

/// Status returned by the handle C ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BamlCffiStatus {
    /// The operation completed successfully and all documented outputs exist.
    Ok = 0,
    /// The key does not identify a live owned engine handle.
    InvalidHandle = 1,
    /// The supplied handle-type discriminator disagrees with the stored value.
    TypeMismatch = 2,
    /// The handle or media kind is recognized by the wire format but unsupported here.
    UnsupportedHandleType = 3,
    /// The operation failed internally, including invalid UTF-8 string input.
    InternalError = 4,
    /// A required input or output pointer was null.
    UnexpectedNullptr = 5,
}

impl From<HandleError> for BamlCffiStatus {
    fn from(error: HandleError) -> Self {
        match error {
            HandleError::InvalidHandle => Self::InvalidHandle,
            HandleError::TypeMismatch => Self::TypeMismatch,
            HandleError::UnsupportedHandleType => Self::UnsupportedHandleType,
            HandleError::InvalidInput(_) => Self::InternalError,
        }
    }
}

fn media_kind_from_proto(media_kind: i32) -> Option<MediaKind> {
    match media_kind {
        x if x == MediaTypeEnum::Image as i32 => Some(MediaKind::Image),
        x if x == MediaTypeEnum::Audio as i32 => Some(MediaKind::Audio),
        x if x == MediaTypeEnum::Pdf as i32 => Some(MediaKind::Pdf),
        x if x == MediaTypeEnum::Video as i32 => Some(MediaKind::Video),
        x if x == MediaTypeEnum::Other as i32 => Some(MediaKind::Generic),
        _ => None,
    }
}

fn write_handle_parts(
    parts: HandleParts,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    unsafe {
        *out_key = parts.key;
        *out_handle_type = parts.handle_type;
    }
    BamlCffiStatus::Ok
}

fn write_u64(out: *mut u64, value: u64) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    unsafe {
        *out = value;
    }
    BamlCffiStatus::Ok
}

fn write_optional_string(out: *mut Buffer, value: Option<String>) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    unsafe {
        *out = value
            .map(|s| Buffer::from(s.into_bytes()))
            .unwrap_or(Buffer {
                ptr: ptr::null(),
                len: 0,
            });
    }
    BamlCffiStatus::Ok
}

fn write_string(out: *mut Buffer, value: String) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    unsafe {
        *out = Buffer::from(value.into_bytes());
    }
    BamlCffiStatus::Ok
}

/// Clone a handle, creating a new owned key pointing to the same underlying value.
///
/// # Safety
/// `out_key` must be either null or valid for writing one `u64`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_handle_clone(key: u64, out_key: *mut u64) -> BamlCffiStatus {
    if out_key.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match handle::clone_handle(key) {
        Ok(new_key) => write_u64(out_key, new_key),
        Err(error) => error.into(),
    }
}

/// Release one owned handle key.
///
/// # Safety
/// The caller must pass a key previously returned by this CFFI handle API or
/// accept an `InvalidHandle` status.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_handle_release(key: u64) -> BamlCffiStatus {
    match handle::release_handle(key) {
        Ok(()) => BamlCffiStatus::Ok,
        Err(error) => error.into(),
    }
}

/// # Safety
/// `out_key` and `out_handle_type` must be either null or valid for writing
/// one value of their pointee type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __testonly_seed_function_ref(
    global_index: u64,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    write_handle_parts(
        handle::seed_function_ref_handle(global_index),
        out_key,
        out_handle_type,
    )
}

/// # Safety
/// `out_key` and `out_handle_type` must be either null or valid for writing
/// one value of their pointee type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __testonly_seed_generic_media(
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    write_handle_parts(
        handle::seed_generic_media_handle(),
        out_key,
        out_handle_type,
    )
}

/// # Safety
/// `url` and `mime_type_or_null`, when non-null, must point to valid
/// NUL-terminated C strings. `out_key` and `out_handle_type` must be either
/// null or valid for writing one value of their pointee type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_from_url(
    media_kind: i32,
    url: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if url.is_null() || out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    let Some(kind) = media_kind_from_proto(media_kind) else {
        return BamlCffiStatus::UnsupportedHandleType;
    };
    let url = match unsafe { CStr::from_ptr(url) }.to_str() {
        Ok(url) => url,
        Err(_) => return BamlCffiStatus::InternalError,
    };
    let mime_type = if mime_type_or_null.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(mime_type_or_null) }.to_str() {
            Ok(mime_type) => Some(mime_type),
            Err(_) => return BamlCffiStatus::InternalError,
        }
    };
    match handle::media_from_url(kind, url, mime_type) {
        Ok(parts) => write_handle_parts(parts, out_key, out_handle_type),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `path` and `mime_type_or_null`, when non-null, must point to valid
/// NUL-terminated C strings. `out_key` and `out_handle_type` must be either
/// null or valid for writing one value of their pointee type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_from_file(
    media_kind: i32,
    path: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if path.is_null() || out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    let Some(kind) = media_kind_from_proto(media_kind) else {
        return BamlCffiStatus::UnsupportedHandleType;
    };
    let path = match unsafe { CStr::from_ptr(path) }.to_str() {
        Ok(path) => path,
        Err(_) => return BamlCffiStatus::InternalError,
    };
    let mime_type = if mime_type_or_null.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(mime_type_or_null) }.to_str() {
            Ok(mime_type) => Some(mime_type),
            Err(_) => return BamlCffiStatus::InternalError,
        }
    };
    match handle::media_from_file(kind, path, mime_type) {
        Ok(parts) => write_handle_parts(parts, out_key, out_handle_type),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `base64` and `mime_type_or_null`, when non-null, must point to valid
/// NUL-terminated C strings. `out_key` and `out_handle_type` must be either
/// null or valid for writing one value of their pointee type.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_from_base64(
    media_kind: i32,
    base64: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if base64.is_null() || out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    let Some(kind) = media_kind_from_proto(media_kind) else {
        return BamlCffiStatus::UnsupportedHandleType;
    };
    let base64 = match unsafe { CStr::from_ptr(base64) }.to_str() {
        Ok(base64) => base64,
        Err(_) => return BamlCffiStatus::InternalError,
    };
    let mime_type = if mime_type_or_null.is_null() {
        None
    } else {
        match unsafe { CStr::from_ptr(mime_type_or_null) }.to_str() {
            Ok(mime_type) => Some(mime_type),
            Err(_) => return BamlCffiStatus::InternalError,
        }
    };
    match handle::media_from_base64(kind, base64, mime_type) {
        Ok(parts) => write_handle_parts(parts, out_key, out_handle_type),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `out` must be either null or valid for writing one `Buffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_url(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match handle::media_url(key, handle_type) {
        Ok(url) => write_optional_string(out, url),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `out` must be either null or valid for writing one `Buffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_file(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match handle::media_file(key, handle_type) {
        Ok(file) => write_optional_string(out, file),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `out` must be either null or valid for writing one `Buffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_base64(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match handle::media_base64(key, handle_type) {
        Ok(base64) => write_string(out, base64),
        Err(error) => error.into(),
    }
}

/// # Safety
/// `out` must be either null or valid for writing one `Buffer`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_media_mime_type(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    if out.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    match handle::media_mime_type(key, handle_type) {
        Ok(mime_type) => write_optional_string(out, mime_type),
        Err(error) => error.into(),
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::CString, ptr};

    use bridge_ctypes::baml_bridge::cffi::MediaTypeEnum;

    use super::*;

    #[test]
    fn clone_reports_unexpected_nullptr_for_null_out_key() {
        assert_eq!(
            unsafe { baml_handle_clone(1, ptr::null_mut()) },
            BamlCffiStatus::UnexpectedNullptr
        );
    }

    #[test]
    fn seed_reports_unexpected_nullptr_for_null_out_pointers() {
        let mut key = 0;
        let mut handle_type = 0;

        assert_eq!(
            unsafe { __testonly_seed_function_ref(1, ptr::null_mut(), &mut handle_type) },
            BamlCffiStatus::UnexpectedNullptr
        );
        assert_eq!(
            unsafe { __testonly_seed_function_ref(1, &mut key, ptr::null_mut()) },
            BamlCffiStatus::UnexpectedNullptr
        );
    }

    #[test]
    fn media_constructor_reports_unexpected_nullptr_for_required_null_pointers() {
        let url = CString::new("https://example.com/image.png").unwrap();
        let mut key = 0;
        let mut handle_type = 0;

        assert_eq!(
            unsafe {
                baml_media_from_url(
                    MediaTypeEnum::Image as i32,
                    ptr::null(),
                    ptr::null(),
                    &mut key,
                    &mut handle_type,
                )
            },
            BamlCffiStatus::UnexpectedNullptr
        );
        assert_eq!(
            unsafe {
                baml_media_from_url(
                    MediaTypeEnum::Image as i32,
                    url.as_ptr(),
                    ptr::null(),
                    ptr::null_mut(),
                    &mut handle_type,
                )
            },
            BamlCffiStatus::UnexpectedNullptr
        );
        assert_eq!(
            unsafe {
                baml_media_from_url(
                    MediaTypeEnum::Image as i32,
                    url.as_ptr(),
                    ptr::null(),
                    &mut key,
                    ptr::null_mut(),
                )
            },
            BamlCffiStatus::UnexpectedNullptr
        );
    }

    #[test]
    fn media_accessor_reports_unexpected_nullptr_before_invalid_handle() {
        assert_eq!(
            unsafe { baml_media_url(999, MediaTypeEnum::Image as i32, ptr::null_mut()) },
            BamlCffiStatus::UnexpectedNullptr
        );
    }
}
