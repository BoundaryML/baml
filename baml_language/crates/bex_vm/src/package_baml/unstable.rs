use std::fmt::Write;

use bex_vm_types::types::{Object, Value, format_float};

use super::{BamlNamespaceUnstable, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmInternalError, VmPanic, VmRustFnError},
};

impl BamlNamespaceUnstable for PackageBamlImpl {
    fn string(vm: &BexVm, value: &Value) -> Result<String, VmRustFnError> {
        format_value_recursive(vm, value, 0)
    }
}

fn format_value_recursive(
    vm: &BexVm,
    value: &Value,
    depth: usize,
) -> Result<String, VmRustFnError> {
    let available_frames = crate::vm::MAX_FRAMES.saturating_sub(vm.frames.len());

    if depth >= available_frames {
        return Err(VmPanic::StackOverflow.into());
    }

    match value {
        Value::OmittedArg => Ok("<omitted>".to_string()),
        Value::Null => Ok("null".to_string()),
        Value::Int(i) => Ok(i.to_string()),
        Value::Float(f) => Ok(format_float(*f)),
        Value::Bool(b) => Ok(b.to_string()),

        Value::Object(obj_idx) => match vm.get_object(*obj_idx) {
            Object::Instance(instance) => {
                let class = vm.get_object(instance.class);
                let Object::Class(class) = class else {
                    return Err(VmInternalError::TypeError {
                        expected: ::bex_vm_types::types::Type::Object(
                            ::bex_vm_types::ObjectType::Class,
                        ),
                        got: ::bex_vm_types::types::Type::Object(::bex_vm_types::ObjectType::of(
                            class,
                        )),
                    }
                    .into());
                };

                let class_name = class.name.clone();
                let class_fields = class.fields.clone();
                let fields = instance.fields.clone();

                let mut result = format!("{class_name} {{\n");
                let field_indent = "    ".repeat(depth + 1);

                for (i, field_value) in fields.iter().enumerate() {
                    let field_name = match class_fields.get(i) {
                        Some(field) => field.name.as_str(),
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
            Object::Enum(e) => Ok(e.name.display_name.to_string()),
            Object::Variant(variant) => {
                let enm = vm.get_object(variant.enm);
                let Object::Enum(enm) = enm else {
                    return Err(VmInternalError::TypeError {
                        expected: ::bex_vm_types::types::Type::Object(
                            ::bex_vm_types::ObjectType::Enum,
                        ),
                        got: ::bex_vm_types::types::Type::Object(::bex_vm_types::ObjectType::of(
                            enm,
                        )),
                    }
                    .into());
                };

                let variant_name = match enm.variants.get(variant.index) {
                    Some(v) => v.name.clone(),
                    None => format!("variant_{}", variant.index),
                };
                Ok(variant_name)
            }
            Object::Function(f) => Ok(format!("<function {}>", f.name)),
            Object::Class(c) => Ok(format!("<class {}>", c.name)),
            Object::Future(_) => Ok("<future>".to_string()),
            Object::UnscheduledFuture(_) => Ok("<unscheduled future>".to_string()),
            Object::Collector(_) => Ok("<collector>".to_string()),
            Object::Type(ty) => Ok(format!("<type: {ty}>")),
            Object::Uint8Array(bytes) => Ok(format!("<uint8array len={}>", bytes.len())),
            Object::RustData(_) => Ok("<rust_data>".to_string()),
            Object::Closure(closure) => {
                Ok(format!("<closure captures={}>", closure.captures.len()))
            }
            Object::BoundMethod(_) => Ok("<bound_method>".to_string()),
            Object::Cell(cell) => Ok(format!("<cell {}>", cell.value)),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Ok("<sentinel>".to_string()),
        },
    }
}
