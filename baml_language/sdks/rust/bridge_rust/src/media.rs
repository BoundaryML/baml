//! Typed BAML media values used by generated Rust SDK signatures.
//!
//! Media stays descriptor-backed on the Rust side. Immediately before a call,
//! encoding turns the descriptor into an owned engine handle through the
//! existing media C ABI; the engine takes ownership of that wire handle.

use std::ffi::CString;

use crate::{DecodeError, SdkError, baml_value::internal::__BamlValuePrivate, wire};

#[derive(Clone, PartialEq, Eq)]
enum Source {
    Url(String),
    File(String),
    Base64(String),
}

#[derive(Clone, PartialEq, Eq)]
struct MediaValue {
    source: Source,
    mime_type: Option<String>,
}

impl MediaValue {
    fn new(source: Source, mime_type: Option<String>) -> Result<Self, SdkError> {
        Self::validate(&source, mime_type.as_deref()).map_err(|field| {
            SdkError::new(format!("media {field} contains an interior NUL byte"))
        })?;
        Ok(Self { source, mime_type })
    }

    fn decoded(source: Source, mime_type: Option<String>) -> Result<Self, DecodeError> {
        Self::validate(&source, mime_type.as_deref())
            .map_err(|field| DecodeError::InvalidMedia { field })?;
        Ok(Self { source, mime_type })
    }

    fn validate(source: &Source, mime_type: Option<&str>) -> Result<(), &'static str> {
        let source_value = match source {
            Source::Url(value) | Source::File(value) | Source::Base64(value) => value,
        };
        CString::new(source_value.as_str()).map_err(|_| "source")?;
        if let Some(mime_type) = mime_type {
            CString::new(mime_type).map_err(|_| "MIME type")?;
        }
        Ok(())
    }

    fn to_baml(&self, kind: Kind) -> wire::InboundValue {
        let source = match &self.source {
            Source::Url(value) | Source::File(value) | Source::Base64(value) => {
                CString::new(value.as_str()).expect("validated media source")
            }
        };
        let mime_type = self
            .mime_type
            .as_deref()
            .map(CString::new)
            .transpose()
            .expect("validated media MIME type");
        let mime_type_ptr = mime_type
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let api = crate::capi::api().expect("BAML runtime must be loaded before encoding media");
        let constructor = match self.source {
            Source::Url(_) => api.media_from_url,
            Source::File(_) => api.media_from_file,
            Source::Base64(_) => api.media_from_base64,
        };
        let mut key = 0_u64;
        let mut handle_type = 0_i32;
        // SAFETY: both strings are NUL-terminated and live for the duration of
        // the call; both output pointers refer to initialized stack storage.
        #[expect(unsafe_code)]
        let status = unsafe {
            constructor(
                kind.media_type() as i32,
                source.as_ptr(),
                mime_type_ptr,
                &raw mut key,
                &raw mut handle_type,
            )
        };
        // The safe constructors already enforce the engine's only descriptor
        // rejection (interior NUL), the kind is a fixed protocol value, and
        // `insert_entry` guarantees a matching type tag and nonzero key. A
        // failure here therefore means the exact-version C ABI violated its
        // contract, not a recoverable input error.
        assert_eq!(status, 0, "failed to construct a BAML media handle");
        assert_ne!(key, 0, "media constructor returned a zero handle");
        assert_eq!(
            handle_type,
            kind.handle_type() as i32,
            "media constructor returned the wrong handle type"
        );
        wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key,
                handle_type,
            })),
        }
    }

    fn from_baml(
        value: wire::BamlOutboundValue,
        expected: Option<Kind>,
    ) -> Result<(Kind, Self), DecodeError> {
        let value = crate::decode::unwrap(value);
        let got = crate::baml_value::wire_variant_kind(&value);
        let media = match value.value {
            Some(wire::baml_outbound_value::Value::MediaValue(media)) => media,
            Some(wire::baml_outbound_value::Value::HandleValue(handle)) => {
                return Self::from_handle(handle.key, handle.handle_type, expected);
            }
            Some(wire::baml_outbound_value::Value::ClassValue(class)) => {
                let class_kind =
                    Kind::from_wrapper_class(&class.name).ok_or(DecodeError::WrongType {
                        expected: expected.map_or("media", Kind::name),
                        got: "class",
                    })?;
                let handle = class
                    .fields
                    .into_iter()
                    .find(|field| field.key == "_data")
                    .and_then(|field| field.value)
                    .and_then(|value| match crate::decode::unwrap(value).value {
                        Some(wire::baml_outbound_value::Value::HandleValue(handle)) => Some(handle),
                        _ => None,
                    })
                    .ok_or(DecodeError::WrongType {
                        expected: expected.map_or("media", Kind::name),
                        got: "media class without a handle",
                    })?;
                if expected.is_some_and(|expected| expected != class_kind) {
                    return Err(DecodeError::WrongType {
                        expected: expected.map_or("media", Kind::name),
                        got: class_kind.name(),
                    });
                }
                let decoded = Self::from_handle(handle.key, handle.handle_type, Some(class_kind))?;
                return Ok(decoded);
            }
            _ => {
                return Err(DecodeError::WrongType {
                    expected: expected.map_or("media", Kind::name),
                    got,
                });
            }
        };
        let Some(kind) = Kind::from_media_type(media.media) else {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: "media with unknown kind",
            });
        };
        if expected.is_some_and(|expected| expected != kind) {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: kind.name(),
            });
        }
        let source = match media.value {
            Some(wire::baml_value_media::Value::Url(value)) => Source::Url(value),
            Some(wire::baml_value_media::Value::File(value)) => Source::File(value),
            Some(wire::baml_value_media::Value::Base64(value)) => Source::Base64(value),
            None => {
                return Err(DecodeError::WrongType {
                    expected: expected.map_or("media", Kind::name),
                    got: "media without a source",
                });
            }
        };
        Ok((kind, Self::decoded(source, media.mime_type)?))
    }

    fn from_handle(
        key: u64,
        handle_type: i32,
        expected: Option<Kind>,
    ) -> Result<(Kind, Self), DecodeError> {
        if key == 0 {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: "zero media handle",
            });
        }
        // Union decoders may try multiple concrete arms against the same
        // outbound handle. Take ownership only after this arm matches.
        let Some(kind) = Kind::from_handle_type(handle_type) else {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: "non-media handle",
            });
        };
        if expected.is_some_and(|expected| expected != kind) {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: kind.name(),
            });
        }
        let api = crate::capi::api().map_err(|_| DecodeError::WrongType {
            expected: expected.map_or("media", Kind::name),
            got: "media handle without a loaded runtime",
        })?;
        let guard = HandleGuard { api, key };
        let url = read_optional(api, api.media_url, key, handle_type)?;
        let file = read_optional(api, api.media_file, key, handle_type)?;
        let mime_type = read_optional(api, api.media_mime_type, key, handle_type)?;
        let source = if let Some(url) = url {
            Source::Url(url)
        } else if let Some(file) = file {
            Source::File(file)
        } else {
            let base64 = read_optional(api, api.media_base64, key, handle_type)?.ok_or(
                DecodeError::WrongType {
                    expected: expected.map_or("media", Kind::name),
                    got: "media handle without a source",
                },
            )?;
            Source::Base64(base64)
        };
        let value = Self::decoded(source, mime_type)?;
        drop(guard);
        Ok((kind, value))
    }

    fn url(&self) -> Option<&str> {
        match &self.source {
            Source::Url(value) => Some(value),
            Source::File(_) | Source::Base64(_) => None,
        }
    }

    fn file(&self) -> Option<&str> {
        match &self.source {
            Source::File(value) => Some(value),
            Source::Url(_) | Source::Base64(_) => None,
        }
    }

    fn base64(&self) -> Option<&str> {
        match &self.source {
            Source::Base64(value) => Some(value),
            Source::Url(_) | Source::File(_) => None,
        }
    }
}

impl std::fmt::Debug for MediaValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let source = match self.source {
            Source::Url(_) => "url",
            Source::File(_) => "file",
            Source::Base64(_) => "base64",
        };
        formatter
            .debug_struct("MediaValue")
            .field("source", &source)
            .field("mime_type", &self.mime_type)
            .finish()
    }
}

struct HandleGuard<'a> {
    api: &'a crate::capi::Api,
    key: u64,
}

impl Drop for HandleGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: the outbound handle key is owned by this decoder and must be
        // released exactly once after its descriptor has been copied out.
        #[expect(unsafe_code)]
        unsafe {
            (self.api.handle_release)(self.key);
        }
    }
}

type MediaAccessor = unsafe extern "C" fn(u64, i32, *mut crate::capi::Buffer) -> u32;

fn read_optional(
    api: &crate::capi::Api,
    accessor: MediaAccessor,
    key: u64,
    handle_type: i32,
) -> Result<Option<String>, DecodeError> {
    let mut output = crate::capi::Buffer {
        ptr: std::ptr::null(),
        len: 0,
    };
    // SAFETY: `output` is valid stack storage, and the handle key and type
    // came from the engine's outbound value.
    #[expect(unsafe_code)]
    let status = unsafe { accessor(key, handle_type, &raw mut output) };
    if status != 0 {
        return Err(DecodeError::WrongType {
            expected: "media",
            got: "invalid media handle",
        });
    }
    api.take_optional_string(output)
        .map_err(|_| DecodeError::WrongType {
            expected: "media",
            got: "invalid media descriptor",
        })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Image,
    Audio,
    Video,
    Pdf,
    Generic,
}

impl Kind {
    fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Audio => "audio",
            Self::Video => "video",
            Self::Pdf => "pdf",
            Self::Generic => "media",
        }
    }

    fn media_type(self) -> wire::MediaTypeEnum {
        match self {
            Self::Image => wire::MediaTypeEnum::Image,
            Self::Audio => wire::MediaTypeEnum::Audio,
            Self::Video => wire::MediaTypeEnum::Video,
            Self::Pdf => wire::MediaTypeEnum::Pdf,
            Self::Generic => wire::MediaTypeEnum::Other,
        }
    }

    fn type_kind(self) -> wire::BamlTyMediaKind {
        match self {
            Self::Image => wire::BamlTyMediaKind::Image,
            Self::Audio => wire::BamlTyMediaKind::Audio,
            Self::Video => wire::BamlTyMediaKind::Video,
            Self::Pdf => wire::BamlTyMediaKind::Pdf,
            Self::Generic => wire::BamlTyMediaKind::Generic,
        }
    }

    fn handle_type(self) -> wire::BamlHandleType {
        match self {
            Self::Image => wire::BamlHandleType::AdtMediaImage,
            Self::Audio => wire::BamlHandleType::AdtMediaAudio,
            Self::Video => wire::BamlHandleType::AdtMediaVideo,
            Self::Pdf => wire::BamlHandleType::AdtMediaPdf,
            Self::Generic => wire::BamlHandleType::AdtMediaGeneric,
        }
    }

    fn from_media_type(value: i32) -> Option<Self> {
        match wire::MediaTypeEnum::try_from(value).ok()? {
            wire::MediaTypeEnum::Image => Some(Self::Image),
            wire::MediaTypeEnum::Audio => Some(Self::Audio),
            wire::MediaTypeEnum::Video => Some(Self::Video),
            wire::MediaTypeEnum::Pdf => Some(Self::Pdf),
            wire::MediaTypeEnum::Other => Some(Self::Generic),
            wire::MediaTypeEnum::MediaTypeUnspecified => None,
        }
    }

    fn from_handle_type(value: i32) -> Option<Self> {
        match wire::BamlHandleType::try_from(value).ok()? {
            wire::BamlHandleType::AdtMediaImage => Some(Self::Image),
            wire::BamlHandleType::AdtMediaAudio => Some(Self::Audio),
            wire::BamlHandleType::AdtMediaVideo => Some(Self::Video),
            wire::BamlHandleType::AdtMediaPdf => Some(Self::Pdf),
            wire::BamlHandleType::AdtMediaGeneric => Some(Self::Generic),
            _ => None,
        }
    }

    fn from_wrapper_class(value: &str) -> Option<Self> {
        match value {
            "baml.media.Image" => Some(Self::Image),
            "baml.media.Audio" => Some(Self::Audio),
            "baml.media.Video" => Some(Self::Video),
            "baml.media.Pdf" => Some(Self::Pdf),
            _ => None,
        }
    }
}

fn media_ty(kind: wire::BamlTyMediaKind) -> wire::BamlTy {
    wire::BamlTy {
        ty: Some(wire::baml_ty::Ty::Media(wire::BamlTyMedia {
            kind: kind as i32,
        })),
    }
}

macro_rules! define_media {
    ($name:ident, $kind:expr) => {
        #[doc = concat!("A BAML `", stringify!($name), "` media value.")]
        #[derive(Clone, Debug, PartialEq, Eq)]
        pub struct $name(MediaValue);

        impl $name {
            /// Create a URL-backed media value.
            pub fn from_url(
                url: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::new(Source::Url(url.into()), mime_type).map(Self)
            }

            /// Create a file-backed media value. The engine reads the file when the value is used.
            pub fn from_file(
                file: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::new(Source::File(file.into()), mime_type).map(Self)
            }

            /// Create a base64-backed media value.
            pub fn from_base64(
                base64: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::new(Source::Base64(base64.into()), mime_type).map(Self)
            }

            /// Return the URL descriptor, when this value is URL-backed.
            pub fn url(&self) -> Option<&str> {
                self.0.url()
            }

            /// Return the file descriptor, when this value is file-backed.
            pub fn file(&self) -> Option<&str> {
                self.0.file()
            }

            /// Return the base64 descriptor, when this value is base64-backed.
            pub fn base64(&self) -> Option<&str> {
                self.0.base64()
            }

            /// Return the optional MIME type supplied with this value.
            pub fn mime_type(&self) -> Option<&str> {
                self.0.mime_type.as_deref()
            }
        }

        impl __BamlValuePrivate for $name {
            fn to_baml(&self) -> wire::InboundValue {
                self.0.to_baml($kind)
            }

            fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
                MediaValue::from_baml(value, Some($kind)).map(|(_, value)| Self(value))
            }

            fn baml_ty() -> wire::BamlTy {
                media_ty($kind.type_kind())
            }
        }
    };
}

define_media!(Image, Kind::Image);
define_media!(Audio, Kind::Audio);
define_media!(Video, Kind::Video);
define_media!(Pdf, Kind::Pdf);
define_media!(GenericMedia, Kind::Generic);

/// A media value whose concrete kind is selected at runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Media {
    /// An image value.
    Image(Image),
    /// An audio value.
    Audio(Audio),
    /// A video value.
    Video(Video),
    /// A PDF value.
    Pdf(Pdf),
    /// A media value without a more specific kind.
    Generic(GenericMedia),
}

impl __BamlValuePrivate for Media {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            Self::Image(value) => value.to_baml(),
            Self::Audio(value) => value.to_baml(),
            Self::Video(value) => value.to_baml(),
            Self::Pdf(value) => value.to_baml(),
            Self::Generic(value) => value.to_baml(),
        }
    }

    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let (kind, value) = MediaValue::from_baml(value, None)?;
        Ok(match kind {
            Kind::Image => Self::Image(Image(value)),
            Kind::Audio => Self::Audio(Audio(value)),
            Kind::Video => Self::Video(Video(value)),
            Kind::Pdf => Self::Pdf(Pdf(value)),
            Kind::Generic => Self::Generic(GenericMedia(value)),
        })
    }

    fn baml_ty() -> wire::BamlTy {
        media_ty(wire::BamlTyMediaKind::Generic)
    }
}

macro_rules! impl_media_from {
    ($name:ident, $variant:ident) => {
        impl From<$name> for Media {
            #[doc = concat!("Wrap a [`", stringify!($name), "`] as dynamic [`Media`].")]
            fn from(value: $name) -> Self {
                Self::$variant(value)
            }
        }
    };
}

impl_media_from!(Image, Image);
impl_media_from!(Audio, Audio);
impl_media_from!(Video, Video);
impl_media_from!(Pdf, Pdf);
impl_media_from!(GenericMedia, Generic);

#[cfg(test)]
mod tests {
    use super::*;

    fn outbound(
        kind: wire::MediaTypeEnum,
        value: wire::baml_value_media::Value,
    ) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue {
            value: Some(wire::baml_outbound_value::Value::MediaValue(
                wire::BamlValueMedia {
                    media: kind as i32,
                    mime_type: Some("image/png".to_string()),
                    value: Some(value),
                },
            )),
        }
    }

    #[test]
    fn image_decodes_without_loading_the_runtime() {
        let image = Image::from_baml(outbound(
            wire::MediaTypeEnum::Image,
            wire::baml_value_media::Value::Url("https://example.com/image.png".to_string()),
        ))
        .unwrap();
        assert_eq!(image.url(), Some("https://example.com/image.png"));
        assert_eq!(image.mime_type(), Some("image/png"));
    }

    #[test]
    fn image_decodes_file_and_base64_sources() {
        let file = Image::from_baml(outbound(
            wire::MediaTypeEnum::Image,
            wire::baml_value_media::Value::File("/tmp/image.png".to_string()),
        ))
        .unwrap();
        assert_eq!(file.file(), Some("/tmp/image.png"));
        assert_eq!(file.url(), None);
        assert_eq!(file.base64(), None);

        let base64 = Image::from_baml(outbound(
            wire::MediaTypeEnum::Image,
            wire::baml_value_media::Value::Base64("aGk=".to_string()),
        ))
        .unwrap();
        assert_eq!(base64.base64(), Some("aGk="));
        assert_eq!(base64.url(), None);
        assert_eq!(base64.file(), None);
    }

    #[test]
    fn concrete_media_rejects_a_different_kind() {
        let error = Image::from_baml(outbound(
            wire::MediaTypeEnum::Audio,
            wire::baml_value_media::Value::Url("https://example.com/audio.mp3".to_string()),
        ))
        .unwrap_err();
        assert_eq!(error.to_string(), "expected image, got wire variant audio");
    }

    #[test]
    fn dynamic_media_selects_the_variant_for_each_kind() {
        let url = || wire::baml_value_media::Value::Url("https://example.com/asset".to_string());
        assert!(matches!(
            Media::from_baml(outbound(wire::MediaTypeEnum::Image, url())).unwrap(),
            Media::Image(_)
        ));
        assert!(matches!(
            Media::from_baml(outbound(wire::MediaTypeEnum::Audio, url())).unwrap(),
            Media::Audio(_)
        ));
        assert!(matches!(
            Media::from_baml(outbound(wire::MediaTypeEnum::Video, url())).unwrap(),
            Media::Video(_)
        ));
        assert!(matches!(
            Media::from_baml(outbound(wire::MediaTypeEnum::Pdf, url())).unwrap(),
            Media::Pdf(_)
        ));
        assert!(matches!(
            Media::from_baml(outbound(wire::MediaTypeEnum::Other, url())).unwrap(),
            Media::Generic(_)
        ));
    }

    fn outbound_handle(kind: wire::BamlHandleType) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue {
            value: Some(wire::baml_outbound_value::Value::HandleValue(
                wire::BamlOutboundHandle {
                    key: 1,
                    handle_type: kind as i32,
                    ty: None,
                },
            )),
        }
    }

    fn outbound_wrapper(class: &str, kind: wire::BamlHandleType) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue {
            value: Some(wire::baml_outbound_value::Value::ClassValue(
                wire::BamlValueClass {
                    name: class.to_string(),
                    fields: vec![wire::BamlOutboundMapEntry {
                        key: "_data".to_string(),
                        value: Some(outbound_handle(kind)),
                    }],
                    type_args: Vec::new(),
                },
            )),
        }
    }

    #[test]
    fn mismatched_handle_kind_fails_before_loading_the_runtime() {
        let error =
            Image::from_baml(outbound_handle(wire::BamlHandleType::AdtMediaAudio)).unwrap_err();
        assert_eq!(error.to_string(), "expected image, got wire variant audio");
    }

    #[test]
    fn mismatched_wrapper_class_fails_before_decoding_its_handle() {
        let error = Image::from_baml(outbound_wrapper(
            "baml.media.Audio",
            wire::BamlHandleType::AdtMediaAudio,
        ))
        .unwrap_err();
        assert_eq!(error.to_string(), "expected image, got wire variant audio");
    }

    #[test]
    fn decoded_media_rejects_interior_nul_bytes() {
        let source_error = Image::from_baml(outbound(
            wire::MediaTypeEnum::Image,
            wire::baml_value_media::Value::Url("bad\0url".to_string()),
        ))
        .unwrap_err();
        assert_eq!(source_error, DecodeError::InvalidMedia { field: "source" });

        let mut value = outbound(
            wire::MediaTypeEnum::Image,
            wire::baml_value_media::Value::Url("https://example.com/image.png".to_string()),
        );
        let Some(wire::baml_outbound_value::Value::MediaValue(media)) = &mut value.value else {
            panic!("expected media value");
        };
        media.mime_type = Some("bad\0mime".to_string());
        assert_eq!(
            Image::from_baml(value).unwrap_err(),
            DecodeError::InvalidMedia { field: "MIME type" }
        );
    }

    #[test]
    fn constructors_reject_interior_nul_bytes() {
        assert_eq!(
            Image::from_file("bad\0path", None).unwrap_err().to_string(),
            "media source contains an interior NUL byte"
        );
        assert_eq!(
            Image::from_url("https://example.com", Some("bad\0mime".to_string()))
                .unwrap_err()
                .to_string(),
            "media MIME type contains an interior NUL byte"
        );
    }

    #[test]
    fn media_types_preserve_the_protocol_kind_ordering() {
        let image = <Image as __BamlValuePrivate>::baml_ty();
        let audio = <Audio as __BamlValuePrivate>::baml_ty();
        let video = <Video as __BamlValuePrivate>::baml_ty();
        let pdf = <Pdf as __BamlValuePrivate>::baml_ty();
        let generic = <GenericMedia as __BamlValuePrivate>::baml_ty();
        let kind = |ty: wire::BamlTy| match ty.ty.unwrap() {
            wire::baml_ty::Ty::Media(media) => media.kind,
            _ => panic!("expected media type"),
        };
        assert_eq!(kind(image), wire::BamlTyMediaKind::Image as i32);
        assert_eq!(kind(audio), wire::BamlTyMediaKind::Audio as i32);
        assert_eq!(kind(video), wire::BamlTyMediaKind::Video as i32);
        assert_eq!(kind(pdf), wire::BamlTyMediaKind::Pdf as i32);
        assert_eq!(kind(generic), wire::BamlTyMediaKind::Generic as i32);
    }
}
