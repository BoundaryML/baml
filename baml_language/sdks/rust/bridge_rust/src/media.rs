//! Opaque, handle-backed BAML media values used by generated Rust SDK signatures.
//!
//! Constructors allocate an engine handle immediately. Introspection goes through
//! the media C ABI, encoding clones the handle for wire ownership, and the final
//! Rust owner releases the original handle.

use std::{ffi::CString, sync::Arc};

use crate::{DecodeError, SdkError, baml_value::internal::__BamlValuePrivate, wire};

#[derive(Clone, Copy)]
enum SourceKind {
    Url,
    File,
    Base64,
}

struct MediaHandle {
    key: u64,
    handle_type: i32,
    #[cfg(test)]
    release: Option<Arc<dyn Fn(u64) + Send + Sync>>,
}

impl Drop for MediaHandle {
    fn drop(&mut self) {
        if self.key == 0 {
            return;
        }
        #[cfg(test)]
        if let Some(release) = &self.release {
            release(self.key);
            return;
        }
        if let Ok(api) = crate::capi::api() {
            // SAFETY: this is the one owned handle key retained by this value.
            #[expect(unsafe_code)]
            unsafe {
                (api.handle_release)(self.key);
            }
        }
    }
}

#[derive(Clone)]
struct MediaValue {
    handle: Arc<MediaHandle>,
}

impl MediaValue {
    fn create(
        kind: Kind,
        source_kind: SourceKind,
        source: String,
        mime_type: Option<String>,
    ) -> Result<Self, SdkError> {
        let source = CString::new(source)
            .map_err(|_| SdkError::new("media source contains an interior NUL byte"))?;
        let mime_type = mime_type
            .map(CString::new)
            .transpose()
            .map_err(|_| SdkError::new("media MIME type contains an interior NUL byte"))?;
        let api = crate::capi::api()?;
        let constructor = match source_kind {
            SourceKind::Url => api.media_from_url,
            SourceKind::File => api.media_from_file,
            SourceKind::Base64 => api.media_from_base64,
        };
        let mut key = 0_u64;
        let mut handle_type = 0_i32;
        let mime_type_ptr = mime_type
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        // SAFETY: both strings are NUL-terminated and live for the call, and
        // both output pointers refer to initialized stack storage.
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
        if status != 0 {
            return Err(status_error(source_kind.constructor_name(), status));
        }
        if key == 0 || handle_type != kind.handle_type() as i32 {
            if key != 0 {
                // SAFETY: a successful constructor transferred this key to us.
                #[expect(unsafe_code)]
                unsafe {
                    (api.handle_release)(key);
                }
            }
            return Err(SdkError::new(format!(
                "{} returned an invalid media handle",
                source_kind.constructor_name()
            )));
        }
        Ok(Self::adopt(key, handle_type))
    }

    fn adopt(key: u64, handle_type: i32) -> Self {
        Self {
            handle: Arc::new(MediaHandle {
                key,
                handle_type,
                #[cfg(test)]
                release: None,
            }),
        }
    }

    fn to_baml(&self) -> wire::InboundValue {
        let api = crate::capi::api().expect("BAML runtime must be loaded before encoding media");
        let mut cloned = 0_u64;
        // SAFETY: `cloned` is valid stack storage and the retained key remains
        // live for the duration of this call.
        #[expect(unsafe_code)]
        let status = unsafe { (api.handle_clone)(self.handle.key, &raw mut cloned) };
        assert_eq!(status, 0, "failed to clone a BAML media handle");
        assert_ne!(cloned, 0, "media handle clone returned a zero handle");
        wire::InboundValue {
            value_type: None,
            value: Some(wire::inbound_value::Value::Handle(wire::BamlHandle {
                key: cloned,
                handle_type: self.handle.handle_type,
            })),
        }
    }

    fn from_baml(
        value: wire::BamlOutboundValue,
        expected: Option<Kind>,
    ) -> Result<(Kind, Self), DecodeError> {
        let value = crate::decode::unwrap(value);
        let got = crate::baml_value::wire_variant_kind(&value);
        match value.value {
            Some(wire::baml_outbound_value::Value::MediaValue(media)) => {
                Self::from_descriptor(media, expected)
            }
            Some(wire::baml_outbound_value::Value::HandleValue(handle)) => {
                Self::from_handle(handle.key, handle.handle_type, expected)
            }
            Some(wire::baml_outbound_value::Value::ClassValue(class)) => {
                Self::from_wrapper_class(class, expected)
            }
            _ => Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got,
            }),
        }
    }

    fn from_descriptor(
        media: wire::BamlValueMedia,
        expected: Option<Kind>,
    ) -> Result<(Kind, Self), DecodeError> {
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
        let (source_kind, source) = match media.value {
            Some(wire::baml_value_media::Value::Url(value)) => (SourceKind::Url, value),
            Some(wire::baml_value_media::Value::File(value)) => (SourceKind::File, value),
            Some(wire::baml_value_media::Value::Base64(value)) => (SourceKind::Base64, value),
            None => {
                return Err(DecodeError::WrongType {
                    expected: expected.map_or("media", Kind::name),
                    got: "media without a source",
                });
            }
        };
        validate_descriptor(&source, media.mime_type.as_deref())?;
        let value = Self::create(kind, source_kind, source, media.mime_type).map_err(|_| {
            DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: "media descriptor rejected by the runtime",
            }
        })?;
        Ok((kind, value))
    }

    fn from_wrapper_class(
        class: wire::BamlValueClass,
        expected: Option<Kind>,
    ) -> Result<(Kind, Self), DecodeError> {
        let class_kind = Kind::from_wrapper_class(&class.name).ok_or(DecodeError::WrongType {
            expected: expected.map_or("media", Kind::name),
            got: "class",
        })?;
        if expected.is_some_and(|expected| expected != class_kind) {
            return Err(DecodeError::WrongType {
                expected: expected.map_or("media", Kind::name),
                got: class_kind.name(),
            });
        }
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
        Self::from_handle(handle.key, handle.handle_type, Some(class_kind))
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
        Ok((kind, Self::adopt(key, handle_type)))
    }

    fn optional_string(
        &self,
        api: &crate::capi::Api,
        accessor: MediaAccessor,
        context: &str,
    ) -> Result<Option<String>, SdkError> {
        let mut output = crate::capi::Buffer {
            ptr: std::ptr::null(),
            len: 0,
        };
        // SAFETY: `output` is valid stack storage and this value retains the
        // matching live handle for the duration of the call.
        #[expect(unsafe_code)]
        let status = unsafe { accessor(self.handle.key, self.handle.handle_type, &raw mut output) };
        if status != 0 {
            return Err(status_error(context, status));
        }
        api.take_optional_string(output)
    }

    fn url(&self) -> Result<Option<String>, SdkError> {
        let api = crate::capi::api()?;
        self.optional_string(api, api.media_url, "media.url")
    }

    fn file(&self) -> Result<Option<String>, SdkError> {
        let api = crate::capi::api()?;
        self.optional_string(api, api.media_file, "media.file")
    }

    fn base64(&self) -> Result<String, SdkError> {
        let api = crate::capi::api()?;
        Ok(self
            .optional_string(api, api.media_base64, "media.base64")?
            .unwrap_or_default())
    }

    fn mime_type(&self) -> Result<Option<String>, SdkError> {
        let api = crate::capi::api()?;
        self.optional_string(api, api.media_mime_type, "media.mime_type")
    }
}

impl PartialEq for MediaValue {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.handle, &other.handle)
    }
}

impl Eq for MediaValue {}

impl SourceKind {
    fn constructor_name(self) -> &'static str {
        match self {
            Self::Url => "media.from_url",
            Self::File => "media.from_file",
            Self::Base64 => "media.from_base64",
        }
    }
}

fn validate_descriptor(source: &str, mime_type: Option<&str>) -> Result<(), DecodeError> {
    CString::new(source).map_err(|_| DecodeError::InvalidMedia { field: "source" })?;
    if let Some(mime_type) = mime_type {
        CString::new(mime_type).map_err(|_| DecodeError::InvalidMedia { field: "MIME type" })?;
    }
    Ok(())
}

fn status_error(context: &str, status: u32) -> SdkError {
    let detail = match status {
        1 => "invalid handle",
        2 => "handle type mismatch",
        3 => "unsupported handle type",
        4 => "internal error",
        5 => "unexpected null pointer",
        _ => "unknown error",
    };
    SdkError::new(format!("{context}: {detail} (status {status})"))
}

type MediaAccessor = unsafe extern "C" fn(u64, i32, *mut crate::capi::Buffer) -> u32;

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
        #[doc = concat!("An opaque, handle-backed BAML `", stringify!($name), "` value.")]
        #[derive(Clone, PartialEq, Eq)]
        pub struct $name(MediaValue);

        impl $name {
            /// Create a URL-backed media handle.
            pub fn from_url(
                url: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::create($kind, SourceKind::Url, url.into(), mime_type).map(Self)
            }

            /// Create a file-backed media handle. The engine reads the file when the value is used.
            pub fn from_file(
                file: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::create($kind, SourceKind::File, file.into(), mime_type).map(Self)
            }

            /// Create a base64-backed media handle.
            pub fn from_base64(
                base64: impl Into<String>,
                mime_type: Option<String>,
            ) -> Result<Self, SdkError> {
                MediaValue::create($kind, SourceKind::Base64, base64.into(), mime_type).map(Self)
            }

            /// Return the URL for a URL-backed value.
            pub fn url(&self) -> Result<Option<String>, SdkError> {
                self.0.url()
            }

            /// Return the file path for a file-backed value.
            pub fn file(&self) -> Result<Option<String>, SdkError> {
                self.0.file()
            }

            /// Return the base64 payload, or an empty string when this value is not base64-backed.
            pub fn base64(&self) -> Result<String, SdkError> {
                self.0.base64()
            }

            /// Return the media value's MIME type.
            pub fn mime_type(&self) -> Result<Option<String>, SdkError> {
                self.0.mime_type()
            }
        }

        impl std::fmt::Debug for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter
                    .debug_struct(stringify!($name))
                    .finish_non_exhaustive()
            }
        }

        impl __BamlValuePrivate for $name {
            fn to_baml(&self) -> wire::InboundValue {
                self.0.to_baml()
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

/// An opaque media handle whose concrete kind is selected at runtime.
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
    use std::sync::atomic::{AtomicU64, Ordering};

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
    fn concrete_media_rejects_a_different_descriptor_kind_before_allocating() {
        let error = Image::from_baml(outbound(
            wire::MediaTypeEnum::Audio,
            wire::baml_value_media::Value::Url("https://example.com/audio.mp3".to_string()),
        ))
        .unwrap_err();
        assert_eq!(error.to_string(), "expected image, got wire variant audio");
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
    fn decoded_media_rejects_interior_nul_bytes_before_allocating() {
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
    fn constructors_reject_interior_nul_bytes_before_loading_the_runtime() {
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
    fn cloned_media_share_one_owned_handle() {
        let image = Image(MediaValue::adopt(
            0,
            wire::BamlHandleType::AdtMediaImage as i32,
        ));
        let cloned = image.clone();
        assert!(Arc::ptr_eq(&image.0.handle, &cloned.0.handle));
    }

    #[test]
    fn media_handle_drop_releases_exactly_its_key() {
        let released = Arc::new(AtomicU64::new(0));
        let capture = Arc::clone(&released);
        drop(MediaHandle {
            key: 73,
            handle_type: wire::BamlHandleType::AdtMediaImage as i32,
            release: Some(Arc::new(move |key| {
                capture.store(key, Ordering::SeqCst);
            })),
        });
        assert_eq!(released.load(Ordering::SeqCst), 73);
    }

    #[test]
    fn media_debug_output_is_opaque() {
        let image = Image(MediaValue::adopt(
            0,
            wire::BamlHandleType::AdtMediaImage as i32,
        ));
        assert_eq!(format!("{image:?}"), "Image { .. }");
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
