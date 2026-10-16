//! `serde_json::Value` and VM values, both ways, for the callers that speak
//! `serde_json`: `baml.sap` and the prompt renderer. `baml.json` itself reads and
//! writes text without a `serde_json` tree in between.

use std::sync::Arc;

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, RealizedTy, Value,
    types::{Array, Object},
};
use indexmap::IndexMap;

use super::{CYCLIC_JSON, STRING_MAP_REQUIRED, json_alias_ty, parse_decimal_bigint};
use crate::BexVm;

/// Run `f` with `seg` appended to `path`, then restore `path` to its prior
/// length.
fn with_path_segment<F, R>(path: &mut String, seg: std::fmt::Arguments<'_>, f: F) -> R
where
    F: FnOnce(&mut String) -> R,
{
    use std::fmt::Write;
    let saved_len = path.len();
    let _ = write!(path, "{seg}");
    let r = f(path);
    path.truncate(saved_len);
    r
}

fn string_map_entries(
    vm: &BexVm,
    map: &bex_vm_types::types::Map,
) -> Result<Vec<(bex_vm_types::BexStr, Value)>, &'static str> {
    if !super::string_keyed(vm, map) {
        return Err(STRING_MAP_REQUIRED);
    }
    map.snapshot_entries()
        .into_iter()
        .map(
            |(key, value)| match key.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
                Some(Object::String(key)) => Ok((key.clone(), value)),
                _ => Err(STRING_MAP_REQUIRED),
            },
        )
        .collect()
}

/// Convert a `serde_json::Value` into a VM `Value`.
///
/// JSON numbers: i63-representable integers become integers, larger integral
/// tokens become bigints, and tokens with a fraction or exponent become
/// heap-boxed floats.
pub(in crate::package_baml) fn serde_to_value(vm: &mut BexVm, v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::NULL,
        serde_json::Value::Bool(b) => Value::bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64()
                && let Some(v) = Value::try_int(i)
            {
                v
            } else if let Some(bigint) = parse_json_bigint(v) {
                // `parse_json_bigint` already enforces the VM's bit limit.
                Value::object(vm.tlab.alloc(Object::Bigint(Arc::new(bigint))))
            } else if let Some(f) = n.as_f64() {
                Value::object(vm.alloc_float(f))
            } else {
                // With `arbitrary_precision`, a non-integral number whose
                // magnitude cannot fit in an f64 reaches here. NaN preserves
                // the existing sentinel for a number we cannot represent.
                Value::object(vm.alloc_float(f64::NAN))
            }
        }
        serde_json::Value::String(s) => Value::object(vm.alloc_string(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(|elem| serde_to_value(vm, elem)).collect();
            // Untyped JSON: elements are `json` values.
            Value::object(
                vm.tlab
                    .alloc(Object::Array(Array::new(json_alias_ty(vm), items))),
            )
        }
        serde_json::Value::Object(map) => {
            let entries: IndexMap<bex_vm_types::BexStr, Value> = map
                .iter()
                .map(|(k, v)| {
                    (
                        bex_vm_types::BexStr::from(k.as_str()),
                        serde_to_value(vm, v),
                    )
                })
                .collect();
            // Untyped JSON object: string keys, `json` values.
            Value::object(
                vm.tlab
                    .alloc_map(RealizedTy::string(), json_alias_ty(vm), entries),
            )
        }
    }
}

/// Convert a VM `Value` into a `serde_json::Value`, ignoring declared types.
///
/// Used for `RealizedTy::TypeAlias(BAML_JSON_JSON)` and for class fields whose runtime
/// field type is intentionally untyped or unavailable.
/// Non-string maps are rejected at any depth, retaining the failing value's path.
fn value_to_serde(vm: &BexVm, v: Value) -> Result<serde_json::Value, ValueToSerdeError> {
    value_to_serde_at(vm, v, &mut String::new(), &mut Vec::new())
}

/// A value the walk met that no `json` value is: the caller's `json` typing
/// was broken before the value got here.
const NOT_JSON: &str = "not a json value";

#[derive(Debug)]
struct ValueToSerdeError {
    message: &'static str,
    path: String,
}

/// Convert a statically checked `json` value at a throws-never boundary.
pub(in crate::package_baml) fn json_value_to_serde(vm: &BexVm, v: Value) -> serde_json::Value {
    value_to_serde(vm, v).unwrap_or_else(|e| {
        unreachable!(
            "invalid statically typed json value: {} at `{}`",
            e.message, e.path
        )
    })
}

/// `active` holds the JSON objects and arrays the walk is inside: meeting one
/// again is a value that contains itself, refused at `path`.
fn value_to_serde_at(
    vm: &BexVm,
    v: Value,
    path: &mut String,
    active: &mut Vec<HeapPtr>,
) -> Result<serde_json::Value, ValueToSerdeError> {
    use bex_vm_types::ValueKind;
    Ok(match v.kind() {
        ValueKind::Null => serde_json::Value::Null,
        ValueKind::Bool(b) => serde_json::Value::Bool(b),
        ValueKind::Int(i) => serde_json::Value::Number(i.into()),
        ValueKind::OmittedArg => serde_json::Value::Null,
        ValueKind::Object(ptr) => {
            let is_container = matches!(vm.get_object(ptr), Object::Array(_) | Object::Map(_));
            if is_container {
                if active.contains(&ptr) {
                    return Err(ValueToSerdeError {
                        message: CYCLIC_JSON,
                        path: path.clone(),
                    });
                }
                active.push(ptr);
            }
            let converted = object_to_serde_at(vm, ptr, path, active);
            if is_container {
                active.pop();
            }
            converted?
        }
    })
}

/// The heap object at `ptr` as JSON ([`value_to_serde_at`]'s object arm).
fn object_to_serde_at(
    vm: &BexVm,
    ptr: HeapPtr,
    path: &mut String,
    active: &mut Vec<HeapPtr>,
) -> Result<serde_json::Value, ValueToSerdeError> {
    Ok(match vm.get_object(ptr) {
        Object::Float(f) => serde_json::Number::from_f64(*f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Object::String(s) => serde_json::Value::String(s.to_string()),
        Object::Array(arr) => {
            let arr = arr.to_vec();
            let mut out = Vec::with_capacity(arr.len());
            for (i, el) in arr.into_iter().enumerate() {
                out.push(with_path_segment(path, format_args!("[{i}]"), |p| {
                    value_to_serde_at(vm, el, p, active)
                })?);
            }
            serde_json::Value::Array(out)
        }
        Object::Map(map) => {
            let map = string_map_entries(vm, map).map_err(|message| ValueToSerdeError {
                message,
                path: path.clone(),
            })?;
            let mut entries = serde_json::Map::with_capacity(map.len());
            for (k, v) in map {
                let value = with_path_segment(path, format_args!("[{k:?}]"), |p| {
                    value_to_serde_at(vm, v, p, active)
                })?;
                entries.insert(k.to_string(), value);
            }
            serde_json::Value::Object(entries)
        }
        Object::Bigint(bi) => bigint_to_serde(bi),
        // An enum variant renders as its variant-name string, matching the
        // structural walker (`render_to_serde`).
        Object::Variant(var) => {
            let (enm, index) = (var.enm, var.index);
            match vm.get_object(enm) {
                Object::Enum(e) => match e.variants.get(index) {
                    Some(v) => serde_json::Value::String(v.name.clone()),
                    None => unreachable!(
                        "variant {index} of enum `{}` is out of range at `{path}`",
                        e.name()
                    ),
                },
                _ => unreachable!("a variant's enum is not an enum at `{path}`"),
            }
        }
        // A capture cell stands for the value it holds, as membership reads it.
        Object::Cell(cell) => return value_to_serde_at(vm, cell.load(), path, active),
        Object::Instance(_)
        | Object::Class(_)
        | Object::Enum(_)
        | Object::TypeAlias(_)
        | Object::Interface(_)
        | Object::Package(_)
        | Object::ImplRule(_)
        | Object::Function(_)
        | Object::Future(_)
        | Object::Type(_)
        | Object::Uint8Array(_)
        | Object::RustData(_)
        | Object::RustInstance(_)
        | Object::Closure(_)
        | Object::BoundMethod(_)
        | Object::GenericFunction(_)
        | Object::HostClosure(_) => {
            return Err(ValueToSerdeError {
                message: NOT_JSON,
                path: path.clone(),
            });
        }
        Object::Tombstone => Object::tombstone_reached(),
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => {
            return Err(ValueToSerdeError {
                message: NOT_JSON,
                path: path.clone(),
            });
        }
    })
}

/// Convert a bigint to the arbitrary-precision JSON number representation used
/// by `serde_json`. `BigInt`'s decimal display is always valid JSON number syntax.
fn bigint_to_serde(value: &num_bigint::BigInt) -> serde_json::Value {
    match value.to_string().parse::<serde_json::Number>() {
        Ok(number) => serde_json::Value::Number(number),
        Err(_) => unreachable!("BigInt decimal display must be a valid JSON number"),
    }
}

/// Decode an integral JSON number token without passing through `f64`.
fn parse_json_bigint(json: &serde_json::Value) -> Option<num_bigint::BigInt> {
    let serde_json::Value::Number(number) = json else {
        return None;
    };
    parse_decimal_bigint(&number.to_string())
}
