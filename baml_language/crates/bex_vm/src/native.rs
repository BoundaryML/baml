//! Native function implementations for BAML builtins.
//!
//! This module uses a trait-based approach for implementing native functions:
//! - `NativeFunctions` trait with clean Rust types (users implement these)
//! - `__baml_*` glue methods that handle Value conversion (auto-generated)
//!
//! # Adding a new builtin
//!
//! 1. Add the definition in `baml_builtins/src/lib.rs`
//! 2. Implement the corresponding method in `impl NativeFunctions for VmNatives`

use std::{collections::HashMap, fmt::Write};

use baml_base::MediaKind;
use bex_vm_types::{
    HeapPtr,
    types::{Future, Instance, MediaContent, MediaValue, Object, Type, Value},
};
use indexmap::IndexMap;

use crate::{
    BexVm,
    errors::{InternalError, RuntimeError, VmError},
};

/// Result type for native functions.
pub type NativeFunctionResult = Result<Value, VmError>;

/// Native function type alias.
pub type NativeFunction = fn(&mut BexVm, &[Value]) -> NativeFunctionResult;

// Generate the NativeFunctions trait from builtin definitions
baml_builtins::with_builtins!(baml_builtins_macros::generate_native_trait);

/// The VM's native function implementations.
pub struct VmNatives;

impl NativeFunctions for VmNatives {
    // =========================================================================
    // Array methods
    // =========================================================================

    #[allow(clippy::cast_possible_wrap)]
    fn baml_array_length(array: &[Value]) -> i64 {
        array.len() as i64
    }

    fn baml_array_push(array: &mut Vec<Value>, item: &Value) {
        array.push(*item);
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn baml_array_at(array: &[Value], index: i64) -> Result<Value, VmError> {
        array.get(index as usize).copied().ok_or_else(|| {
            VmError::RuntimeError(RuntimeError::Other(
                format!("Index out of bounds: {index}",),
            ))
        })
    }

    fn baml_array_concat(array: &[Value], other: &[Value]) -> Vec<Value> {
        array.iter().chain(other.iter()).copied().collect()
    }

    // =========================================================================
    // String methods
    // =========================================================================

    #[allow(clippy::cast_possible_wrap)]
    fn baml_string_length(string: &str) -> i64 {
        string.chars().count() as i64
    }

    fn baml_string_to_lower_case(string: &str) -> String {
        string.to_lowercase()
    }

    fn baml_string_to_upper_case(string: &str) -> String {
        string.to_uppercase()
    }

    fn baml_string_trim(string: &str) -> String {
        string.trim().to_string()
    }

    fn baml_string_includes(string: &str, search: &str) -> bool {
        string.contains(search)
    }

    fn baml_string_starts_with(string: &str, prefix: &str) -> bool {
        string.starts_with(prefix)
    }

    fn baml_string_ends_with(string: &str, suffix: &str) -> bool {
        string.ends_with(suffix)
    }

    fn baml_string_split(vm: &mut BexVm, string: &str, delimiter: &str) -> Vec<Value> {
        string
            .split(delimiter)
            .map(|s| vm.alloc_string(s.to_string()))
            .collect()
    }

    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    fn baml_string_substring(string: &str, start: i64, end: i64) -> String {
        let len = string.len();
        let start = (start as usize).min(len);
        let end = (end as usize).min(len).max(start);
        string[start..end].to_string()
    }

    fn baml_string_replace(string: &str, search: &str, replacement: &str) -> String {
        // Replace first occurrence only (matching JavaScript behavior)
        string.replacen(search, replacement, 1)
    }

    // =========================================================================
    // Map methods
    // =========================================================================

    #[allow(clippy::cast_possible_wrap)]
    fn baml_map_length(map: &IndexMap<String, Value>) -> i64 {
        map.len() as i64
    }

    fn baml_map_has(map: &IndexMap<String, Value>, key: &str) -> bool {
        map.contains_key(key)
    }

    // =========================================================================
    // Free functions
    // =========================================================================

    fn baml_deep_copy(vm: &mut BexVm, value: &Value) -> Result<Value, VmError> {
        let mut copied_objects = HashMap::new();
        deep_copy_value_recursive(vm, *value, &mut copied_objects)
    }

    fn baml_deep_equals(vm: &mut BexVm, a: &Value, b: &Value) -> bool {
        let mut visited = HashMap::new();
        deep_equals_recursive(vm, *a, *b, &mut visited)
    }

    fn baml_unstable_string(vm: &mut BexVm, value: &Value) -> Result<String, VmError> {
        format_value_recursive(vm, value, 0)
    }

    // =========================================================================
    // Media methods
    // =========================================================================

    fn baml_media_as_url(media: &MediaValue) -> Option<String> {
        match &media.content {
            MediaContent::Url { url, .. } => Some(url.clone()),
            _ => None,
        }
    }

    fn baml_media_as_base64(media: &MediaValue) -> Option<String> {
        match &media.content {
            MediaContent::Base64 { base64_data, .. } => Some(base64_data.clone()),
            MediaContent::File {
                base64_data: Some(base64_data),
                ..
            } => Some(base64_data.clone()),
            MediaContent::Url {
                base64_data: Some(base64_data),
                ..
            } => Some(base64_data.clone()),
            _ => None,
        }
    }

    fn baml_media_as_file(media: &MediaValue) -> Option<String> {
        match &media.content {
            MediaContent::File { file, .. } => Some(file.clone()),
            _ => None,
        }
    }

    fn baml_media_mime_type(media: &MediaValue) -> Option<String> {
        media.mime_type.clone()
    }

    // =========================================================================
    // Env functions
    // =========================================================================

    fn env_get(vm: &mut BexVm, key: &str) -> Result<String, VmError> {
        vm.env_vars.get(key).cloned().ok_or_else(|| {
            VmError::RuntimeError(RuntimeError::Other(format!(
                "Environment variable '{key}' not found",
            )))
        })
    }
}

// =============================================================================
// Helper functions
// =============================================================================

/// Recursively deep copy a value, handling nested objects.
fn deep_copy_value_recursive(
    vm: &mut BexVm,
    value: Value,
    copied_objects: &mut HashMap<HeapPtr, HeapPtr>,
) -> NativeFunctionResult {
    match value {
        // Primitive values are copied by value
        Value::Null | Value::Int(_) | Value::Float(_) | Value::Bool(_) => Ok(value),

        // Objects need deep copying
        Value::Object(ptr) => {
            // Check if we've already copied this object (handles circular references)
            if let Some(&new_ptr) = copied_objects.get(&ptr) {
                return Ok(Value::Object(new_ptr));
            }

            // Clone the object first to avoid borrow checker issues
            let object = vm.get_object(ptr).clone();

            // Deep copy based on object type
            let new_ptr = match object {
                Object::String(s) => {
                    // Strings are immutable, but we still create a new copy
                    vm.tlab.alloc(Object::String(s))
                }

                Object::Array(values) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_ptr = vm.tlab.alloc(Object::Array(Vec::new()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    // Deep copy each element in the array
                    let mut new_values = Vec::with_capacity(values.len());
                    for value in values {
                        new_values.push(deep_copy_value_recursive(vm, value, copied_objects)?);
                    }

                    // Update the placeholder with the actual array
                    *vm.get_object_mut(placeholder_ptr) = Object::Array(new_values);
                    placeholder_ptr
                }

                Object::Map(map) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_ptr = vm.tlab.alloc(Object::Map(IndexMap::new()));
                    copied_objects.insert(ptr, placeholder_ptr);

                    // Deep copy each key-value pair
                    let mut new_map = IndexMap::new();
                    for (key, value) in &map {
                        let new_value = deep_copy_value_recursive(vm, *value, copied_objects)?;
                        new_map.insert(key.clone(), new_value);
                    }

                    // Update the placeholder with the actual map
                    *vm.get_object_mut(placeholder_ptr) = Object::Map(new_map);
                    placeholder_ptr
                }

                Object::Instance(instance) => {
                    // First, register a placeholder to handle circular references
                    let placeholder_ptr = vm.tlab.alloc(Object::Instance(Instance {
                        class: instance.class,
                        fields: Vec::new(),
                    }));
                    copied_objects.insert(ptr, placeholder_ptr);

                    // Deep copy each field in the instance
                    let mut new_fields = Vec::with_capacity(instance.fields.len());
                    for field in instance.fields {
                        new_fields.push(deep_copy_value_recursive(vm, field, copied_objects)?);
                    }

                    // Update the placeholder with the actual instance
                    *vm.get_object_mut(placeholder_ptr) = Object::Instance(Instance {
                        class: instance.class,
                        fields: new_fields,
                    });
                    placeholder_ptr
                }

                // These types don't contain nested objects that need deep copying
                Object::Function(f) => vm.tlab.alloc(Object::Function(f)),
                Object::Class(c) => vm.tlab.alloc(Object::Class(c)),
                Object::Enum(e) => vm.tlab.alloc(Object::Enum(e)),
                Object::Variant(v) => vm.tlab.alloc(Object::Variant(v)),
                Object::Media(m) => vm.tlab.alloc(Object::Media(m)),
                Object::Resource(r) => vm.tlab.alloc(Object::Resource(r)),
                Object::Future(f) => vm.tlab.alloc(Object::Future(f)),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(kind) => vm.tlab.alloc(Object::Sentinel(kind)),
            };

            // Record the mapping if not already done (for non-circular cases)
            copied_objects.entry(ptr).or_insert(new_ptr);

            Ok(Value::Object(new_ptr))
        }
    }
}

/// Recursively compare two values for deep equality
#[allow(clippy::float_cmp)]
fn deep_equals_recursive(
    vm: &BexVm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(HeapPtr, HeapPtr), bool>,
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
        (Value::Object(a_ptr), Value::Object(b_ptr)) => {
            // Check if same reference (optimization)
            if a_ptr == b_ptr {
                return true;
            }

            // Check if we've already compared these objects (circular reference handling)
            let key = if a_ptr < b_ptr {
                (a_ptr, b_ptr)
            } else {
                (b_ptr, a_ptr)
            };

            if let Some(&result) = visited.get(&key) {
                return result;
            }

            // Mark as being visited (assume equal to handle circular refs)
            visited.insert(key, true);

            // Compare based on object type
            let result = match (vm.get_object(a_ptr), vm.get_object(b_ptr)) {
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

                // Functions are compared by reference
                (Object::Function(_), Object::Function(_)) => a_ptr == b_ptr,

                // Future comparison - compare the inner values if both are ready
                (Object::Future(a_fut), Object::Future(b_fut)) => match (a_fut, b_fut) {
                    (Future::Ready(a_val), Future::Ready(b_val)) => {
                        deep_equals_recursive(vm, *a_val, *b_val, visited)
                    }
                    (Future::Pending(a_pend), Future::Pending(b_pend)) => {
                        a_pend.operation == b_pend.operation
                            && a_pend.args.len() == b_pend.args.len()
                            && a_pend
                                .args
                                .iter()
                                .zip(b_pend.args.iter())
                                .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                    }
                    _ => false,
                },

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

fn format_value_recursive(vm: &mut BexVm, value: &Value, depth: usize) -> Result<String, VmError> {
    // Check available stack space
    let available_frames = crate::vm::MAX_FRAMES.saturating_sub(vm.frames.len());

    if depth >= available_frames {
        return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
    }

    match value {
        Value::Null => Ok("null".to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(f.to_string()),
        Value::Bool(b) => Ok(b.to_string()),

        Value::Object(obj_idx) => match vm.get_object(*obj_idx) {
            Object::Instance(instance) => {
                let Object::Class(class) = vm.get_object(instance.class) else {
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
                            let _ = writeln!(result, "{field_indent}{fallback}: {formatted_value}");
                            continue;
                        }
                    };
                    let formatted_value = format_value_recursive(vm, field_value, depth + 1)?;
                    let _ = writeln!(result, "{field_indent}{field_name}: {formatted_value}");
                }

                let indent = "    ".repeat(depth);
                let _ = write!(result, "{indent}}}");
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

                for (key, value) in &map {
                    let formatted_value = format_value_recursive(vm, value, depth + 1)?;
                    let _ = writeln!(result, "{field_indent}\"{key}\": {formatted_value}");
                }

                let indent = "    ".repeat(depth);
                let _ = write!(result, "{indent}}}");
                Ok(result)
            }

            Object::String(s) => Ok(format!("\"{s}\"")),
            Object::Enum(e) => Ok(e.name.clone()),
            Object::Variant(variant) => {
                let Object::Enum(enm) = vm.get_object(variant.enm) else {
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
            Object::Media(m) => Ok(format!("<type {}>", m.kind)),
            Object::Resource(r) => Ok(format!("<{r}>")),
            Object::Future(_) => Ok("<future>".to_string()),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Ok("<sentinel>".to_string()),
        },
    }
}

/// Resolves native function pointers for unresolved native functions in objects.
///
/// At compile time, native functions are marked as `FunctionKind::NativeUnresolved`
/// because the compiler doesn't have access to the VM's native function table.
/// This function resolves those references by looking up the native function
/// implementations at runtime.
pub fn attach_builtins(object: Object) -> Result<Object, VmError> {
    Ok(match object {
        Object::Function(function) => {
            let kind = match function.kind {
                bex_vm_types::FunctionKind::Bytecode => bex_vm_types::FunctionKind::Bytecode,
                bex_vm_types::FunctionKind::External(op) => {
                    bex_vm_types::FunctionKind::External(op)
                }
                bex_vm_types::FunctionKind::NativeUnresolved => {
                    let Some(native_function) = crate::get_native_fn(function.name.as_str()) else {
                        return Err(VmError::RuntimeError(RuntimeError::Other(format!(
                            "Native function '{}' not found",
                            function.name
                        ))));
                    };
                    // Store as type-erased pointer
                    bex_vm_types::FunctionKind::Native(native_function as *const ())
                }
                bex_vm_types::FunctionKind::Native(ptr) => {
                    // Already resolved, pass through
                    bex_vm_types::FunctionKind::Native(ptr)
                }
            };
            Object::Function(bex_vm_types::Function {
                name: function.name,
                arity: function.arity,
                bytecode: function.bytecode,
                kind,
                locals_in_scope: function.locals_in_scope,
                span: function.span,
                block_notifications: function.block_notifications,
                viz_nodes: function.viz_nodes,
            })
        }
        // All other object types pass through unchanged
        other => other,
    })
}
