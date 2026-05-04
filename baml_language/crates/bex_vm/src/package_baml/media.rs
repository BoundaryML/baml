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
    fn url(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<baml_builtins2::MediaValue>(vm).url()
    }

    fn file(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<baml_builtins2::MediaValue>(vm).file()
    }

    fn base64(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> String {
        pdf._data::<baml_builtins2::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<baml_builtins2::MediaValue>(vm).mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_url(MediaKind::Pdf, url, mime_type),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_file(MediaKind::Pdf, file, mime_type),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: baml_builtins2::MediaValue::from_base64(MediaKind::Pdf, base64, mime_type),
        }
    }
}

// =========================================================================
// Audio
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaAudio for PackageBamlImpl {
    fn url(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio._data::<baml_builtins2::MediaValue>(vm).url()
    }

    fn file(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio._data::<baml_builtins2::MediaValue>(vm).file()
    }

    fn base64(vm: &BexVm, audio: &view::media::Audio<'_>) -> String {
        audio._data::<baml_builtins2::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio._data::<baml_builtins2::MediaValue>(vm).mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_url(MediaKind::Audio, url, mime_type),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_file(MediaKind::Audio, file, mime_type),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: baml_builtins2::MediaValue::from_base64(MediaKind::Audio, base64, mime_type),
        }
    }
}

// =========================================================================
// Video
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaVideo for PackageBamlImpl {
    fn url(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video._data::<baml_builtins2::MediaValue>(vm).url()
    }

    fn file(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video._data::<baml_builtins2::MediaValue>(vm).file()
    }

    fn base64(vm: &BexVm, video: &view::media::Video<'_>) -> String {
        video._data::<baml_builtins2::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video._data::<baml_builtins2::MediaValue>(vm).mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_url(MediaKind::Video, url, mime_type),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_file(MediaKind::Video, file, mime_type),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: baml_builtins2::MediaValue::from_base64(MediaKind::Video, base64, mime_type),
        }
    }
}

// =========================================================================
// Image
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaImage for PackageBamlImpl {
    fn url(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image._data::<baml_builtins2::MediaValue>(vm).url()
    }

    fn file(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image._data::<baml_builtins2::MediaValue>(vm).file()
    }

    fn base64(vm: &BexVm, image: &view::media::Image<'_>) -> String {
        image._data::<baml_builtins2::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image._data::<baml_builtins2::MediaValue>(vm).mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_url(MediaKind::Image, url, mime_type),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_file(MediaKind::Image, file, mime_type),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: baml_builtins2::MediaValue::from_base64(MediaKind::Image, base64, mime_type),
        }
    }
}

// Namespace aggregator (only default dispatch methods, no required methods)
impl BamlNamespaceMedia for PackageBamlImpl {}
