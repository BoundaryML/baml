//! Native helper functions for TypeScript BAML media wrappers.
//!
//! The public `BamlImage`/`BamlAudio`/`BamlVideo`/`BamlPdf` classes live in
//! `typescript_src/media.ts`. This module keeps only the low-level bridge
//! operations that need access to the shared CFFI handle table.

use bex_project::MediaKind;
use bridge_ctypes::baml_core::cffi::BamlHandleType;
use napi_derive::napi;

use crate::handle::BamlHandle;

fn handle_status_err(context: &str, status: bridge_cffi::BamlCffiStatus) -> napi::Error {
    let reason = match status {
        bridge_cffi::BAML_HANDLE_INVALID_HANDLE => "invalid handle",
        bridge_cffi::BAML_HANDLE_TYPE_MISMATCH => "handle type mismatch",
        bridge_cffi::BAML_HANDLE_UNSUPPORTED_HANDLE_TYPE => "unsupported handle type",
        bridge_cffi::BAML_HANDLE_INTERNAL_ERROR => "internal handle error",
        _ => "unknown handle error",
    };
    napi::Error::new(napi::Status::GenericFailure, format!("{context}: {reason}"))
}

fn media_kind_from_handle_type(handle_type: i32) -> napi::Result<MediaKind> {
    match BamlHandleType::try_from(handle_type) {
        Ok(BamlHandleType::AdtMediaImage) => Ok(MediaKind::Image),
        Ok(BamlHandleType::AdtMediaAudio) => Ok(MediaKind::Audio),
        Ok(BamlHandleType::AdtMediaVideo) => Ok(MediaKind::Video),
        Ok(BamlHandleType::AdtMediaPdf) => Ok(MediaKind::Pdf),
        Ok(_) => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("handleType {handle_type} is not a typed BAML media handle"),
        )),
        Err(_) => Err(napi::Error::new(
            napi::Status::InvalidArg,
            format!("unsupported BAML media handleType {handle_type}"),
        )),
    }
}

fn media_value(
    handle: &BamlHandle,
    expected_handle_type: i32,
    context: &str,
) -> napi::Result<std::sync::Arc<bex_project::MediaValue>> {
    let expected_kind = media_kind_from_handle_type(expected_handle_type)?;
    bridge_cffi::media_value_impl(handle.key_u64(), expected_handle_type, expected_kind)
        .map_err(|status| handle_status_err(context, status))
}

#[napi(js_name = "mediaFromUrl")]
pub fn media_from_url(
    media_handle_type: i32,
    url: String,
    mime_type: Option<String>,
) -> napi::Result<BamlHandle> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) = bridge_cffi::media_from_url_impl(kind, &url, mime_type.as_deref());
    Ok(BamlHandle::from_parts(key, handle_type))
}

#[napi(js_name = "mediaFromFile")]
pub fn media_from_file(
    media_handle_type: i32,
    file: String,
    mime_type: Option<String>,
) -> napi::Result<BamlHandle> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) = bridge_cffi::media_from_file_impl(kind, &file, mime_type.as_deref());
    Ok(BamlHandle::from_parts(key, handle_type))
}

#[napi(js_name = "mediaFromBase64")]
pub fn media_from_base64(
    media_handle_type: i32,
    base64: String,
    mime_type: Option<String>,
) -> napi::Result<BamlHandle> {
    let kind = media_kind_from_handle_type(media_handle_type)?;
    let (key, handle_type) =
        bridge_cffi::media_from_base64_impl(kind, &base64, mime_type.as_deref());
    Ok(BamlHandle::from_parts(key, handle_type))
}

#[napi(js_name = "mediaUrl")]
pub fn media_url(handle: &BamlHandle, expected_handle_type: i32) -> napi::Result<Option<String>> {
    Ok(media_value(handle, expected_handle_type, "mediaUrl")?.url())
}

#[napi(js_name = "mediaFile")]
pub fn media_file(handle: &BamlHandle, expected_handle_type: i32) -> napi::Result<Option<String>> {
    Ok(media_value(handle, expected_handle_type, "mediaFile")?.file())
}

#[napi(js_name = "mediaBase64")]
pub fn media_base64(handle: &BamlHandle, expected_handle_type: i32) -> napi::Result<String> {
    Ok(media_value(handle, expected_handle_type, "mediaBase64")?.base64())
}

#[napi(js_name = "mediaMimeType")]
pub fn media_mime_type(
    handle: &BamlHandle,
    expected_handle_type: i32,
) -> napi::Result<Option<String>> {
    Ok(media_value(handle, expected_handle_type, "mediaMimeType")?.mime_type())
}

#[napi(js_name = "mediaValidate")]
pub fn media_validate(handle: &BamlHandle, expected_handle_type: i32) -> napi::Result<()> {
    media_value(handle, expected_handle_type, "mediaValidate")?;
    Ok(())
}
