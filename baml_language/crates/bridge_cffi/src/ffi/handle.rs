//! Handle lifecycle FFI entry points.

use std::{ffi::CStr, ptr, sync::Arc};

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_core::cffi::BamlHandleType};

use crate::Buffer;

#[repr(i32)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub enum BamlCffiStatus {
    BAML_OK = 0,
    BAML_HANDLE_INVALID_HANDLE = 1,
    BAML_HANDLE_TYPE_MISMATCH = 2,
    BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE = 3,
    BAML_HANDLE_INTERNAL_ERROR = 4,
}

pub use BamlCffiStatus::{
    BAML_HANDLE_INTERNAL_ERROR, BAML_HANDLE_INVALID_HANDLE, BAML_HANDLE_TYPE_MISMATCH,
    BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE, BAML_OK,
};

fn handle_type_from_i32(handle_type: i32) -> Result<BamlHandleType, BamlCffiStatus> {
    BamlHandleType::try_from(handle_type).map_err(|_| BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE)
}

fn validate_entry_type(
    entry: &CffiHandleTableEntry,
    handle_type: i32,
) -> Result<BamlHandleType, BamlCffiStatus> {
    let requested = handle_type_from_i32(handle_type)?;
    if requested == BamlHandleType::HostValueCallable {
        return Err(BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE);
    }
    let intrinsic = entry.handle_type();
    if requested != BamlHandleType::HandleUnspecified && requested != intrinsic {
        return Err(BAML_HANDLE_TYPE_MISMATCH);
    }
    Ok(intrinsic)
}

fn missing_key_status(handle_type: i32) -> BamlCffiStatus {
    match handle_type_from_i32(handle_type) {
        Ok(BamlHandleType::HostValueCallable) | Err(_) => BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE,
        Ok(_) => BAML_HANDLE_INVALID_HANDLE,
    }
}

pub fn handle_validate_impl(key: u64, handle_type: i32) -> BamlCffiStatus {
    match HANDLE_TABLE.resolve(key) {
        Some(entry) => match validate_entry_type(&entry, handle_type) {
            Ok(_) => BAML_OK,
            Err(status) => status,
        },
        None => missing_key_status(handle_type),
    }
}

pub fn handle_clone_impl(
    key: u64,
    handle_type: i32,
    out_key: Option<&mut u64>,
    out_handle_type: Option<&mut i32>,
) -> BamlCffiStatus {
    let Some(entry) = HANDLE_TABLE.resolve(key) else {
        return missing_key_status(handle_type);
    };
    let intrinsic = match validate_entry_type(&entry, handle_type) {
        Ok(intrinsic) => intrinsic,
        Err(status) => return status,
    };
    let Some(new_key) = HANDLE_TABLE.clone_handle(key) else {
        return BAML_HANDLE_INVALID_HANDLE;
    };
    if let Some(out_key) = out_key {
        *out_key = new_key;
    }
    if let Some(out_handle_type) = out_handle_type {
        *out_handle_type = intrinsic as i32;
    }
    BAML_OK
}

pub fn handle_release_impl(key: u64, handle_type: i32) -> BamlCffiStatus {
    let Some(entry) = HANDLE_TABLE.resolve(key) else {
        return missing_key_status(handle_type);
    };
    if let Err(status) = validate_entry_type(&entry, handle_type) {
        return status;
    }
    if HANDLE_TABLE.release(key) {
        BAML_OK
    } else {
        BAML_HANDLE_INVALID_HANDLE
    }
}

pub fn handle_type_impl(key: u64) -> Result<BamlHandleType, BamlCffiStatus> {
    HANDLE_TABLE
        .resolve(key)
        .map(|entry| entry.handle_type())
        .ok_or(BAML_HANDLE_INVALID_HANDLE)
}

pub fn insert_media_impl(kind: MediaKind, media: Arc<MediaValue>) -> (u64, i32) {
    let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(media));
    let handle_type = entry.handle_type() as i32;
    let key = HANDLE_TABLE.insert(entry);
    debug_assert_eq!(handle_type, media_kind_to_handle_type(kind) as i32);
    (key, handle_type)
}

pub fn media_from_url_impl(kind: MediaKind, url: &str, mime_type: Option<&str>) -> (u64, i32) {
    insert_media_impl(kind, MediaValue::from_url(kind, url, mime_type))
}

pub fn media_from_file_impl(kind: MediaKind, file: &str, mime_type: Option<&str>) -> (u64, i32) {
    insert_media_impl(kind, MediaValue::from_file(kind, file, mime_type))
}

pub fn media_from_base64_impl(
    kind: MediaKind,
    base64: &str,
    mime_type: Option<&str>,
) -> (u64, i32) {
    insert_media_impl(kind, MediaValue::from_base64(kind, base64, mime_type))
}

pub fn media_value_impl(
    key: u64,
    handle_type: i32,
    expected_kind: MediaKind,
) -> Result<Arc<MediaValue>, BamlCffiStatus> {
    let entry = HANDLE_TABLE
        .resolve(key)
        .ok_or(BAML_HANDLE_INVALID_HANDLE)?;
    validate_entry_type(&entry, handle_type)?;
    match &*entry {
        CffiHandleTableEntry::Adt(BexExternalAdt::Media(media)) if media.kind == expected_kind => {
            Ok(Arc::clone(media))
        }
        _ => Err(BAML_HANDLE_TYPE_MISMATCH),
    }
}

fn media_kind_to_handle_type(kind: MediaKind) -> BamlHandleType {
    match kind {
        MediaKind::Image => BamlHandleType::AdtMediaImage,
        MediaKind::Audio => BamlHandleType::AdtMediaAudio,
        MediaKind::Video => BamlHandleType::AdtMediaVideo,
        MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
        MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
    }
}

fn media_kind_from_i32(media_kind: i32) -> Result<MediaKind, BamlCffiStatus> {
    match handle_type_from_i32(media_kind)? {
        BamlHandleType::AdtMediaImage => Ok(MediaKind::Image),
        BamlHandleType::AdtMediaAudio => Ok(MediaKind::Audio),
        BamlHandleType::AdtMediaVideo => Ok(MediaKind::Video),
        BamlHandleType::AdtMediaPdf => Ok(MediaKind::Pdf),
        BamlHandleType::AdtMediaGeneric => Ok(MediaKind::Generic),
        _ => Err(BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE),
    }
}

unsafe fn opt_cstr<'a>(ptr: *const libc::c_char) -> Result<Option<&'a str>, BamlCffiStatus> {
    if ptr.is_null() {
        return Ok(None);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map(Some)
        .map_err(|_| BAML_HANDLE_INTERNAL_ERROR)
}

unsafe fn required_cstr<'a>(ptr: *const libc::c_char) -> Result<&'a str, BamlCffiStatus> {
    if ptr.is_null() {
        return Err(BAML_HANDLE_INTERNAL_ERROR);
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .map_err(|_| BAML_HANDLE_INTERNAL_ERROR)
}

unsafe fn write_handle_outputs(
    out_key: *mut u64,
    out_handle_type: *mut i32,
    key: u64,
    handle_type: i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BAML_HANDLE_INTERNAL_ERROR;
    }
    unsafe {
        *out_key = key;
        *out_handle_type = handle_type;
    }
    BAML_OK
}

fn buffer_from_option(value: Option<String>, out: *mut Buffer) -> BamlCffiStatus {
    if out.is_null() {
        return BAML_HANDLE_INTERNAL_ERROR;
    }
    unsafe {
        *out = value
            .map(|s| Buffer::from(s.into_bytes()))
            .unwrap_or(Buffer {
                ptr: ptr::null(),
                len: 0,
            });
    }
    BAML_OK
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_validate(key: u64, handle_type: i32) -> BamlCffiStatus {
    handle_validate_impl(key, handle_type)
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_clone(
    key: u64,
    handle_type: i32,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    if out_key.is_null() || out_handle_type.is_null() {
        return BAML_HANDLE_INTERNAL_ERROR;
    }
    let mut new_key = 0;
    let mut new_handle_type = 0;
    let status = handle_clone_impl(
        key,
        handle_type,
        Some(&mut new_key),
        Some(&mut new_handle_type),
    );
    if status == BAML_OK {
        unsafe {
            *out_key = new_key;
            *out_handle_type = new_handle_type;
        }
    }
    status
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_release(key: u64, handle_type: i32) -> BamlCffiStatus {
    handle_release_impl(key, handle_type)
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_type(key: u64, out_handle_type: *mut i32) -> BamlCffiStatus {
    if out_handle_type.is_null() {
        return BAML_HANDLE_INTERNAL_ERROR;
    }
    match handle_type_impl(key) {
        Ok(handle_type) => {
            unsafe {
                *out_handle_type = handle_type as i32;
            }
            BAML_OK
        }
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_test_seed_function_ref(
    global_index: u64,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    let entry = CffiHandleTableEntry::FunctionRef {
        global_index: global_index as usize,
    };
    let handle_type = entry.handle_type() as i32;
    let key = HANDLE_TABLE.insert(entry);
    unsafe { write_handle_outputs(out_key, out_handle_type, key, handle_type) }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_handle_test_seed_generic_media(
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    let (key, handle_type) = media_from_url_impl(MediaKind::Generic, "https://example.com/", None);
    unsafe { write_handle_outputs(out_key, out_handle_type, key, handle_type) }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_from_url(
    media_kind: i32,
    url: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    let result = (|| {
        let kind = media_kind_from_i32(media_kind)?;
        let url = unsafe { required_cstr(url)? };
        let mime_type = unsafe { opt_cstr(mime_type_or_null)? };
        Ok::<_, BamlCffiStatus>(media_from_url_impl(kind, url, mime_type))
    })();
    match result {
        Ok((key, handle_type)) => unsafe {
            write_handle_outputs(out_key, out_handle_type, key, handle_type)
        },
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_from_file(
    media_kind: i32,
    path: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    let result = (|| {
        let kind = media_kind_from_i32(media_kind)?;
        let path = unsafe { required_cstr(path)? };
        let mime_type = unsafe { opt_cstr(mime_type_or_null)? };
        Ok::<_, BamlCffiStatus>(media_from_file_impl(kind, path, mime_type))
    })();
    match result {
        Ok((key, handle_type)) => unsafe {
            write_handle_outputs(out_key, out_handle_type, key, handle_type)
        },
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_from_base64(
    media_kind: i32,
    base64: *const libc::c_char,
    mime_type_or_null: *const libc::c_char,
    out_key: *mut u64,
    out_handle_type: *mut i32,
) -> BamlCffiStatus {
    let result = (|| {
        let kind = media_kind_from_i32(media_kind)?;
        let base64 = unsafe { required_cstr(base64)? };
        let mime_type = unsafe { opt_cstr(mime_type_or_null)? };
        Ok::<_, BamlCffiStatus>(media_from_base64_impl(kind, base64, mime_type))
    })();
    match result {
        Ok((key, handle_type)) => unsafe {
            write_handle_outputs(out_key, out_handle_type, key, handle_type)
        },
        Err(status) => status,
    }
}

fn media_accessor(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
    get: impl FnOnce(&MediaValue) -> Option<String>,
) -> BamlCffiStatus {
    let Ok(expected_kind) = media_kind_from_i32(handle_type) else {
        return BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE;
    };
    match media_value_impl(key, handle_type, expected_kind) {
        Ok(media) => buffer_from_option(get(&media), out),
        Err(status) => status,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_url(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus {
    media_accessor(key, handle_type, out, MediaValue::url)
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_file(key: u64, handle_type: i32, out: *mut Buffer) -> BamlCffiStatus {
    media_accessor(key, handle_type, out, MediaValue::file)
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_base64(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    media_accessor(key, handle_type, out, |media| Some(media.base64()))
}

#[unsafe(no_mangle)]
pub extern "C" fn baml_media_mime_type(
    key: u64,
    handle_type: i32,
    out: *mut Buffer,
) -> BamlCffiStatus {
    media_accessor(key, handle_type, out, MediaValue::mime_type)
}

/// Compatibility alias for the old unchecked API.
#[unsafe(no_mangle)]
pub extern "C" fn clone_handle(key: u64) -> u64 {
    let mut out_key = 0;
    let status = handle_clone_impl(
        key,
        BamlHandleType::HandleUnspecified as i32,
        Some(&mut out_key),
        None,
    );
    if status == BAML_OK { out_key } else { 0 }
}

/// Compatibility alias for the old unchecked API.
#[unsafe(no_mangle)]
pub extern "C" fn release_handle(key: u64) {
    let _ = handle_release_impl(key, BamlHandleType::HandleUnspecified as i32);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_unknown_key() {
        assert_eq!(
            handle_validate_impl(9_999_999, BamlHandleType::FunctionRef as i32),
            BAML_HANDLE_INVALID_HANDLE
        );
    }

    #[test]
    fn validate_rejects_mismatched_type() {
        let key = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 7 });
        assert_eq!(
            handle_validate_impl(key, BamlHandleType::AdtMediaImage as i32),
            BAML_HANDLE_TYPE_MISMATCH
        );
        let _ = handle_release_impl(key, BamlHandleType::FunctionRef as i32);
    }

    #[test]
    fn clone_returns_distinct_key_and_release_is_independent() {
        let key = HANDLE_TABLE.insert(CffiHandleTableEntry::FunctionRef { global_index: 7 });
        let mut cloned_key = 0;
        let mut cloned_type = 0;
        assert_eq!(
            handle_clone_impl(
                key,
                BamlHandleType::FunctionRef as i32,
                Some(&mut cloned_key),
                Some(&mut cloned_type),
            ),
            BAML_OK
        );
        assert_ne!(key, cloned_key);
        assert_eq!(cloned_type, BamlHandleType::FunctionRef as i32);
        assert_eq!(
            handle_release_impl(cloned_key, BamlHandleType::FunctionRef as i32),
            BAML_OK
        );
        assert_eq!(
            handle_validate_impl(key, BamlHandleType::FunctionRef as i32),
            BAML_OK
        );
        let _ = handle_release_impl(key, BamlHandleType::FunctionRef as i32);
    }

    #[test]
    fn host_value_callable_is_unsupported() {
        assert_eq!(
            handle_validate_impl(1, BamlHandleType::HostValueCallable as i32),
            BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE
        );
    }

    #[test]
    fn unknown_handle_type_is_unsupported() {
        assert_eq!(
            handle_validate_impl(1, 123_456),
            BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE
        );
    }

    #[test]
    fn media_constructor_and_accessor_round_trip() {
        let (key, handle_type) = media_from_url_impl(
            MediaKind::Image,
            "https://example.com/img.png",
            Some("image/png"),
        );
        assert_eq!(handle_type, BamlHandleType::AdtMediaImage as i32);
        let media = media_value_impl(key, handle_type, MediaKind::Image).unwrap();
        assert_eq!(media.url().as_deref(), Some("https://example.com/img.png"));
        assert_eq!(media.mime_type().as_deref(), Some("image/png"));
        assert!(media_value_impl(key, handle_type, MediaKind::Audio).is_err());
        let _ = handle_release_impl(key, handle_type);
    }
}
