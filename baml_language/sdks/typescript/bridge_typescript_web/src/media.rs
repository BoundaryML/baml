//! Raw media handle bindings.

use bex_project::MediaKind;
use bridge_cffi::handle_cffi::{self, HandleError, HandleParts};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use wasm_bindgen::prelude::*;

use crate::errors::{handle_error, unexpected_handle_type};

fn media_kind(media_kind: i32, operation: &'static str) -> Result<MediaKind, JsError> {
    handle_cffi::media_kind_from_proto(media_kind)
        .ok_or_else(|| handle_error(operation, &HandleError::UnsupportedHandleType))
}

fn expected_handle_type(kind: MediaKind) -> BamlHandleType {
    match kind {
        MediaKind::Image => BamlHandleType::AdtMediaImage,
        MediaKind::Audio => BamlHandleType::AdtMediaAudio,
        MediaKind::Video => BamlHandleType::AdtMediaVideo,
        MediaKind::Pdf => BamlHandleType::AdtMediaPdf,
        MediaKind::Generic => BamlHandleType::AdtMediaGeneric,
    }
}

fn media_key(operation: &'static str, kind: MediaKind, parts: HandleParts) -> Result<u64, JsError> {
    let expected = expected_handle_type(kind) as i32;
    if parts.handle_type != expected {
        return Err(unexpected_handle_type(
            operation,
            expected,
            parts.handle_type,
        ));
    }
    Ok(parts.key)
}

#[wasm_bindgen(js_name = mediaFromUrl)]
#[allow(clippy::needless_pass_by_value)]
pub fn media_from_url(
    media_kind_value: i32,
    url: &str,
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    let kind = media_kind(media_kind_value, "mediaFromUrl")?;
    let parts = handle_cffi::media_from_url(kind, url, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromUrl", &error))?;
    media_key("mediaFromUrl", kind, parts)
}

/// Media from the bytes of `file`, which the JavaScript host read: this
/// module has no file system of its own.
#[wasm_bindgen(js_name = mediaFromFileBytes)]
#[expect(clippy::needless_pass_by_value)]
pub fn media_from_file_bytes(
    media_kind_value: i32,
    file: &str,
    content: &[u8],
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    use base64::Engine as _;

    let kind = media_kind(media_kind_value, "mediaFromFileBytes")?;
    let base64 = base64::engine::general_purpose::STANDARD.encode(content);
    let parts = handle_cffi::media_from_file_content(kind, file, &base64, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromFileBytes", &error))?;
    media_key("mediaFromFileBytes", kind, parts)
}

/// Media from base64 content that was read from `file`. Reads nothing.
#[wasm_bindgen(js_name = mediaFromFileContent)]
#[expect(clippy::needless_pass_by_value)]
pub fn media_from_file_content(
    media_kind_value: i32,
    file: &str,
    base64: &str,
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    let kind = media_kind(media_kind_value, "mediaFromFileContent")?;
    let parts = handle_cffi::media_from_file_content(kind, file, base64, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromFileContent", &error))?;
    media_key("mediaFromFileContent", kind, parts)
}

#[wasm_bindgen(js_name = mediaFromBase64)]
#[allow(clippy::needless_pass_by_value)]
pub fn media_from_base64(
    media_kind_value: i32,
    base64: &str,
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    let kind = media_kind(media_kind_value, "mediaFromBase64")?;
    let parts = handle_cffi::media_from_base64(kind, base64, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromBase64", &error))?;
    media_key("mediaFromBase64", kind, parts)
}

#[wasm_bindgen(js_name = mediaUrl)]
pub fn media_url(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_cffi::media_url(key, handle_type).map_err(|error| handle_error("mediaUrl", &error))
}

/// The base name of the file the content was read from, if any.
#[wasm_bindgen(js_name = mediaName)]
pub fn media_name(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_cffi::media_name(key, handle_type).map_err(|error| handle_error("mediaName", &error))
}

#[wasm_bindgen(js_name = mediaBase64)]
pub fn media_base64(key: u64, handle_type: i32) -> Result<String, JsError> {
    handle_cffi::media_base64(key, handle_type).map_err(|error| handle_error("mediaBase64", &error))
}

#[wasm_bindgen(js_name = mediaMimeType)]
pub fn media_mime_type(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_cffi::media_mime_type(key, handle_type)
        .map_err(|error| handle_error("mediaMimeType", &error))
}
