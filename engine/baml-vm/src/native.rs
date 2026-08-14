//! Native functions and methods.
//!
//! We need to find a better pattern for this, but this works for now.

use std::collections::HashMap;

use baml_types::{BamlMap, BamlMedia, BamlMediaContent, BamlMediaType};
use ryu_js::Buffer;

use crate::{
    errors::{InternalError, RuntimeError, VmError},
    indexable::ObjectIndex,
    types::{Future, FutureKind, Instance, Object, ObjectType, Value},
    Vm,
};

type NativeFunctionResult = Result<Value, VmError>;

/// Formats a number using JavaScript's `Number.prototype.toFixed()` semantics.
pub fn number_to_fixed(value: f64, digits: i64) -> Result<String, String> {
    if !(0..=100).contains(&digits) {
        return Err(format!(
            "to_fixed: digits must be in [0, 100], got {digits}"
        ));
    }

    let mut buffer = Buffer::new();
    Ok(buffer.format_to_fixed(value, digits as u8).to_string())
}

/// String length.
pub fn string_len(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let s = vm.objects.as_string(&args[0])?;
    Ok(Value::Int(s.chars().count() as i64))
}

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

/// Array push
pub fn array_push(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.

    let expected = ObjectType::Array;
    let ob_index = vm.objects.as_object(&args[0], expected)?;

    let Object::Array(array) = &mut vm.objects[ob_index] else {
        return Err(InternalError::TypeError {
            expected: expected.into(),
            got: ObjectType::of(&vm.objects[ob_index]).into(),
        }
        .into());
    };

    let value = args[1];

    array.push(value);

    // TODO: Should have no return type.
    Ok(Value::Null)
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

/// String length
pub fn string_length(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let string = vm.objects.as_string(&args[0])?;
    Ok(Value::Int(string.len() as i64))
}

/// String to lowercase
pub fn string_to_lower_case(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    Ok(vm.alloc_string(string.to_lowercase()))
}

/// String to uppercase
pub fn string_to_upper_case(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    Ok(vm.alloc_string(string.to_uppercase()))
}

/// String trim
pub fn string_trim(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    Ok(vm.alloc_string(string.trim().to_string()))
}

/// String includes
pub fn string_includes(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    let search = vm.objects.as_string(&args[1])?;
    Ok(Value::Bool(string.contains(search)))
}

/// String starts with
pub fn string_starts_with(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    let prefix = vm.objects.as_string(&args[1])?;
    Ok(Value::Bool(string.starts_with(prefix)))
}

/// String ends with
pub fn string_ends_with(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    let suffix = vm.objects.as_string(&args[1])?;
    Ok(Value::Bool(string.ends_with(suffix)))
}

/// String split
pub fn string_split(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?.to_owned();
    let delimiter = vm.objects.as_string(&args[1])?.to_owned();

    let parts: Vec<Value> = string
        .split(&delimiter)
        .map(|s| vm.alloc_string(s.to_string()))
        .collect();

    Ok(vm.alloc_array(parts))
}

/// String substring
pub fn string_substring(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;

    let start = match &args[1] {
        Value::Int(i) => *i as usize,
        _ => {
            return Err(VmError::RuntimeError(RuntimeError::Other(
                "substring() start index must be an integer".to_string(),
            )))
        }
    };

    let end = match &args[2] {
        Value::Int(i) => *i as usize,
        _ => {
            return Err(VmError::RuntimeError(RuntimeError::Other(
                "substring() end index must be an integer".to_string(),
            )))
        }
    };

    // Handle bounds
    let len = string.len();
    let start = start.min(len);
    let end = end.min(len).max(start);

    // Note: This is byte indexing, not char indexing
    // For full Unicode support, we'd need to use char_indices()
    Ok(vm.alloc_string(string[start..end].to_string()))
}

/// String replace
pub fn string_replace(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let string = vm.objects.as_string(&args[0])?;
    let search = vm.objects.as_string(&args[1])?;
    let replacement = vm.objects.as_string(&args[2])?;

    // Replace first occurrence only (matching JavaScript behavior)
    let result = string.replacen(search, replacement, 1);
    Ok(vm.alloc_string(result))
}

/// Number.prototype.toFixed compatibility for float/int values.
pub fn float_to_fixed(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    let value = match args[0] {
        Value::Float(float) => float,
        Value::Int(int) => int as f64,
        _ => {
            return Err(VmError::RuntimeError(RuntimeError::Other(
                "to_fixed() method only available on numbers".to_string(),
            )));
        }
    };

    let digits = match args[1] {
        Value::Int(int) => int,
        _ => {
            return Err(VmError::RuntimeError(RuntimeError::Other(
                "to_fixed() digits argument must be an int".to_string(),
            )));
        }
    };

    let formatted = number_to_fixed(value, digits)
        .map_err(|msg| VmError::RuntimeError(RuntimeError::Other(msg)))?;
    Ok(vm.alloc_string(formatted))
}

pub fn deep_copy_object(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM.
    let mut copied_objects = HashMap::new();
    deep_copy_value_recursive(vm, args[0], &mut copied_objects)
}

/// Deep equality comparison between two values
pub fn deep_equals(vm: &mut Vm, args: &[Value]) -> NativeFunctionResult {
    // Arity is already checked by the VM
    let mut visited = HashMap::new();
    let result = deep_equals_recursive(vm, args[0], args[1], &mut visited);
    Ok(Value::Bool(result))
}

/// Recursively deep copy a value, handling nested objects.
///
/// TODO: Likely will need to be refactored to iterative for perf / stack
/// overflow issues with big objects. But since this one is not as straight
/// forward as `while stack.pop()`, we'll leave it as is for now.
fn deep_copy_value_recursive(
    vm: &mut Vm,
    value: Value,
    copied_objects: &mut HashMap<ObjectIndex, ObjectIndex>,
) -> NativeFunctionResult {
    match value {
        // Primitive values are copied by value
        Value::Null | Value::Int(_) | Value::Float(_) | Value::Bool(_) => Ok(value),

        // Objects need deep copying
        Value::Object(index) => {
            // Check if we've already copied this object (handles circular references)
            if let Some(&new_index) = copied_objects.get(&index) {
                return Ok(Value::Object(new_index));
            }

            // Clone the object first to avoid borrow checker issues
            let object = vm.objects[index].clone();

            // Deep copy based on object type
            let new_index = match object {
                Object::String(s) => {
                    // Strings are immutable, but we still create a new copy
                    vm.objects.insert(Object::String(s))
                }

                Object::Array(values) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_index = vm.objects.insert(Object::Array(Vec::new()));
                    copied_objects.insert(index, placeholder_index);

                    // Deep copy each element in the array
                    let mut new_values = Vec::with_capacity(values.len());
                    for value in values {
                        new_values.push(deep_copy_value_recursive(vm, value, copied_objects)?);
                    }

                    // Update the placeholder with the actual array
                    vm.objects[placeholder_index] = Object::Array(new_values);
                    placeholder_index
                }

                Object::Map(map) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_index = vm.objects.insert(Object::Map(BamlMap::new()));
                    copied_objects.insert(index, placeholder_index);

                    // Deep copy each key-value pair
                    let mut new_map = BamlMap::new();
                    for (key, value) in map.iter() {
                        let new_value = deep_copy_value_recursive(vm, *value, copied_objects)?;
                        new_map.insert(key.clone(), new_value);
                    }

                    // Update the placeholder with the actual map
                    vm.objects[placeholder_index] = Object::Map(new_map);
                    placeholder_index
                }

                Object::Instance(instance) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_index = vm.objects.insert(Object::Instance(Instance {
                        class: instance.class,
                        fields: Vec::new(),
                    }));
                    copied_objects.insert(index, placeholder_index);

                    // Deep copy each field in the instance
                    let mut new_fields = Vec::with_capacity(instance.fields.len());
                    for field in instance.fields {
                        new_fields.push(deep_copy_value_recursive(vm, field, copied_objects)?);
                    }

                    // Update the placeholder with the actual instance
                    vm.objects[placeholder_index] = Object::Instance(Instance {
                        class: instance.class,
                        fields: new_fields,
                    });
                    placeholder_index
                }

                // These types don't contain nested objects that need deep copying
                Object::Function(f) => vm.objects.insert(Object::Function(f)),
                Object::Class(c) => vm.objects.insert(Object::Class(c)),
                Object::Enum(e) => vm.objects.insert(Object::Enum(e)),
                Object::Variant(v) => vm.objects.insert(Object::Variant(v)),
                Object::Media(m) => vm.objects.insert(Object::Media(m)),
                Object::Future(f) => vm.objects.insert(Object::Future(f)),
                Object::BamlType(t) => vm.objects.insert(Object::BamlType(t)),
            };

            // Record the mapping if not already done (for non-circular cases)
            copied_objects.entry(index).or_insert(new_index);

            Ok(Value::Object(new_index))
        }
    }
}

/// Recursively compare two values for deep equality
fn deep_equals_recursive(
    vm: &Vm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(ObjectIndex, ObjectIndex), bool>,
) -> bool {
    match (a, b) {
        // Primitive values - direct comparison
        (Value::Null, Value::Null) => true,
        (Value::Int(a), Value::Int(b)) => a == b,
        (Value::Float(a), Value::Float(b)) => {
            // Handle NaN case: NaN != NaN but we want deep_equals to consider them equal
            (a.is_nan() && b.is_nan()) || a == b
        }
        (Value::Bool(a), Value::Bool(b)) => a == b,

        // Objects - need recursive comparison
        (Value::Object(a_idx), Value::Object(b_idx)) => {
            // Check if same reference (optimization)
            if a_idx == b_idx {
                return true;
            }

            // Check if we've already compared these objects (circular reference handling)
            let key = if a_idx < b_idx {
                (a_idx, b_idx)
            } else {
                (b_idx, a_idx)
            };

            if let Some(&result) = visited.get(&key) {
                return result;
            }

            // Mark as being visited (assume equal to handle circular refs)
            visited.insert(key, true);

            // Compare based on object type
            let result = match (&vm.objects[a_idx], &vm.objects[b_idx]) {
                (Object::String(a), Object::String(b)) => a == b,

                (Object::Array(a_values), Object::Array(b_values)) => {
                    a_values.len() == b_values.len()
                        && a_values
                            .iter()
                            .zip(b_values.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Map(a_map), Object::Map(b_map)) => {
                    a_map.len() == b_map.len()
                        && a_map.iter().all(|(key, a_val)| {
                            b_map.get(key).is_some_and(|b_val| {
                                deep_equals_recursive(vm, *a_val, *b_val, visited)
                            })
                        })
                }

                (Object::Instance(a_inst), Object::Instance(b_inst)) => {
                    a_inst.class == b_inst.class
                        && a_inst.fields.len() == b_inst.fields.len()
                        && a_inst
                            .fields
                            .iter()
                            .zip(b_inst.fields.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Variant(a_var), Object::Variant(b_var)) => {
                    a_var.enm == b_var.enm && a_var.index == b_var.index
                }

                (Object::Enum(a_enum), Object::Enum(b_enum)) => {
                    a_enum.name == b_enum.name && a_enum.variant_names == b_enum.variant_names
                }

                (Object::Class(a_class), Object::Class(b_class)) => {
                    a_class.name == b_class.name && a_class.field_names == b_class.field_names
                }

                (Object::Media(a_media), Object::Media(b_media)) => a_media == b_media,

                // Functions are compared by reference (they're the same if they point to the same function)
                (Object::Function(_), Object::Function(_)) => a_idx == b_idx,

                // Future comparison - compare the inner values if both are ready
                (Object::Future(a_fut), Object::Future(b_fut)) => match (a_fut, b_fut) {
                    (Future::Ready(a_val), Future::Ready(b_val)) => {
                        deep_equals_recursive(vm, *a_val, *b_val, visited)
                    }
                    (Future::Pending(a_pend), Future::Pending(b_pend)) => {
                        // Compare pending futures by their function and args
                        a_pend.function == b_pend.function
                            && matches!(
                                (&a_pend.kind, &b_pend.kind),
                                (FutureKind::Llm, FutureKind::Llm)
                                    | (FutureKind::Net, FutureKind::Net)
                            )
                            && a_pend.args.len() == b_pend.args.len()
                            && a_pend
                                .args
                                .iter()
                                .zip(b_pend.args.iter())
                                .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                    }
                    _ => false,
                },

                (Object::BamlType(a_type), Object::BamlType(b_type)) => a_type == b_type,

                // Different types are not equal
                _ => false,
            };

            // Update the visited map with the actual result
            visited.insert(key, result);
            result
        }

        // Different types are not equal
        _ => false,
    }
}

pub type NativeFunction = fn(&mut Vm, &[Value]) -> NativeFunctionResult;

pub fn functions() -> BamlMap<String, (NativeFunction, usize)> {
    let fns: &[(&str, (NativeFunction, usize))] = &[
        // String.
        ("baml.String.length", (string_len, 1)),
        // Array.
        ("baml.Array.length", (array_len, 1)),
        ("baml.Array.push", (array_push, 2)),
        // Map.
        ("baml.Map.length", (map_len, 1)),
        ("baml.Map.has", (map_has, 2)),
        // String methods
        ("baml.String.length", (string_length, 1)),
        ("baml.String.toLowerCase", (string_to_lower_case, 1)),
        ("baml.String.toUpperCase", (string_to_upper_case, 1)),
        ("baml.String.trim", (string_trim, 1)),
        ("baml.String.includes", (string_includes, 2)),
        ("baml.String.startsWith", (string_starts_with, 2)),
        ("baml.String.endsWith", (string_ends_with, 2)),
        ("baml.String.split", (string_split, 2)),
        ("baml.String.substring", (string_substring, 3)),
        ("baml.String.replace", (string_replace, 3)),
        ("baml.Float.to_fixed", (float_to_fixed, 2)),
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
        ("baml.deep_equals", (deep_equals, 2)),
    ];

    BamlMap::from_iter(
        fns.iter()
            .map(|(name, (func, arity))| (name.to_string(), (*func, *arity))),
    )
}

#[cfg(test)]
mod tests {
    use super::number_to_fixed;

    #[test]
    fn number_to_fixed_matches_js_reference_cases() {
        let cases = [
            (3.14159, 2, "3.14"),
            (3.14159, 5, "3.14159"),
            (3.14159, 0, "3"),
            (3.7, 0, "4"),
            (3.2, 0, "3"),
            (0.9, 0, "1"),
            (0.5, 0, "1"),
            (0.4, 0, "0"),
            (1.5, 3, "1.500"),
            (5.0, 4, "5.0000"),
            (42.1, 5, "42.10000"),
            (0.000123, 6, "0.000123"),
            (1234.5678, 2, "1234.57"),
            (-3.14, 1, "-3.1"),
            (-1.5, 0, "-2"),
            (-0.4, 0, "-0"),
            (-100.5, 0, "-101"),
            (-0.0, 2, "0.00"),
            (-999.999, 2, "-1000.00"),
            (1.005, 2, "1.00"),
            (1.255, 2, "1.25"),
            (1.355, 2, "1.35"),
            (1.045, 2, "1.04"),
            (1.105, 2, "1.10"),
            (1e21, 2, "1e+21"),
            (1.5e21, 0, "1.5e+21"),
            (-2e25, 3, "-2e+25"),
            (1e100, 0, "1e+100"),
            (9.99e20, 2, "999000000000000000000.00"),
        ];

        for (value, digits, expected) in cases {
            assert_eq!(
                number_to_fixed(value, digits).unwrap(),
                expected,
                "value={value}, digits={digits}"
            );
        }
    }

    #[test]
    fn number_to_fixed_handles_special_values() {
        assert_eq!(number_to_fixed(f64::NAN, 2).unwrap(), "NaN");
        assert_eq!(number_to_fixed(f64::INFINITY, 2).unwrap(), "Infinity");
        assert_eq!(number_to_fixed(f64::NEG_INFINITY, 2).unwrap(), "-Infinity");
    }

    #[test]
    fn number_to_fixed_validates_digit_range() {
        assert_eq!(
            number_to_fixed(3.14, -1).unwrap_err(),
            "to_fixed: digits must be in [0, 100], got -1"
        );
        assert_eq!(
            number_to_fixed(3.14, 101).unwrap_err(),
            "to_fixed: digits must be in [0, 100], got 101"
        );
        assert_eq!(
            number_to_fixed(3.14, 200).unwrap_err(),
            "to_fixed: digits must be in [0, 100], got 200"
        );
    }

    #[test]
    fn number_to_fixed_accepts_boundary_digit_values() {
        assert_eq!(number_to_fixed(3.14, 0).unwrap(), "3");

        let output = number_to_fixed(3.14, 100).unwrap();
        let (_, fractional) = output.split_once('.').expect("should contain decimal part");
        assert_eq!(fractional.len(), 100);
    }
}
