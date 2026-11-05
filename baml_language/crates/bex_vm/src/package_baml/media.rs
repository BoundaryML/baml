use baml_type::MediaKind;

use super::{
    BamlClassMediaAudio, BamlClassMediaImage, BamlClassMediaPdf, BamlClassMediaVideo,
    BamlNamespaceMedia, PackageBamlImpl, copy, view,
};

// All media accessors and constructors live on `bex_vm_types::MediaValue`
// itself (see `crates/bex_vm_types/src/media.rs`); the per-kind impls
// here are thin wrappers — the `view::media::*` payload ↔ trait method on
// `MediaValue`, plus `from_*` constructors that wrap the resulting
// `Arc<MediaValue>` in the kind-specific `copy::media::*` shell.

// =========================================================================
// Pdf
// =========================================================================

#[allow(clippy::used_underscore_items)]
impl BamlClassMediaPdf for PackageBamlImpl {
    fn url(pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf.payload().url().map(bex_str::BexStr::from)
    }

    fn name(pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf.payload().name().map(bex_str::BexStr::from)
    }

    fn base64(pdf: &view::media::Pdf<'_>) -> bex_str::BexStr {
        pdf.payload().base64()
    }

    fn mime_type(pdf: &view::media::Pdf<'_>) -> Option<bex_str::BexStr> {
        pdf.payload().mime_type().map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Pdf {
        copy::media::Pdf {
            payload: bex_vm_types::MediaValue::from_url(
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
            payload: bex_vm_types::MediaValue::from_file_content(
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
            payload: bex_vm_types::MediaValue::from_base64(
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
    fn url(audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio.payload().url().map(bex_str::BexStr::from)
    }

    fn name(audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio.payload().name().map(bex_str::BexStr::from)
    }

    fn base64(audio: &view::media::Audio<'_>) -> bex_str::BexStr {
        audio.payload().base64()
    }

    fn mime_type(audio: &view::media::Audio<'_>) -> Option<bex_str::BexStr> {
        audio.payload().mime_type().map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Audio {
        copy::media::Audio {
            payload: bex_vm_types::MediaValue::from_url(
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
            payload: bex_vm_types::MediaValue::from_file_content(
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
            payload: bex_vm_types::MediaValue::from_base64(
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
    fn url(video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video.payload().url().map(bex_str::BexStr::from)
    }

    fn name(video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video.payload().name().map(bex_str::BexStr::from)
    }

    fn base64(video: &view::media::Video<'_>) -> bex_str::BexStr {
        video.payload().base64()
    }

    fn mime_type(video: &view::media::Video<'_>) -> Option<bex_str::BexStr> {
        video.payload().mime_type().map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Video {
        copy::media::Video {
            payload: bex_vm_types::MediaValue::from_url(
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
            payload: bex_vm_types::MediaValue::from_file_content(
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
            payload: bex_vm_types::MediaValue::from_base64(
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
    fn url(image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image.payload().url().map(bex_str::BexStr::from)
    }

    fn name(image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image.payload().name().map(bex_str::BexStr::from)
    }

    fn base64(image: &view::media::Image<'_>) -> bex_str::BexStr {
        image.payload().base64()
    }

    fn mime_type(image: &view::media::Image<'_>) -> Option<bex_str::BexStr> {
        image.payload().mime_type().map(bex_str::BexStr::from)
    }

    fn from_url(url: &bex_str::BexStr, mime_type: Option<&bex_str::BexStr>) -> copy::media::Image {
        copy::media::Image {
            payload: bex_vm_types::MediaValue::from_url(
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
            payload: bex_vm_types::MediaValue::from_file_content(
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
            payload: bex_vm_types::MediaValue::from_base64(
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
        let stored = image.payload.base64();
        let BexStr::Flat(stored) = &stored else {
            panic!("expected a heap-backed payload, got {stored:?}")
        };
        assert!(Arc::ptr_eq(source, stored));
    }
}
