//! Handle lifecycle and media FFI entry points.

use std::{ffi::CStr, ptr};

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{
    CffiHandleTableEntry, HANDLE_TABLE,
    baml_core::cffi::{BamlHandleType, MediaTypeEnum},
};

use crate::Buffer;

/// Status returned by the handle C ABI.
#[repr(u32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BamlCffiStatus {
    Ok = 0,
    InvalidHandle = 1,
    TypeMismatch = 2,
    UnsupportedHandleType = 3,
    InternalError = 4,
    UnexpectedNullptr = 5,
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

fn write_handle(
    entry: CffiHandleTableEntry,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BamlCffiStatus::UnexpectedNullptr;
    }
    let handle_type = entry.handle_type() as i32;
    let key = HANDLE_TABLE.insert(entry);
    unsafe {
        *out_key = key;
        *out_handle_type = handle_type;
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

fn media_entry(key: u64, handle_type: i32) -> Result<std::sync::Arc<MediaValue>, BamlCffiStatus> {
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or(BamlCffiStatus::InvalidHandle)?;

    if handle_type != BamlHandleType::HandleUnspecified as i32
        && handle_type != entry.handle_type() as i32
    {
        return Err(BamlCffiStatus::TypeMismatch);
    }

    match &*entry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media)) => Ok(media.clone()),
        _ => Err(BamlCffiStatus::UnsupportedHandleType),
    }
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
    match HANDLE_TABLE.clone_handle(key) {
        Some(new_key) => write_u64(out_key, new_key),
        None => BamlCffiStatus::InvalidHandle,
    }
}

/// Release one owned handle key.
///
/// # Safety
/// The caller must pass a key previously returned by this CFFI handle API or
/// accept an `InvalidHandle` status.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn baml_handle_release(key: u64) -> BamlCffiStatus {
    if HANDLE_TABLE.release(key) {
        BamlCffiStatus::Ok
    } else {
        BamlCffiStatus::InvalidHandle
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
    write_handle(
        CffiHandleTableEntry::FunctionRef {
            global_index: global_index as usize,
        },
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
    write_handle(
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_url(
            MediaKind::Generic,
            "https://example.com/",
            None,
        ))),
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
    write_handle(
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_url(
            kind, url, mime_type,
        ))),
        out_key,
        out_handle_type,
    )
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
    write_handle(
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_file(
            kind, path, mime_type,
        ))),
        out_key,
        out_handle_type,
    )
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
    write_handle(
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(MediaValue::from_base64(
            kind, base64, mime_type,
        ))),
        out_key,
        out_handle_type,
    )
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
    match media_entry(key, handle_type) {
        Ok(media) => write_optional_string(out, media.url()),
        Err(status) => status,
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
    match media_entry(key, handle_type) {
        Ok(media) => write_optional_string(out, media.file()),
        Err(status) => status,
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
    match media_entry(key, handle_type) {
        Ok(media) => write_string(out, media.base64()),
        Err(status) => status,
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
    match media_entry(key, handle_type) {
        Ok(media) => write_optional_string(out, media.mime_type()),
        Err(status) => status,
    }
}

#[cfg(test)]
mod tests {
    use std::{ffi::CString, ptr};

    use bridge_ctypes::baml_core::cffi::MediaTypeEnum;

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
