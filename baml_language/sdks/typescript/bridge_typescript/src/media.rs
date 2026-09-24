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

use bex_project::MediaKind;
use bridge_cffi::handle::{self as handle_core, HandleError, HandleParts};
use bridge_ctypes::baml_bridge::cffi::BamlHandleType;
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::handle::{BamlHandle, handle_clone, status_to_napi};

type MediaConstructor =
    fn(MediaKind, &str, Option<&str>) -> std::result::Result<HandleParts, HandleError>;
type MediaAccessor<T> = fn(u64, i32) -> std::result::Result<T, HandleError>;

fn create_media(
    constructor: MediaConstructor,
    media_kind: MediaKind,
    value: String,
    mime_type: Option<String>,
    context: &str,
) -> napi::Result<(u64, i32)> {
    // Keep each language's existing argument error and field name.
    for (field, input) in [
        (context, value.as_str()),
        ("mimeType", mime_type.as_deref().unwrap_or_default()),
    ] {
        if input.contains('\0') {
            return Err(napi::Error::new(
                napi::Status::InvalidArg,
                format!("{field} cannot contain NUL"),
            ));
        }
    }
    let parts = constructor(media_kind, &value, mime_type.as_deref())
        .map_err(|error| status_to_napi(context, error.into()))?;
    Ok((parts.key, parts.handle_type))
}

fn media_string<T>(
    key: u64,
    handle_type: i32,
    accessor: MediaAccessor<T>,
    context: &str,
) -> napi::Result<T> {
    accessor(key, handle_type).map_err(|error| status_to_napi(context, error.into()))
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
                let (key, handle_type) = create_media(
                    handle_core::media_from_url,
                    $media_kind,
                    url,
                    mime_type,
                    "fromUrl",
                )?;
                Ok(Self { key, handle_type })
            }

            #[napi(factory, js_name = "fromFile")]
            pub fn from_file(file: String, mime_type: Option<String>) -> napi::Result<Self> {
                let (key, handle_type) = create_media(
                    handle_core::media_from_file,
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
                    handle_core::media_from_base64,
                    $media_kind,
                    base64,
                    mime_type,
                    "fromBase64",
                )?;
                Ok(Self { key, handle_type })
            }

            #[napi]
            pub fn url(&self) -> napi::Result<Option<String>> {
                media_string(self.key, self.handle_type, handle_core::media_url, "url")
            }

            #[napi]
            pub fn file(&self) -> napi::Result<Option<String>> {
                media_string(self.key, self.handle_type, handle_core::media_file, "file")
            }

            #[napi]
            pub fn base64(&self) -> napi::Result<String> {
                media_string(
                    self.key,
                    self.handle_type,
                    handle_core::media_base64,
                    "base64",
                )
            }

            #[napi(js_name = "mimeType")]
            pub fn mime_type(&self) -> napi::Result<Option<String>> {
                media_string(
                    self.key,
                    self.handle_type,
                    handle_core::media_mime_type,
                    "mimeType",
                )
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
                let _ = handle_core::release_handle(self.key);
                Ok(())
            }
        }
    };
}

define_media_napi_class!(
    BamlImage,
    MediaKind::Image,
    BamlHandleType::AdtMediaImage as u64
);
define_media_napi_class!(
    BamlAudio,
    MediaKind::Audio,
    BamlHandleType::AdtMediaAudio as u64
);
define_media_napi_class!(
    BamlVideo,
    MediaKind::Video,
    BamlHandleType::AdtMediaVideo as u64
);
define_media_napi_class!(BamlPdf, MediaKind::Pdf, BamlHandleType::AdtMediaPdf as u64);
