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
//! `@boundaryml/baml-core` under aliases (`BamlImage as Image`, etc.). See
//! `00a-spec-codegen-mappings.md` "Stdlib Re-Exports".
//!
//! The key is stored inline as a raw `(key, handle_type)` pair (rather than a
//! `napi::Reference<BamlHandle>`) to avoid napi reference-lifetime complexity;
//! `ObjectFinalize` on the media class releases the table row. `_fromHandle`
//! and `_toHandle` clone the table row so the input/output `BamlHandle` and the
//! media wrapper own independent references.

use std::sync::Arc;

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_core::cffi::BamlHandleType};
use napi::bindgen_prelude::*;
use napi_derive::napi;

use crate::handle::BamlHandle;

macro_rules! define_media_napi_class {
    ($name:ident, $kind:expr, $expected_ht:expr) => {
        #[napi(custom_finalize)]
        pub struct $name {
            key: u64,
            handle_type: i32,
        }

        #[napi]
        impl $name {
            #[napi(factory, js_name = "fromUrl")]
            pub fn from_url(url: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_url($kind, &url, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self {
                    key,
                    handle_type: $expected_ht as i32,
                }
            }

            #[napi(factory, js_name = "fromFile")]
            pub fn from_file(file: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_file($kind, &file, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self {
                    key,
                    handle_type: $expected_ht as i32,
                }
            }

            #[napi(factory, js_name = "fromBase64")]
            pub fn from_base64(base64: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_base64($kind, &base64, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self {
                    key,
                    handle_type: $expected_ht as i32,
                }
            }

            #[napi]
            pub fn url(&self) -> napi::Result<Option<String>> {
                Ok(self.media_arc()?.url())
            }

            #[napi]
            pub fn file(&self) -> napi::Result<Option<String>> {
                Ok(self.media_arc()?.file())
            }

            #[napi]
            pub fn base64(&self) -> napi::Result<String> {
                Ok(self.media_arc()?.base64())
            }

            #[napi(js_name = "mimeType")]
            pub fn mime_type(&self) -> napi::Result<Option<String>> {
                Ok(self.media_arc()?.mime_type())
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
                let new_key = HANDLE_TABLE.clone_handle(handle.key_u64()).ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::GenericFailure,
                        "media handle key no longer valid",
                    )
                })?;
                Ok(Self {
                    key: new_key,
                    handle_type: $expected_ht as i32,
                })
            }

            /// Internal: produce a fresh `BamlHandle` pointing at the same
            /// table row (cloned). Used by inbound encode.
            #[napi(js_name = "_toHandle")]
            pub fn to_handle(&self) -> napi::Result<BamlHandle> {
                let new_key = HANDLE_TABLE.clone_handle(self.key).ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::GenericFailure,
                        "media handle key no longer valid",
                    )
                })?;
                Ok(BamlHandle::from_parts(new_key, self.handle_type))
            }
        }

        impl $name {
            fn media_arc(&self) -> napi::Result<Arc<MediaValue>> {
                let entry = HANDLE_TABLE.resolve(self.key).ok_or_else(|| {
                    napi::Error::new(
                        napi::Status::GenericFailure,
                        format!("media handle key {} no longer in HANDLE_TABLE", self.key),
                    )
                })?;
                match &*entry {
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc)) if arc.kind == $kind => {
                        Ok(Arc::clone(arc))
                    }
                    _ => Err(napi::Error::new(
                        napi::Status::GenericFailure,
                        "media handle no longer points to a media value of the expected kind",
                    )),
                }
            }
        }

        impl ObjectFinalize for $name {
            fn finalize(self, _env: Env) -> napi::Result<()> {
                HANDLE_TABLE.release(self.key);
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
