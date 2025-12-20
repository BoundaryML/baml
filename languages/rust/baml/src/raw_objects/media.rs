//! Media types (Image, Audio, Pdf, Video)
//!
//! These wrap FFI pointers to media objects managed by the BAML runtime.
//! Full implementation in Phase 11.

use crate::proto::baml_cffi_v1::BamlObjectType;

use super::{define_raw_object_wrapper, RawObject, RawObjectTrait};

define_raw_object_wrapper! {
    /// Image media type
    Image => ObjectMediaImage
}

define_raw_object_wrapper! {
    /// Audio media type
    Audio => ObjectMediaAudio
}

define_raw_object_wrapper! {
    /// PDF media type
    Pdf => ObjectMediaPdf
}

define_raw_object_wrapper! {
    /// Video media type
    Video => ObjectMediaVideo
}
