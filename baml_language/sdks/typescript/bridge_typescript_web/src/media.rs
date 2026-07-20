//! Raw media handle bindings.

use bex_project::MediaKind;
use bridge_cffi::handle::{self as handle_core, HandleError, HandleParts};
use bridge_ctypes::baml_bridge::cffi::{BamlHandleType, MediaTypeEnum};
use wasm_bindgen::prelude::*;

use crate::errors::{handle_error, unexpected_handle_type};

fn media_kind(media_kind: i32, operation: &'static str) -> Result<MediaKind, JsError> {
    let media_kind = MediaTypeEnum::try_from(media_kind)
        .map_err(|_| handle_error(operation, &HandleError::UnsupportedHandleType))?;
    match media_kind {
        MediaTypeEnum::Image => Ok(MediaKind::Image),
        MediaTypeEnum::Audio => Ok(MediaKind::Audio),
        MediaTypeEnum::Pdf => Ok(MediaKind::Pdf),
        MediaTypeEnum::Video => Ok(MediaKind::Video),
        MediaTypeEnum::Other => Ok(MediaKind::Generic),
        MediaTypeEnum::MediaTypeUnspecified => {
            Err(handle_error(operation, &HandleError::UnsupportedHandleType))
        }
    }
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
    let parts = handle_core::media_from_url(kind, url, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromUrl", &error))?;
    media_key("mediaFromUrl", kind, parts)
}

#[wasm_bindgen(js_name = mediaFromFile)]
#[allow(clippy::needless_pass_by_value)]
pub fn media_from_file(
    media_kind_value: i32,
    file: &str,
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    let kind = media_kind(media_kind_value, "mediaFromFile")?;
    let parts = handle_core::media_from_file(kind, file, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromFile", &error))?;
    media_key("mediaFromFile", kind, parts)
}

#[wasm_bindgen(js_name = mediaFromBase64)]
#[allow(clippy::needless_pass_by_value)]
pub fn media_from_base64(
    media_kind_value: i32,
    base64: &str,
    mime_type: Option<String>,
) -> Result<u64, JsError> {
    let kind = media_kind(media_kind_value, "mediaFromBase64")?;
    let parts = handle_core::media_from_base64(kind, base64, mime_type.as_deref())
        .map_err(|error| handle_error("mediaFromBase64", &error))?;
    media_key("mediaFromBase64", kind, parts)
}

#[wasm_bindgen(js_name = mediaUrl)]
pub fn media_url(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_core::media_url(key, handle_type).map_err(|error| handle_error("mediaUrl", &error))
}

#[wasm_bindgen(js_name = mediaFile)]
pub fn media_file(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_core::media_file(key, handle_type).map_err(|error| handle_error("mediaFile", &error))
}

#[wasm_bindgen(js_name = mediaBase64)]
pub fn media_base64(key: u64, handle_type: i32) -> Result<String, JsError> {
    handle_core::media_base64(key, handle_type).map_err(|error| handle_error("mediaBase64", &error))
}

#[wasm_bindgen(js_name = mediaMimeType)]
pub fn media_mime_type(key: u64, handle_type: i32) -> Result<Option<String>, JsError> {
    handle_core::media_mime_type(key, handle_type)
        .map_err(|error| handle_error("mediaMimeType", &error))
}
