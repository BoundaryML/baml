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
    fn url(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn name(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .name()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> bex_str::BexStr {
        pdf._data::<bex_vm_types::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Pdf,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file_content(
        file: &bex_str::BexStr,
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Pdf,
                file.as_str(),
                base64.clone(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn infer_mime_type(source: &bex_str::BexStr) -> bex_str::BexStr {
        bex_vm_types::media::mime_for(source.as_str(), MediaKind::Pdf).into()
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Pdf {
        copy::media::Pdf {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Pdf,
                base64.clone(),
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
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn name(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, audio: &view::media::Audio<'_>) -> bex_str::BexStr {
        audio._data::<bex_vm_types::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Audio,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file_content(
        file: &bex_str::BexStr,
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Audio,
                file.as_str(),
                base64.clone(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn infer_mime_type(source: &bex_str::BexStr) -> bex_str::BexStr {
        bex_vm_types::media::mime_for(source.as_str(), MediaKind::Audio).into()
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Audio {
        copy::media::Audio {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Audio,
                base64.clone(),
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
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn name(vm: &BexVm, video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, video: &view::media::Video<'_>) -> bex_str::BexStr {
        video._data::<bex_vm_types::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Video,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file_content(
        file: &bex_str::BexStr,
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Video,
                file.as_str(),
                base64.clone(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn infer_mime_type(source: &bex_str::BexStr) -> bex_str::BexStr {
        bex_vm_types::media::mime_for(source.as_str(), MediaKind::Video).into()
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Video {
        copy::media::Video {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Video,
                base64.clone(),
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
            ._data::<bex_vm_types::MediaValue>(vm)
            .url()
            .map(bex_str::BexStr::from)
    }

    fn name(vm: &BexVm, image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image
            ._data::<bex_vm_types::MediaValue>(vm)
            .name()
            .map(bex_str::BexStr::from)
    }

    fn base64(vm: &BexVm, image: &view::media::Image<'_>) -> bex_str::BexStr {
        image._data::<bex_vm_types::MediaValue>(vm).base64()
    }

    fn mime_type(vm: &BexVm, image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image
            ._data::<bex_vm_types::MediaValue>(vm)
            .mime_type()
            .map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_url(
                MediaKind::Image,
                url.as_str(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn from_file_content(
        file: &bex_str::BexStr,
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_file_content(
                MediaKind::Image,
                file.as_str(),
                base64.clone(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }

    fn infer_mime_type(source: &bex_str::BexStr) -> bex_str::BexStr {
        bex_vm_types::media::mime_for(source.as_str(), MediaKind::Image).into()
    }

    fn from_base64(
        base64: &bex_str::BexStr,
        mime_type: Option<&bex_str::BexStr>,
    ) -> copy::media::Image {
        copy::media::Image {
            _data: bex_vm_types::MediaValue::from_base64(
                MediaKind::Image,
                base64.clone(),
                mime_type.map(bex_str::BexStr::as_str),
            ),
        }
    }
}

// Namespace aggregator (only default dispatch methods, no required methods)
impl BamlNamespaceMedia for PackageBamlImpl {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bex_str::BexStr;

    use super::*;

    #[test]
    fn from_base64_shares_the_callers_payload() {
        // Longer than the inline capacity, so the payload is heap-backed.
        let payload = BexStr::from("QUJD".repeat(64));
        let BexStr::Flat(source) = &payload else {
            panic!("expected a heap-backed payload, got {payload:?}")
        };
        let image = <PackageBamlImpl as BamlClassMediaImage>::from_base64(&payload, None);
        let stored = image
            ._data
            .downcast_ref::<bex_vm_types::MediaValue>()
            .expect("media wrapper holds a MediaValue")
            .base64();
        let BexStr::Flat(stored) = &stored else {
            panic!("expected a heap-backed payload, got {stored:?}")
        };
        assert!(Arc::ptr_eq(source, stored));
    }
}
