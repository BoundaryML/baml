//! Media types (Image, Audio, Pdf, Video)
//!
//! These wrap FFI pointers to media objects managed by the BAML runtime.

use std::ffi::c_void;

use super::{RawObject, RawObjectTrait};
use crate::{baml_unreachable, proto::baml_cffi_v1::BamlObjectType};

// =============================================================================
// Media type macro - generates Image, Audio, Pdf, Video
// Uses define_raw_object_wrapper for base struct + from_raw + RawObjectTrait
// =============================================================================

macro_rules! define_media_type {
    (
        $(#[$meta:meta])*
        $name:ident => $object_type:ident
    ) => {
        // Use the base wrapper for struct, from_raw, and RawObjectTrait
        define_raw_object_wrapper! {
            $(#[$meta])*
            $name => $object_type
        }

        // Add media-specific constructors and methods
        impl $name {
            /// Create from a URL
            pub fn from_url(runtime: *const c_void, url: &str, mime_type: Option<&str>) -> Self {
                let raw = RawObject::new(
                    runtime,
                    BamlObjectType::$object_type,
                    (("url", url), ("mime_type", mime_type)),
                )
                .unwrap_or_else(|e| baml_unreachable!("Failed to create {} from URL: {}", stringify!($name), e));
                Self { raw }
            }

            /// Create from base64-encoded data
            pub fn from_base64(runtime: *const c_void, base64: &str, mime_type: Option<&str>) -> Self {
                let raw = RawObject::new(
                    runtime,
                    BamlObjectType::$object_type,
                    (("base64", base64), ("mime_type", mime_type)),
                )
                .unwrap_or_else(|e| baml_unreachable!("Failed to create {} from base64: {}", stringify!($name), e));
                Self { raw }
            }

            /// Get the MIME type (if known)
            pub fn mime_type(&self) -> Option<String> {
                self.raw.call_method("mime_type", ())
            }

            /// Check if this is a URL reference
            pub fn is_url(&self) -> bool {
                self.raw.call_method("is_url", ())
            }

            /// Check if this is base64 encoded
            pub fn is_base64(&self) -> bool {
                self.raw.call_method("is_base64", ())
            }

            /// Get as URL (if applicable)
            pub fn as_url(&self) -> Option<String> {
                self.raw.call_method("as_url", ())
            }

            /// Get as base64 (if applicable)
            pub fn as_base64(&self) -> Option<String> {
                self.raw.call_method("as_base64", ())
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($name)).finish_non_exhaustive()
            }
        }

        impl Default for $name {
            fn default() -> Self {
                unreachable!("Media types cannot be default-constructed; they require a runtime")
            }
        }

        impl $crate::BamlDecode for $name {
            fn baml_decode(
                _holder: &$crate::__internal::CffiValueHolder
            ) -> ::core::result::Result<Self, $crate::BamlError> {
                // Media types come as ObjectValue which requires runtime context to decode.
                // This path shouldn't be hit in normal usage - media decoding goes through
                // the runtime's object handle mechanism instead.
                Err($crate::BamlError::internal(
                    concat!("Cannot decode ", stringify!($name), " without runtime context")
                ))
            }
        }
    };
}

// =============================================================================
// Media type definitions
// =============================================================================

define_media_type! {
    /// Image media type
    Image => ObjectMediaImage
}

define_media_type! {
    /// Audio media type
    Audio => ObjectMediaAudio
}

define_media_type! {
    /// PDF media type
    Pdf => ObjectMediaPdf
}

define_media_type! {
    /// Video media type
    Video => ObjectMediaVideo
}
