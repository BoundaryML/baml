//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use baml_types::{BamlMap, BamlMedia, BamlMediaContent, BamlMediaType};

use crate::{
    errors::{InternalError, RuntimeError, VmError},
    types::{Object, ObjectType, Value},
    Vm,
};

type NativeFunctionResult = Result<Value, VmError>;

/// Array length.
pub fn array_len(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.

    let expected = ObjectType::Array;
    let ob_index = vm.objects.as_object(&args[0], expected)?;

    let Object::Array(array) = &vm.objects[ob_index] else {
        return Err(InternalError::TypeError {
            expected: expected.into(),
            got: ObjectType::of(&vm.objects[ob_index]).into(),
        }
        .into());
    };

    Ok(Value::Int(array.len() as i64))
}

/// Length of map
pub fn map_len(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.

    let expected = ObjectType::Map;
    let ob_index = vm.objects.as_object(&args[0], expected)?;

    let Object::Map(map) = &vm.objects[ob_index] else {
        return Err(InternalError::TypeError {
            expected: expected.into(),
            got: ObjectType::of(&vm.objects[ob_index]).into(),
        }
        .into());
    };

    Ok(Value::Int(map.len() as i64))
}
/// Map `contains`
pub fn map_has(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.

    let expected = ObjectType::Map;
    let ob_index = vm.objects.as_object(&args[0], expected)?;

    let Object::Map(map) = &vm.objects[ob_index] else {
        return Err(InternalError::TypeError {
            expected: expected.into(),
            got: ObjectType::of(&vm.objects[ob_index]).into(),
        }
        .into());
    };

    let key = vm.objects.as_string(&args[1])?;

    Ok(Value::Bool(map.contains_key(key)))
}

pub fn env_get(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let key = vm.objects.as_string(&args[0])?;

    match vm.env_vars.get(key) {
        Some(value) => Ok(vm.alloc_string(value.clone())),
        None => Err(VmError::RuntimeError(RuntimeError::Other(format!(
            "Environment variable '{key}' not found",
        )))),
    }
}

pub fn image_from_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let url = vm.objects.as_string(&args[0])?;

    Ok(vm.alloc_media(BamlMedia::url(BamlMediaType::Image, url.to_owned(), None)))
}

pub fn audio_from_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let url = vm.objects.as_string(&args[0])?;

    Ok(vm.alloc_media(BamlMedia::url(BamlMediaType::Audio, url.to_owned(), None)))
}

pub fn video_from_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let url = vm.objects.as_string(&args[0])?;

    Ok(vm.alloc_media(BamlMedia::url(BamlMediaType::Video, url.to_owned(), None)))
}

pub fn pdf_from_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let url = vm.objects.as_string(&args[0])?;

    Ok(vm.alloc_media(BamlMedia::url(BamlMediaType::Pdf, url.to_owned(), None)))
}

pub fn image_from_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media_type = vm.objects.as_string(&args[0])?;
    let base64 = vm.objects.as_string(&args[1])?;

    Ok(vm.alloc_media(BamlMedia::base64(
        BamlMediaType::Image,
        base64.to_owned(),
        Some(media_type.to_owned()),
    )))
}

pub fn audio_from_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.5a
    let media_type = vm.objects.as_string(&args[0])?;
    let base64 = vm.objects.as_string(&args[1])?;

    Ok(vm.alloc_media(BamlMedia::base64(
        BamlMediaType::Audio,
        base64.to_owned(),
        Some(media_type.to_owned()),
    )))
}

pub fn video_from_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media_type = vm.objects.as_string(&args[0])?;
    let base64 = vm.objects.as_string(&args[1])?;

    Ok(vm.alloc_media(BamlMedia::base64(
        BamlMediaType::Video,
        base64.to_owned(),
        Some(media_type.to_owned()),
    )))
}

pub fn pdf_from_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let base64 = vm.objects.as_string(&args[0])?;

    Ok(vm.alloc_media(BamlMedia::base64(
        BamlMediaType::Pdf,
        base64.to_owned(),
        Some("application/pdf".to_string()),
    )))
}

pub fn media_is_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media = vm.objects.as_media(&args[0])?;

    Ok(Value::Bool(matches!(
        media.content,
        BamlMediaContent::Url(_)
    )))
}

pub fn media_is_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media = vm.objects.as_media(&args[0])?;

    Ok(Value::Bool(matches!(
        media.content,
        BamlMediaContent::Base64(_)
    )))
}

pub fn media_as_url(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media = vm.objects.as_media(&args[0])?;

    match &media.content {
        BamlMediaContent::Url(url) => Ok(vm.alloc_string(url.url.clone())),

        _ => Err(VmError::RuntimeError(RuntimeError::Other(
            "Media is not a URL".to_string(),
        ))),
    }
}

pub fn media_as_base64(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media = vm.objects.as_media(&args[0])?;

    match &media.content {
        BamlMediaContent::Base64(base64) => Ok(vm.alloc_string(base64.base64.clone())),

        _ => Err(VmError::RuntimeError(RuntimeError::Other(
            "Media is not base64".to_string(),
        ))),
    }
}

pub fn media_mime_type(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let media = vm.objects.as_media(&args[0])?;

    Ok(vm.alloc_string(media.mime_type.clone().unwrap_or("".to_string())))
}

pub fn deep_copy_object(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let index = vm.objects.as_object(&args[0], ObjectType::Any)?;

    let object = vm.objects[index].clone();

    Ok(Value::Object(vm.objects.insert(object)))
}

pub fn any_value_to_string(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let formatted = format_value_recursive(vm, &args[0], 0)?;

    Ok(vm.alloc_string(formatted))
}

fn format_value_recursive(vm: &mut Vm, value: &Value, depth: usize) -> Result<String, VmError> {
    // Check available stack space (MAX_FRAMES - current_frames)
    let available_frames = crate::vm::MAX_FRAMES.saturating_sub(vm.frames.len());

    if depth >= available_frames {
        return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
    }

    match value {
        Value::Null => Ok("null".to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::Bool(b) => Ok(b.to_string()),

        Value::Object(obj_idx) => match &vm.objects[*obj_idx] {
            Object::Instance(instance) => {
                let Object::Class(class) = &vm.objects[instance.class] else {
                    return Err(VmError::RuntimeError(RuntimeError::Other(
                        "Invalid class reference".to_string(),
                    )));
                };

                let class_name = class.name.clone();
                let field_names = class.field_names.clone();
                let fields = instance.fields.clone();

                let mut result = format!("{class_name} {{\n");
                let field_indent = "    ".repeat(depth + 1);

                for (i, field_value) in fields.iter().enumerate() {
                    let field_name = match field_names.get(i) {
                        Some(name) => name.as_str(),
                        None => {
                            let fallback = format!("field_{i}");
                            let formatted_value =
                                format_value_recursive(vm, field_value, depth + 1)?;
                            result.push_str(&format!(
                                "{field_indent}{fallback}: {formatted_value}\n"
                            ));
                            continue;
                        }
                    };
                    let formatted_value = format_value_recursive(vm, field_value, depth + 1)?;
                    result.push_str(&format!("{field_indent}{field_name}: {formatted_value}\n"));
                }

                let indent = "    ".repeat(depth);
                result.push_str(&format!("{indent}}}"));
                Ok(result)
            }

            Object::Array(values) => {
                let values = values.clone();
                let mut result = String::from("[");
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        result.push_str(", ");
                    }
                    result.push_str(&format_value_recursive(vm, value, depth)?);
                }
                result.push(']');
                Ok(result)
            }

            Object::Map(map) => {
                let map = map.clone();
                let mut result = String::from("{\n");
                let field_indent = "    ".repeat(depth + 1);

                for (key, value) in map.iter() {
                    let formatted_value = format_value_recursive(vm, value, depth + 1)?;
                    result.push_str(&format!("{field_indent}\"{key}\": {formatted_value}\n"));
                }

                let indent = "    ".repeat(depth);
                result.push_str(&format!("{indent}}}"));
                Ok(result)
            }

            Object::String(s) => Ok(format!("\"{s}\"")),
            Object::Enum(e) => Ok(e.name.clone()),
            Object::Variant(variant) => {
                let Object::Enum(enm) = &vm.objects[variant.enm] else {
                    return Err(VmError::RuntimeError(RuntimeError::Other(
                        "Invalid enum reference".to_string(),
                    )));
                };

                let variant_name = match enm.variant_names.get(variant.index) {
                    Some(name) => name.clone(),
                    None => format!("variant_{}", variant.index),
                };
                Ok(variant_name)
            }
            Object::Function(f) => Ok(format!("<function {}>", f.name)),
            Object::Class(c) => Ok(format!("<class {}>", c.name)),
            Object::Media(_) => Ok("<media>".to_string()),
            Object::Future(_) => Ok("<future>".to_string()),
            Object::BamlType(_) => Ok("<baml type>".to_string()),
        },
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> NativeFunctionResult;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let fns: &[(&str, (NativeFunction, usize))] = &[
        // Array.
        ("baml.Array.length", (array_len, 1)),
        // Map.
        ("baml.Map.length", (map_len, 1)),
        ("baml.Map.has", (map_has, 2)),
        // Media
        ("baml.media.image.from_url", (image_from_url, 1)),
        ("baml.media.audio.from_url", (audio_from_url, 1)),
        ("baml.media.video.from_url", (video_from_url, 1)),
        ("baml.media.pdf.from_url", (pdf_from_url, 1)),
        ("baml.media.image.from_base64", (image_from_base64, 2)),
        ("baml.media.audio.from_base64", (audio_from_base64, 2)),
        ("baml.media.video.from_base64", (video_from_base64, 2)),
        ("baml.media.pdf.from_base64", (pdf_from_base64, 1)),
        ("baml.media.image.is_url", (media_is_url, 1)),
        ("baml.media.video.is_url", (media_is_url, 1)),
        ("baml.media.audio.is_url", (media_is_url, 1)),
        ("baml.media.pdf.is_url", (media_is_url, 1)),
        ("baml.media.image.is_base64", (media_is_base64, 1)),
        ("baml.media.video.is_base64", (media_is_base64, 1)),
        ("baml.media.audio.is_base64", (media_is_base64, 1)),
        ("baml.media.pdf.is_base64", (media_is_base64, 1)),
        ("baml.media.image.as_url", (media_as_url, 1)),
        ("baml.media.video.as_url", (media_as_url, 1)),
        ("baml.media.audio.as_url", (media_as_url, 1)),
        ("baml.media.pdf.as_url", (media_as_url, 1)),
        ("baml.media.image.as_base64", (media_as_base64, 1)),
        ("baml.media.video.as_base64", (media_as_base64, 1)),
        ("baml.media.audio.as_base64", (media_as_base64, 1)),
        ("baml.media.pdf.as_base64", (media_as_base64, 1)),
        ("baml.media.image.mime", (media_mime_type, 1)),
        ("baml.media.video.mime", (media_mime_type, 1)),
        ("baml.media.audio.mime", (media_mime_type, 1)),
        ("baml.media.pdf.mime", (media_mime_type, 1)),
        // Environment
        ("env.get", (env_get, 1)),
        // Utility functions.
        ("baml.deep_copy", (deep_copy_object, 1)),
        ("baml.unstable.string", (any_value_to_string, 1)),
    ];

    BamlMap::from_iter(
        fns.iter()
            .map(|(name, (func, arity))| (name.to_string(), (*func, *arity))),
    )
}
