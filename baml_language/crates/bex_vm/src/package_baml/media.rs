use baml_type::MediaKind;

use super::{
    BamlClassMediaAudio, BamlClassMediaImage, BamlClassMediaPdf, BamlClassMediaVideo,
    BamlNamespaceMedia, PackageBamlImpl, copy, view,
};
use crate::BexVm;

// All media accessors and constructors live on `baml_builtins2::MediaValue`
// itself (see `crates/baml_builtins2/src/media.rs`); the per-kind impls
// here are thin wrappers — `_data` getter on the `view::media::*` ↔ trait
// method on `MediaValue`, plus `from_*` constructors that wrap the
// resulting `Arc<MediaValue>` in the kind-specific `copy::media::*` shell.

// =========================================================================
// Pdf
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaPdf for PackageBamlImpl {
    fn url(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<baml_builtins2::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn file(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<baml_builtins2::MediaValue>(vm)
            .file()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> bex_str::BexStr {
        bex_str::BexStr::from(pdf._data::<baml_builtins2::MediaValue>(vm).base64())
    }

    fn mime_type(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<baml_builtins2::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_url(
                MediaKind::Pdf,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file(file: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_file(
                MediaKind::Pdf,
                file.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_base64(
                MediaKind::Pdf,
                base64.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }
}

// =========================================================================
// Audio
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaAudio for PackageBamlImpl {
    fn url(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio
            ._data::<baml_builtins2::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn file(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio
            ._data::<baml_builtins2::MediaValue>(vm)
            .file()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, audio: &view::media::Audio<'_>) -> bex_str::BexStr {
        bex_str::BexStr::from(audio._data::<baml_builtins2::MediaValue>(vm).base64())
    }

    fn mime_type(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio
            ._data::<baml_builtins2::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_url(
                MediaKind::Audio,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file(
        file: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_file(
                MediaKind::Audio,
                file.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_base64(
                MediaKind::Audio,
                base64.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }
}

// =========================================================================
// Video
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaVideo for PackageBamlImpl {
    fn url(vm: &BexVm, video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video
            ._data::<baml_builtins2::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn file(vm: &BexVm, video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video
            ._data::<baml_builtins2::MediaValue>(vm)
            .file()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, video: &view::media::Video<'_>) -> bex_str::BexStr {
        bex_str::BexStr::from(video._data::<baml_builtins2::MediaValue>(vm).base64())
    }

    fn mime_type(vm: &BexVm, video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video
            ._data::<baml_builtins2::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_url(
                MediaKind::Video,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file(
        file: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_file(
                MediaKind::Video,
                file.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_base64(
                MediaKind::Video,
                base64.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }
}

// =========================================================================
// Image
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaImage for PackageBamlImpl {
    fn url(vm: &BexVm, image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image
            ._data::<baml_builtins2::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn file(vm: &BexVm, image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image
            ._data::<baml_builtins2::MediaValue>(vm)
            .file()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, image: &view::media::Image<'_>) -> bex_str::BexStr {
        bex_str::BexStr::from(image._data::<baml_builtins2::MediaValue>(vm).base64())
    }

    fn mime_type(vm: &BexVm, image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image
            ._data::<baml_builtins2::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_url(
                MediaKind::Image,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file(
        file: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_file(
                MediaKind::Image,
                file.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_base64(
                MediaKind::Image,
                base64.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }
}

// Namespace aggregator (only default dispatch methods, no required methods)
impl BamlNamespaceMedia for PackageBamlImpl {}
