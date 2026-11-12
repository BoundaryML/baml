use baml_type::MediaKind;

use super::{
    BamlClassMediaAudio, BamlClassMediaImage, BamlClassMediaPdf, BamlClassMediaVideo,
    BamlNamespaceMedia, PackageBamlImpl, copy, view,
};
use crate::BexVm;

// All media accessors and constructors live on `bex_vm_types::MediaValue`
// itself (see `crates/bex_vm_types/src/media.rs`); the per-kind impls
// here are thin wrappers — `_data` getter on the `view::media::*` ↔ trait
// method on `MediaValue`, plus `from_*` constructors that wrap the
// resulting `Arc<MediaValue>` in the kind-specific `copy::media::*` shell.

// =========================================================================
// Pdf
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaPdf for PackageBamlImpl {
    fn url(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .url()
    }

    fn name(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .name()
    }

    fn base64(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> String {
        pdf._data::<bex_vm_types::MediaValue>(vm).base64().to_string()
    }

    fn mime_type(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<String> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Pdf,
                url,
                mime_type,
            ),
        }
    }

    fn from_file_content(
        file: &str,
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Pdf,
                file,
                base64.into(),
                mime_type,
            ),
        }
    }

    fn infer_mime_type(source: &str) -> String {
        bex_vm_types::media::mime_for(source, MediaKind::Pdf).into()
    }

    fn from_base64(
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Pdf,
                base64.into(),
                mime_type,
            ),
        }
    }
}

// =========================================================================
// Audio
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaAudio for PackageBamlImpl {
    fn url(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
    }

    fn name(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
    }

    fn base64(vm: &BexVm, audio: &view::media::Audio<'_>) -> String {
        audio._data::<bex_vm_types::MediaValue>(vm).base64().to_string()
    }

    fn mime_type(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<String> {
        audio
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Audio,
                url,
                mime_type,
            ),
        }
    }

    fn from_file_content(
        file: &str,
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Audio,
                file,
                base64.into(),
                mime_type,
            ),
        }
    }

    fn infer_mime_type(source: &str) -> String {
        bex_vm_types::media::mime_for(source, MediaKind::Audio).into()
    }

    fn from_base64(
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Audio,
                base64.into(),
                mime_type,
            ),
        }
    }
}

// =========================================================================
// Video
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaVideo for PackageBamlImpl {
    fn url(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
    }

    fn name(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
    }

    fn base64(vm: &BexVm, video: &view::media::Video<'_>) -> String {
        video._data::<bex_vm_types::MediaValue>(vm).base64().to_string()
    }

    fn mime_type(vm: &BexVm, video: &view::media::Video<'_>) -> Option<String> {
        video
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Video,
                url,
                mime_type,
            ),
        }
    }

    fn from_file_content(
        file: &str,
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Video,
                file,
                base64.into(),
                mime_type,
            ),
        }
    }

    fn infer_mime_type(source: &str) -> String {
        bex_vm_types::media::mime_for(source, MediaKind::Video).into()
    }

    fn from_base64(
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Video,
                base64.into(),
                mime_type,
            ),
        }
    }
}

// =========================================================================
// Image
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaImage for PackageBamlImpl {
    fn url(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
    }

    fn name(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
    }

    fn base64(vm: &BexVm, image: &view::media::Image<'_>) -> String {
        image._data::<bex_vm_types::MediaValue>(vm).base64().to_string()
    }

    fn mime_type(vm: &BexVm, image: &view::media::Image<'_>) -> Option<String> {
        image
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
    }

    fn from_url(url: &str, mime_type: Option<&str>) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Image,
                url,
                mime_type,
            ),
        }
    }

    fn from_file_content(
        file: &str,
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Image,
                file,
                base64.into(),
                mime_type,
            ),
        }
    }

    fn infer_mime_type(source: &str) -> String {
        bex_vm_types::media::mime_for(source, MediaKind::Image).into()
    }

    fn from_base64(
        base64: &str,
        mime_type: Option<&str>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Image,
                base64.into(),
                mime_type,
            ),
        }
    }
}

// Namespace aggregator (only default dispatch methods, no required methods)
impl BamlNamespaceMedia for PackageBamlImpl {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_base64_stores_the_payload() {
        let payload = "QUJD".repeat(64);
        let image = <PackageBamlImpl as BamlClassMediaImage>::from_base64(&payload, None);
        let stored = image
            ._data
            .downcast_ref::<bex_vm_types::MediaValue>()
            .expect("media wrapper holds a MediaValue")
            .base64();
        assert_eq!(stored.as_str(), payload);
    }
}
