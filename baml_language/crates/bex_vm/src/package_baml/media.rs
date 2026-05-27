use std::sync::Arc;

use baml_type::MediaKind;
use bex_vm_types::types::{Object, Value};
use indexmap::IndexMap;

use super::{
    BamlClassMediaAudio, BamlClassMediaImage, BamlClassMediaPdf, BamlClassMediaVideo,
    BamlNamespaceMedia, PackageBamlImpl, copy, view,
};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

/// Extract a cloned `Arc<baml_builtins2::MediaValue>` from a media instance
/// `Value`, releasing the immutable `vm` borrow before the caller needs to
/// mutably borrow `vm` for allocation.
///
/// Layout: `media_val → Object::Instance { fields: [Value::object(data_ptr)] }`
///          where `data_ptr → Object::RustData(Arc<MediaValue>)`.
fn clone_media_value(
    vm: &BexVm,
    media_val: Value,
) -> Result<Arc<baml_builtins2::MediaValue>, VmRustFnError> {
    // Step 1: get the _data field value (Copy) from the instance.
    let data_field: Value = {
        let instance = vm.as_instance(&media_val)?;
        instance.load_field(0)
        // `instance` borrow dropped here.
    };

    // Step 2: get the RustData Arc and clone it (still immutable borrow).
    let Some(ptr) = data_field.as_object_ptr() else {
        return Err(VmInternalError::MissingNativeFunction {
            name: "media._data: expected Value::Object".to_string(),
        }
        .into());
    };
    let cloned_arc: Arc<baml_builtins2::MediaValue> = match vm.get_object(ptr) {
        Object::RustData(arc) => {
            let cloned = arc.clone();
            cloned
                .downcast::<baml_builtins2::MediaValue>()
                .map_err(|_| VmInternalError::RustTypeError {
                    expected: ::std::any::TypeId::of::<baml_builtins2::MediaValue>(),
                    got: ::std::any::TypeId::of::<baml_builtins2::MediaValue>(),
                })?
        }
        _ => {
            return Err(VmInternalError::MissingNativeFunction {
                name: "media._data: RustData not found".to_string(),
            }
            .into());
        }
    };
    // `cloned_arc` owns the Arc now — `vm` immutable borrow is released.
    Ok(cloned_arc)
}

/// Allocate the BEP tagged-object map `{ kind, source, value, mime }` on the
/// VM heap, returning a `Value` suitable as `json`.
fn media_value_to_json(
    vm: &mut BexVm,
    media: &baml_builtins2::MediaValue,
    kind: MediaKind,
) -> Value {
    let kind_str = kind.tag_str();

    let (source_str, value_str) = if let Some(url) = media.url() {
        ("url", url)
    } else if let Some(file) = media.file() {
        ("file", file)
    } else {
        ("base64", media.base64())
    };

    let mime_val = match media.mime_type() {
        Some(m) => vm.alloc_string(m),
        None => Value::NULL,
    };

    let mut map: IndexMap<bex_vm_types::BexStr, _> = IndexMap::new();
    map.insert(
        bex_vm_types::BexStr::from("kind"),
        vm.alloc_string(kind_str.to_string()),
    );
    map.insert(
        bex_vm_types::BexStr::from("source"),
        vm.alloc_string(source_str.to_string()),
    );
    map.insert(
        bex_vm_types::BexStr::from("value"),
        vm.alloc_string(value_str),
    );
    map.insert(bex_vm_types::BexStr::from("mime"), mime_val);
    vm.alloc_map(map)
}

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
    fn to_json(vm: &mut BexVm, pdf: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *pdf)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Pdf))
    }

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
    fn to_json(vm: &mut BexVm, audio: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *audio)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Audio))
    }

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
    fn to_json(vm: &mut BexVm, video: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *video)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Video))
    }

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
    fn to_json(vm: &mut BexVm, image: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *image)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Image))
    }

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
