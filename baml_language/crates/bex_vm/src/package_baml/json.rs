//! Native handlers for `baml.json` namespace:
//! `parse`, `stringify`, and `stringify_pretty`.

use bex_vm_types::types::{Object, Value};
use indexmap::IndexMap;

use super::{BamlNamespaceJson, PackageBamlImpl};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

// ─── Constants ────────────────────────────────────────────────────────────────

const JSON_PARSE_ERROR_FQN: &str = "baml.json.JsonParseError";

// ─── Trait implementation ─────────────────────────────────────────────────────

impl BamlNamespaceJson for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
        json_parse(vm, s)
    }

    fn stringify(vm: &mut BexVm, j: &Value) -> String {
        let json_val = value_to_serde(vm, *j);
        serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string())
    }

    fn stringify_pretty(vm: &mut BexVm, j: &Value) -> String {
        let json_val = value_to_serde(vm, *j);
        serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| "null".to_string())
    }
}

// ─── Parse ────────────────────────────────────────────────────────────────────

/// Parse a JSON string and return a `json`-typed VM value.
///
/// The `json` type alias is `null | bool | int | float | string | json[] | map<string, json>`,
/// which maps directly onto VM value types:
/// - JSON `null`   → `Value::Null`
/// - JSON `bool`   → `Value::Bool`
/// - JSON integer  → `Value::Int`
/// - JSON float    → `Value::Float`
/// - JSON `string` → `Value::Object(String)`
/// - JSON array    → `Value::Object(Array)`
/// - JSON object   → `Value::Object(Map)`
///
/// On failure, throws a `baml.json.JsonParseError { message }` instance.
pub fn json_parse(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        let msg = e.to_string();
        match throw_json_parse_error(vm, msg) {
            Ok(v) => VmRustFnError::Thrown(v),
            Err(e) => VmRustFnError::InternalError(e),
        }
    })?;
    Ok(serde_to_value(vm, &parsed))
}

/// Allocate a `baml.json.JsonParseError { message }` instance and return it
/// as a `Value` suitable for `VmRustFnError::Thrown`.
fn throw_json_parse_error(vm: &mut BexVm, message: String) -> Result<Value, VmInternalError> {
    let class_ptr = vm
        .resolved_class_names
        .get(JSON_PARSE_ERROR_FQN)
        .copied()
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: JSON_PARSE_ERROR_FQN.to_string(),
        })?;
    let message_val = vm.alloc_string(message);
    Ok(vm.alloc_instance(class_ptr, vec![message_val]))
}

// ─── serde_json ↔ VM Value conversion ────────────────────────────────────────

/// Convert a `serde_json::Value` into a VM `Value`.
///
/// JSON numbers: integer-representable numbers become `Value::Int`; all others
/// become `Value::Float`.  This matches SAP's disambiguation behaviour.
pub fn serde_to_value(vm: &mut BexVm, v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                // Extremely large integer that doesn't fit in i64 — fall back to float.
                Value::Float(n.as_f64().unwrap_or(f64::NAN))
            }
        }
        serde_json::Value::String(s) => vm.alloc_string(s.clone()),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(|elem| serde_to_value(vm, elem)).collect();
            Value::Object(vm.tlab.alloc(Object::Array(items)))
        }
        serde_json::Value::Object(map) => {
            let entries: IndexMap<String, Value> = map
                .iter()
                .map(|(k, v)| (k.clone(), serde_to_value(vm, v)))
                .collect();
            Value::Object(vm.tlab.alloc(Object::Map(entries)))
        }
    }
}

/// Convert a VM `Value` into a `serde_json::Value` for serialization.
///
/// The `json` type only contains null, bool, int, float, string, array, and
/// map, so this is a total conversion for well-typed `json` values.
/// Non-`json`-representable values (class instances, enums, etc.) are
/// serialized as `null` with a best-effort fallback.
pub fn value_to_serde(vm: &BexVm, v: Value) -> serde_json::Value {
    match v {
        Value::Null => serde_json::Value::Null,
        Value::Bool(b) => serde_json::Value::Bool(b),
        Value::Int(i) => serde_json::Value::Number(i.into()),
        Value::Float(f) => serde_json::Number::from_f64(f)
            .map(serde_json::Value::Number)
            .unwrap_or(serde_json::Value::Null),
        Value::Object(ptr) => match vm.get_object(ptr) {
            Object::String(s) => serde_json::Value::String(s.clone()),
            Object::Array(arr) => {
                let arr = arr.clone();
                serde_json::Value::Array(arr.iter().map(|el| value_to_serde(vm, *el)).collect())
            }
            Object::Map(map) => {
                let map = map.clone();
                let entries: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_serde(vm, *v)))
                    .collect();
                serde_json::Value::Object(entries)
            }
            // Non-json-representable values: fall back to null.
            Object::Instance(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Variant(_)
            | Object::Function(_)
            | Object::Future(_)
            | Object::Collector(_)
            | Object::Type(_)
            | Object::Uint8Array(_)
            | Object::RustData(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::Cell(_) => serde_json::Value::Null,
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => serde_json::Value::Null,
        },
    }
}
