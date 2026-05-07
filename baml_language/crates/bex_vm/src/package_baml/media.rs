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
/// Layout: `media_val → Object::Instance { fields: [Value::Object(data_ptr)] }`
///          where `data_ptr → Object::RustData(Arc<MediaValue>)`.
fn clone_media_value(
    vm: &BexVm,
    media_val: Value,
) -> Result<Arc<baml_builtins2::MediaValue>, VmRustFnError> {
    // Step 1: get the _data field value (Copy) from the instance.
    let data_field: Value = {
        let instance = vm.as_instance(&media_val)?;
        instance.fields[0]
        // `instance` borrow dropped here.
    };

    // Step 2: get the RustData Arc and clone it (still immutable borrow).
    let Value::Object(ptr) = data_field else {
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
        None => Value::Null,
    };

    let mut map = IndexMap::new();
    map.insert("kind".to_string(), vm.alloc_string(kind_str.to_string()));
    map.insert(
        "source".to_string(),
        vm.alloc_string(source_str.to_string()),
    );
    map.insert("value".to_string(), vm.alloc_string(value_str));
    map.insert("mime".to_string(), mime_val);
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
    fn to_json(vm: &mut BexVm, audio: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *audio)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Audio))
    }

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
    fn to_json(vm: &mut BexVm, video: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *video)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Video))
    }

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
    fn to_json(vm: &mut BexVm, image: &Value) -> Result<Value, VmRustFnError> {
        let media = clone_media_value(vm, *image)?;
        Ok(media_value_to_json(vm, &media, MediaKind::Image))
    }

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
