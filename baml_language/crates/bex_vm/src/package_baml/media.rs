use std::sync::Arc;

use baml_type::MediaKind;

use super::{
    BamlClassMediaAudio, BamlClassMediaImage, BamlClassMediaPdf, BamlClassMediaVideo,
    BamlNamespaceMedia, PackageBamlImpl, copy, view,
};
use crate::BexVm;

// =========================================================================
// Pdf
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaPdf for PackageBamlImpl {
    fn url(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = pdf._data(vm);
        media_url(media)
    }

    fn file(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = pdf._data(vm);
        media_file(media)
    }

    fn base64(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> String {
        let media: &baml_builtins2::MediaValue = pdf._data(vm);
        media_base64(media)
    }

    fn mime_type(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = pdf._data(vm);
        media.mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: Arc::new(media_from_url(MediaKind::Pdf, url, mime_type)),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: Arc::new(media_from_file(MediaKind::Pdf, file, mime_type)),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: Arc::new(media_from_base64(MediaKind::Pdf, base64, mime_type)),
        }
    }
}

// =========================================================================
// Audio
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaAudio for PackageBamlImpl {
    fn url(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = audio._data(vm);
        media_url(media)
    }

    fn file(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = audio._data(vm);
        media_file(media)
    }

    fn base64(vm: &BexVm, audio: &view::media::Audio<'_>) -> String {
        let media: &baml_builtins2::MediaValue = audio._data(vm);
        media_base64(media)
    }

    fn mime_type(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = audio._data(vm);
        media.mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: Arc::new(media_from_url(MediaKind::Audio, url, mime_type)),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: Arc::new(media_from_file(MediaKind::Audio, file, mime_type)),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: Arc::new(media_from_base64(MediaKind::Audio, base64, mime_type)),
        }
    }
}

// =========================================================================
// Video
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaVideo for PackageBamlImpl {
    fn url(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = video._data(vm);
        media_url(media)
    }

    fn file(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = video._data(vm);
        media_file(media)
    }

    fn base64(vm: &BexVm, video: &view::media::Video<'_>) -> String {
        let media: &baml_builtins2::MediaValue = video._data(vm);
        media_base64(media)
    }

    fn mime_type(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = video._data(vm);
        media.mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: Arc::new(media_from_url(MediaKind::Video, url, mime_type)),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: Arc::new(media_from_file(MediaKind::Video, file, mime_type)),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: Arc::new(media_from_base64(MediaKind::Video, base64, mime_type)),
        }
    }
}

// =========================================================================
// Image
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaImage for PackageBamlImpl {
    fn url(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = image._data(vm);
        media_url(media)
    }

    fn file(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = image._data(vm);
        media_file(media)
    }

    fn base64(vm: &BexVm, image: &view::media::Image<'_>) -> String {
        let media: &baml_builtins2::MediaValue = image._data(vm);
        media_base64(media)
    }

    fn mime_type(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        let media: &baml_builtins2::MediaValue = image._data(vm);
        media.mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: Arc::new(media_from_url(MediaKind::Image, url, mime_type)),
        }
    }

    fn from_file(file: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: Arc::new(media_from_file(MediaKind::Image, file, mime_type)),
        }
    }

    fn from_base64(base64: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: Arc::new(media_from_base64(MediaKind::Image, base64, mime_type)),
        }
    }
}

// Namespace aggregator (only default dispatch methods, no required methods)
impl BamlNamespaceMedia for PackageBamlImpl {}

// =========================================================================
// Shared helpers
// =========================================================================

fn media_url(media: &baml_builtins2::MediaValue) -> Option<String> {
    #[allow(unsafe_code)]
    unsafe {
        media.read_content_unguarded(|content| match content {
            baml_builtins2::MediaContent::Url { url, .. } => Some(url.clone()),
            _ => None,
        })
    }
}

fn media_file(media: &baml_builtins2::MediaValue) -> Option<String> {
    #[allow(unsafe_code)]
    unsafe {
        media.read_content_unguarded(|content| match content {
            baml_builtins2::MediaContent::File { file, .. } => Some(file.clone()),
            _ => None,
        })
    }
}

fn media_base64(media: &baml_builtins2::MediaValue) -> String {
    use baml_builtins2::MediaContent;
    media.read_content(|content| match content {
        MediaContent::Base64 { base64_data, .. } => base64_data.clone(),
        MediaContent::File {
            base64_data: Some(base64_data),
            ..
        } => base64_data.clone(),
        MediaContent::Url {
            base64_data: Some(base64_data),
            ..
        } => base64_data.clone(),
        _ => String::new(),
    })
}

fn media_from_url(
    kind: MediaKind,
    url: &str,
    mime_type: Option<&str>,
) -> baml_builtins2::MediaValue {
    baml_builtins2::MediaValue::new(
        kind,
        baml_builtins2::MediaContent::Url {
            url: url.to_string(),
            base64_data: None,
        },
        mime_type.map(std::string::ToString::to_string),
    )
}

fn media_from_file(
    kind: MediaKind,
    file: &str,
    mime_type: Option<&str>,
) -> baml_builtins2::MediaValue {
    baml_builtins2::MediaValue::new(
        kind,
        baml_builtins2::MediaContent::File {
            file: file.to_string(),
            base64_data: None,
        },
        mime_type.map(std::string::ToString::to_string),
    )
}

fn media_from_base64(
    kind: MediaKind,
    base64: &str,
    mime_type: Option<&str>,
) -> baml_builtins2::MediaValue {
    baml_builtins2::MediaValue::new(
        kind,
        baml_builtins2::MediaContent::Base64 {
            base64_data: base64.to_string(),
        },
        mime_type.map(std::string::ToString::to_string),
    )
}
