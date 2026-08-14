//! napi types for BAML media (`baml.media.{Image,Video,Audio,Pdf}`).
//!
//! Mirrors `bridge_python/src/media.rs`. Each class wraps a `HANDLE_TABLE`
//! row that is a `CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc))`.
//! Static constructors (`fromUrl`/`fromFile`/`fromBase64`) and accessors
//! (`url`/`file`/`base64`/`mimeType`) dispatch natively here instead of
//! round-tripping through the BAML engine.
//!
//! These four are runtime-owned stdlib value classes: codegen does NOT emit
//! a structural class body for them — it re-exports them from
//! `@boundaryml/baml-bridge` under aliases (`BamlImage as Image`, etc.). See
//! `00a-spec-codegen-mappings.md` "Stdlib Re-Exports".
//!
//! The key is stored inline as a raw `(key, handle_type)` pair (rather than a
//! `napi::Reference<BamlHandle>`) to avoid napi reference-lifetime complexity;
//! `ObjectFinalize` on the media class releases the table row. `_fromHandle`
//! and `_toHandle` clone the table row so the input/output `BamlHandle` and the
//! media wrapper own independent references.

use std::{
    ffi::{CString, c_char},
    ptr, slice,
};

use bridge_cffi::{
    BamlCffiStatus, Buffer, baml_handle_release, baml_media_base64, baml_media_file,
    baml_media_from_base64, baml_media_from_file, baml_media_from_url, baml_media_mime_type,
    baml_media_url, free_buffer,
};
use bridge_ctypes::baml_bridge::cffi::{BamlHandleType, MediaTypeEnum};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::handle::{BamlHandle, handle_clone, status_to_napi};

type MediaConstructor =
    unsafe extern "C" fn(i32, *const c_char, *const c_char, *mut u64, *mut i32) -> BamlCffiStatus;
type MediaAccessor = unsafe extern "C" fn(u64, i32, *mut Buffer) -> BamlCffiStatus;

fn c_string(value: String, field: &str) -> napi::Result<CString> {
    CString::new(value).map_err(|_| {
        napi::Error::new(
            napi::Status::InvalidArg,
            format!("{field} cannot contain NUL"),
        )
    })
}

fn create_media(
    constructor: MediaConstructor,
    media_kind: MediaTypeEnum,
    value: String,
    mime_type: Option<String>,
    context: &str,
) -> napi::Result<(u64, i32)> {
    let value = c_string(value, context)?;
    let mime_type = mime_type.map(|s| c_string(s, "mimeType")).transpose()?;
    let mime_ptr = mime_type
        .as_ref()
        .map(|s| s.as_ptr())
        .unwrap_or_else(ptr::null);
    let mut key = 0;
    let mut handle_type = 0;
    match unsafe {
        constructor(
            media_kind as i32,
            value.as_ptr(),
            mime_ptr,
            &mut key,
            &mut handle_type,
        )
    } {
        BamlCffiStatus::Ok => Ok((key, handle_type)),
        status => Err(status_to_napi(context, status)),
    }
}

fn buffer_to_option(buf: Buffer) -> napi::Result<Option<String>> {
    if buf.ptr.is_null() {
        return Ok(None);
    }
    let bytes = unsafe { slice::from_raw_parts(buf.ptr as *const u8, buf.len) };
    let owned = bytes.to_vec();
    free_buffer(buf);
    String::from_utf8(owned)
        .map(Some)
        .map_err(|_| napi::Error::new(napi::Status::GenericFailure, "invalid UTF-8 from media"))
}

fn media_string_option(
    key: u64,
    handle_type: i32,
    accessor: MediaAccessor,
    context: &str,
) -> napi::Result<Option<String>> {
    let mut out = Buffer {
        ptr: ptr::null(),
        len: 0,
    };
    match unsafe { accessor(key, handle_type, &mut out) } {
        BamlCffiStatus::Ok => buffer_to_option(out),
        status => Err(status_to_napi(context, status)),
    }
}

fn media_string(
    key: u64,
    handle_type: i32,
    accessor: MediaAccessor,
    context: &str,
) -> napi::Result<String> {
    Ok(media_string_option(key, handle_type, accessor, context)?.unwrap_or_default())
}

macro_rules! define_media_napi_class {
    ($name:ident, $media_kind:expr, $expected_ht:expr) => {
        #[napi(custom_finalize)]
        pub struct $name {
            key: u64,
            handle_type: i32,
        }

        #[napi]
        impl $name {
            #[napi(factory, js_name = "fromUrl")]
            pub fn from_url(url: String, mime_type: Option<String>) -> napi::Result<Self> {
                let (key, handle_type) =
                    create_media(baml_media_from_url, $media_kind, url, mime_type, "fromUrl")?;
                Ok(Self { key, handle_type })
            }

            #[napi(factory, js_name = "fromFile")]
            pub fn from_file(file: String, mime_type: Option<String>) -> napi::Result<Self> {
                let (key, handle_type) = create_media(
                    baml_media_from_file,
                    $media_kind,
                    file,
                    mime_type,
                    "fromFile",
                )?;
                Ok(Self { key, handle_type })
            }

            #[napi(factory, js_name = "fromBase64")]
            pub fn from_base64(base64: String, mime_type: Option<String>) -> napi::Result<Self> {
                let (key, handle_type) = create_media(
                    baml_media_from_base64,
                    $media_kind,
                    base64,
                    mime_type,
                    "fromBase64",
                )?;
                Ok(Self { key, handle_type })
            }

            #[napi]
            pub fn url(&self) -> napi::Result<Option<String>> {
                media_string_option(self.key, self.handle_type, baml_media_url, "url")
            }

            #[napi]
            pub fn file(&self) -> napi::Result<Option<String>> {
                media_string_option(self.key, self.handle_type, baml_media_file, "file")
            }

            #[napi]
            pub fn base64(&self) -> napi::Result<String> {
                media_string(self.key, self.handle_type, baml_media_base64, "base64")
            }

            #[napi(js_name = "mimeType")]
            pub fn mime_type(&self) -> napi::Result<Option<String>> {
                media_string_option(self.key, self.handle_type, baml_media_mime_type, "mimeType")
            }

            /// Internal: build from an existing `BamlHandle`. Used by proto
            /// decode. Validates the handle's `handle_type` tag matches the
            /// expected media kind, then clones the table row so the input
            /// handle stays usable.
            #[napi(factory, js_name = "_fromHandle")]
            pub fn from_handle(handle: &BamlHandle) -> napi::Result<Self> {
                if handle.handle_type() != $expected_ht as i32 {
                    return Err(napi::Error::new(
                        napi::Status::InvalidArg,
                        format!(
                            "BamlHandle.handleType is {}, expected {} for {}",
                            handle.handle_type(),
                            $expected_ht as i32,
                            stringify!($name),
                        ),
                    ));
                }
                let new_key = handle_clone(handle.key_u64(), "_fromHandle")?;
                Ok(Self {
                    key: new_key,
                    handle_type: $expected_ht as i32,
                })
            }

            /// Internal: produce a fresh `BamlHandle` pointing at the same
            /// table row (cloned). Used by inbound encode.
            #[napi(js_name = "_toHandle")]
            pub fn to_handle(&self) -> napi::Result<BamlHandle> {
                let new_key = handle_clone(self.key, "_toHandle")?;
                Ok(BamlHandle::from_parts(new_key, self.handle_type))
            }
        }

        impl ObjectFinalize for $name {
            fn finalize(self, _env: Env) -> napi::Result<()> {
                let _ = unsafe { baml_handle_release(self.key) };
                Ok(())
            }
        }
    };
}

define_media_napi_class!(
    BamlImage,
    MediaTypeEnum::Image,
    BamlHandleType::AdtMediaImage as u64
);
define_media_napi_class!(
    BamlAudio,
    MediaTypeEnum::Audio,
    BamlHandleType::AdtMediaAudio as u64
);
define_media_napi_class!(
    BamlVideo,
    MediaTypeEnum::Video,
    BamlHandleType::AdtMediaVideo as u64
);
define_media_napi_class!(
    BamlPdf,
    MediaTypeEnum::Pdf,
    BamlHandleType::AdtMediaPdf as u64
);
