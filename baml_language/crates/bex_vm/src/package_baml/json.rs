//! Native handlers for `baml.json` namespace:
//! `parse`, `stringify`, `stringify_pretty`, `to_string<T>`, `from_string<T>`.
//!
//! `to_string<T>` and `from_string<T>` read their type-arg `T` from
//! `vm.current_call_type_args()`, populated by the call-instruction handler
//! from the leading `LoadType` operand.  See `BexVm::pending_call_type_args`.

// `path: &mut String` callees need ownership for `truncate` and `write!`.
// Match arms that throw via the VM error helpers read clearer than
// `let-else` since both branches contribute to the diagnostic.
#![allow(
    clippy::ptr_arg,
    clippy::manual_let_else,
    clippy::items_after_statements
)]

use std::sync::Arc;

use baml_type::{MediaKind, Ty, TypeName};

/// FQN of the recursive `json` type alias declared in `baml.json`.
/// Mirrors `baml_base::qualified_name::BAML_JSON_JSON`; inlined here to
/// avoid dragging the whole `baml_base` crate into `bex_vm` deps.
const BAML_JSON_JSON: &str = "baml.json.json";

/// Build the runtime registration key for a `Ty::Class(qtn, ...)` /
/// `Ty::Enum(qtn, _)` lookup against `BexVm::resolved_class_names`.
///
/// Compiler-side `qtn_to_type_name` strips the `user.` prefix from
/// user-defined types' `display_name` for nicer diagnostic strings, but
/// the runtime registration uses the full `package.namespace.name` form.
/// We rebuild that form here from `module_path + name`; for builtin types
/// (where `display_name` already encodes the full path) this also works
/// because `module_path` is the same path split on dots.
fn class_lookup_key(qtn: &TypeName) -> String {
    if qtn.module_path.is_empty() {
        qtn.name.to_string()
    } else {
        let mut buf = String::new();
        for (i, seg) in qtn.module_path.iter().enumerate() {
            if i > 0 {
                buf.push('.');
            }
            buf.push_str(seg.as_str());
        }
        buf.push('.');
        buf.push_str(qtn.name.as_str());
        buf
    }
}
use std::collections::HashMap;

use bex_vm_types::{
    HeapPtr,
    types::{Instance, Object, Value},
};
use indexmap::IndexMap;

use super::{
    BamlNamespaceJson, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee,
};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

/// Pass-through continuation for `baml.json.to_json(v)`. The dynamically
/// dispatched `to_json` produces the json value directly, so we just hand it
/// back to the caller.
struct ToJsonDynContinuation;

impl Continuation for ToJsonDynContinuation {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        NativeCallResult::Done(value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }

    fn apply_forwarding(&mut self, _forwarding: &HashMap<HeapPtr, HeapPtr>) {}
}

// ─── Constants ────────────────────────────────────────────────────────────────

const JSON_PARSE_ERROR_FQN: &str = "baml.json.JsonParseError";
const JSON_DECODE_ERROR_FQN: &str = "baml.json.JsonDecodeError";
const JSON_SERIALIZATION_ERROR_FQN: &str = "baml.json.JsonSerializationError";

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

    fn to_string(vm: &mut BexVm, v: &Value) -> Result<String, VmRustFnError> {
        let ty = vm
            .current_call_type_args()
            .first()
            .cloned()
            .ok_or_else(|| {
                VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                    name: "baml.json.to_string: missing type argument".to_string(),
                })
            })?;
        json_to_string_typed(vm, *v, &ty)
    }

    fn from_string(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
        let ty = vm
            .current_call_type_args()
            .first()
            .cloned()
            .ok_or_else(|| {
                VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                    name: "baml.json.from_string: missing type argument".to_string(),
                })
            })?;
        json_from_string_typed(vm, s, &ty)
    }

    fn to_json(vm: &mut BexVm, v: &Value) -> NativeCallResult {
        let v = *v;
        match make_to_json_callee(vm, v) {
            Ok(callee) => NativeCallResult::YieldToCall {
                callee,
                args: vec![],
                continuation: Box::new(ToJsonDynContinuation),
            },
            Err(e) => NativeCallResult::Error(e),
        }
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

// ─── Error throwers ───────────────────────────────────────────────────────────

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

fn throw_json_decode_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
) -> Result<Value, VmInternalError> {
    let class_ptr = vm
        .resolved_class_names
        .get(JSON_DECODE_ERROR_FQN)
        .copied()
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: JSON_DECODE_ERROR_FQN.to_string(),
        })?;
    let message_val = vm.alloc_string(message);
    let path_val = vm.alloc_string(path.to_string());
    Ok(vm.alloc_instance(class_ptr, vec![message_val, path_val]))
}

fn throw_json_serialization_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
    reason: &str,
) -> Result<Value, VmInternalError> {
    let class_ptr = vm
        .resolved_class_names
        .get(JSON_SERIALIZATION_ERROR_FQN)
        .copied()
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: JSON_SERIALIZATION_ERROR_FQN.to_string(),
        })?;
    let message_val = vm.alloc_string(message);
    let path_val = vm.alloc_string(path.to_string());
    let reason_val = vm.alloc_string(reason.to_string());
    Ok(vm.alloc_instance(class_ptr, vec![message_val, path_val, reason_val]))
}

fn raise_decode(vm: &mut BexVm, message: impl Into<String>, path: &str) -> VmRustFnError {
    match throw_json_decode_error(vm, message.into(), path) {
        Ok(v) => VmRustFnError::Thrown(v),
        Err(e) => VmRustFnError::InternalError(e),
    }
}

fn raise_serialize(
    vm: &mut BexVm,
    message: impl Into<String>,
    path: &str,
    reason: &str,
) -> VmRustFnError {
    match throw_json_serialization_error(vm, message.into(), path, reason) {
        Ok(v) => VmRustFnError::Thrown(v),
        Err(e) => VmRustFnError::InternalError(e),
    }
}

/// Public helper for native methods outside `json.rs` that need to throw a
/// `JsonSerializationError` without a path context (e.g. `Uint8Array.to_json`).
pub fn raise_serialize_no_path(
    vm: &mut BexVm,
    message: impl Into<String>,
    reason: &str,
) -> VmRustFnError {
    raise_serialize(vm, message, "", reason)
}

// ─── serde_json ↔ VM Value conversion (untyped) ──────────────────────────────

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

/// Convert a VM `Value` into a `serde_json::Value`, ignoring declared types.
///
/// Used for `Ty::TypeAlias(BAML_JSON_JSON)` and as a fallback for class fields
/// whose runtime `field_type` was erased (generic params lower to `Ty::Void`).
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

// ─── Typed JSON serialize ────────────────────────────────────────────────────

/// Serialize a VM `Value` to a JSON string driven by the runtime `Ty`.
///
/// Walks the value matching the shape of `ty`.  Throws
/// `JsonSerializationError` for non-representable types (`uint8array`,
/// function values, etc.).
pub fn json_to_string_typed(vm: &mut BexVm, v: Value, ty: &Ty) -> Result<String, VmRustFnError> {
    let mut path = String::new();
    let json_val = ty_value_to_serde(vm, v, ty, &mut path)?;
    serde_json::to_string(&json_val).map_err(|e| {
        raise_serialize(
            vm,
            format!("serde_json::to_string failed: {e}"),
            &path,
            "serde_json",
        )
    })
}

/// Walk `value` driven by `ty`, building a `serde_json::Value`.
///
/// `path` is mutated in place: `.field_name` is appended when descending into
/// class fields, `[i]` for list indices, `[\"key\"]` for map keys.  On any
/// failure the caller's `path` is left where the failure occurred so it can
/// be embedded in the thrown error.
fn ty_value_to_serde(
    vm: &mut BexVm,
    value: Value,
    ty: &Ty,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    match ty {
        // Primitive shapes: emit the value directly through value_to_serde,
        // which is total for scalar values.
        Ty::Null { .. } => Ok(serde_json::Value::Null),
        Ty::Int { .. } | Ty::Float { .. } | Ty::Bool { .. } | Ty::String { .. } => {
            Ok(value_to_serde(vm, value))
        }
        Ty::Literal(_, _) => Ok(value_to_serde(vm, value)),

        Ty::Optional(inner, _) => {
            if matches!(value, Value::Null) {
                Ok(serde_json::Value::Null)
            } else {
                ty_value_to_serde(vm, value, inner, path)
            }
        }

        Ty::List(elem, _) => {
            let items = match value {
                Value::Object(ptr) => match vm.get_object(ptr) {
                    Object::Array(arr) => arr.clone(),
                    _ => return Err(raise_serialize(vm, "expected array", path, "list")),
                },
                _ => return Err(raise_serialize(vm, "expected array", path, "list")),
            };
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                let saved_len = path.len();
                use std::fmt::Write;
                write!(path, "[{i}]").ok();
                let elem_json = ty_value_to_serde(vm, item, elem, path)?;
                path.truncate(saved_len);
                out.push(elem_json);
            }
            Ok(serde_json::Value::Array(out))
        }

        Ty::Map { value: vty, .. } => {
            let entries = match value {
                Value::Object(ptr) => match vm.get_object(ptr) {
                    Object::Map(m) => m.clone(),
                    _ => return Err(raise_serialize(vm, "expected map", path, "map")),
                },
                _ => return Err(raise_serialize(vm, "expected map", path, "map")),
            };
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                let saved_len = path.len();
                use std::fmt::Write;
                write!(path, "[{k:?}]").ok();
                let val_json = ty_value_to_serde(vm, v, vty, path)?;
                path.truncate(saved_len);
                out.insert(k, val_json);
            }
            Ok(serde_json::Value::Object(out))
        }

        Ty::TypeAlias(name, _) if name.display_name.as_str() == BAML_JSON_JSON => {
            Ok(value_to_serde(vm, value))
        }

        Ty::TypeAlias(_, _) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            Ok(value_to_serde(vm, value))
        }

        Ty::Class(qtn, _type_args, _) => serialize_class_instance(vm, value, qtn, path),

        Ty::Enum(_, _) => match value {
            Value::Object(ptr) => match vm.get_object(ptr) {
                Object::Variant(var) => {
                    let enm_ptr = var.enm;
                    let idx = var.index;
                    let variant_name = match vm.get_object(enm_ptr) {
                        Object::Enum(e) => e.variants[idx].name.clone(),
                        _ => {
                            return Err(raise_serialize(
                                vm,
                                "enum variant points to non-enum object",
                                path,
                                "enum",
                            ));
                        }
                    };
                    Ok(serde_json::Value::String(variant_name))
                }
                _ => Err(raise_serialize(vm, "expected enum variant", path, "enum")),
            },
            _ => Err(raise_serialize(vm, "expected enum variant", path, "enum")),
        },

        Ty::EnumVariant(_, name, _) => Ok(serde_json::Value::String(name.to_string())),

        Ty::Media(kind, _) => serialize_media(vm, value, *kind, path),

        Ty::Uint8Array { .. } => Err(raise_serialize(
            vm,
            "uint8array requires explicit encoding (use to_base64() or to_hex())",
            path,
            "uint8array",
        )),

        Ty::Union(_, _) => {
            // Tagged structurally — dispatch on the runtime Value shape rather
            // than trying each member.  This matches the json-alias union
            // semantics used in Phase 2.
            Ok(value_to_serde(vm, value))
        }

        Ty::Opaque(name, _) => Err(raise_serialize(
            vm,
            format!("cannot serialize opaque type `{name}`"),
            path,
            "opaque",
        )),

        // Compiler-only / non-representable variants.
        Ty::Function { .. } => Err(raise_serialize(
            vm,
            "cannot serialize function values",
            path,
            "function",
        )),
        Ty::Future(_, _) => Err(raise_serialize(
            vm,
            "cannot serialize future values",
            path,
            "future",
        )),
        Ty::WatchAccessor(_, _) => Err(raise_serialize(
            vm,
            "cannot serialize watch accessors",
            path,
            "watch_accessor",
        )),
        Ty::BuiltinUnknown { .. } => Err(raise_serialize(
            vm,
            "cannot serialize unknown type",
            path,
            "unknown",
        )),
        Ty::Void { .. } => {
            // `Ty::Void` shows up at runtime for class fields whose declared
            // type was a generic param (TypeVar erased to Void).  Fall back to
            // untyped serialization so generic class fields still round-trip
            // when the value's shape is JSON-representable.
            Ok(value_to_serde(vm, value))
        }
    }
}

/// Serialize a class instance: look up the runtime `Class`, iterate fields
/// by name, recurse on each field value with the declared `field_type`.
///
/// Special-cases media classes (`baml.media.Pdf`/`Audio`/`Video`/`Image`)
/// which are stored as `Object::Instance` with a `_data: Object::RustData`
/// field.  Detected by class FQN; a leading `Ty::Media(_)` would have
/// already routed through `serialize_media`.
fn serialize_class_instance(
    vm: &mut BexVm,
    value: Value,
    qtn: &TypeName,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    let inst_ptr = match value {
        Value::Object(ptr) => ptr,
        _ => {
            return Err(raise_serialize(
                vm,
                format!("expected class instance for `{qtn}`"),
                path,
                "class",
            ));
        }
    };
    let (class_ptr, class_type_args, field_values) = match vm.get_object(inst_ptr) {
        Object::Instance(inst) => (
            inst.class,
            inst.class_type_args.clone(),
            inst.fields.clone(),
        ),
        _ => {
            return Err(raise_serialize(
                vm,
                format!("expected class instance for `{qtn}`"),
                path,
                "class",
            ));
        }
    };

    if let Some(kind) = media_kind_from_fqn(qtn.display_name.as_str()) {
        return serialize_media(vm, value, kind, path);
    }

    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return Err(raise_serialize(
                vm,
                format!("instance class pointer for `{qtn}` is not a class"),
                path,
                "class",
            ));
        }
    };

    // Per BEP-038 (`@alias` / `@skip` are LLM-path-only): JSON interchange
    // always uses raw field names and includes every declared field, even
    // those marked `@skip`.  Aliased keys live exclusively on the
    // `ctx.output_format` / `$parse` LLM path.
    let mut out = serde_json::Map::with_capacity(class_fields.len());
    for (i, cf) in class_fields.iter().enumerate() {
        let Some(field_value) = field_values.get(i).copied() else {
            return Err(raise_serialize(
                vm,
                format!("class `{qtn}` has fewer fields than declared"),
                path,
                "class",
            ));
        };
        let saved_len = path.len();
        use std::fmt::Write;
        write!(path, ".{}", cf.name).ok();
        // Substitute class-level type-args into the field's template so
        // generic positions (`item: T` in `Container<T>`) resolve to the
        // concrete type carried on `Instance::class_type_args`.
        let field_ty = cf.field_template.substitute(&class_type_args);
        let field_json = ty_value_to_serde(vm, field_value, &field_ty, path)?;
        path.truncate(saved_len);
        out.insert(cf.name.clone(), field_json);
    }
    Ok(serde_json::Value::Object(out))
}

fn media_kind_from_fqn(fqn: &str) -> Option<MediaKind> {
    match fqn {
        "baml.media.Image" => Some(MediaKind::Image),
        "baml.media.Audio" => Some(MediaKind::Audio),
        "baml.media.Video" => Some(MediaKind::Video),
        "baml.media.Pdf" => Some(MediaKind::Pdf),
        _ => None,
    }
}

fn media_kind_str(kind: MediaKind) -> &'static str {
    match kind {
        MediaKind::Image => "image",
        MediaKind::Audio => "audio",
        MediaKind::Video => "video",
        MediaKind::Pdf => "pdf",
        MediaKind::Generic => "media",
    }
}

/// Emit a tagged JSON object for a media value.
///
/// Shape: `{ "kind": "image"|..., "source": "url"|"file"|"base64", "value":
/// <data>, "mime": <mime_type-or-null> }`.  The `value` is the URL, the file
/// path, or the base64 payload depending on `source`.
fn serialize_media(
    vm: &mut BexVm,
    value: Value,
    kind: MediaKind,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    let media = read_media_value(vm, value)
        .ok_or_else(|| raise_serialize(vm, "expected media instance", path, "media"))?;

    let (source, payload) = if let Some(url) = media.url() {
        ("url", url)
    } else if let Some(file) = media.file() {
        ("file", file)
    } else {
        ("base64", media.base64())
    };

    let mut obj = serde_json::Map::new();
    obj.insert(
        "kind".into(),
        serde_json::Value::String(media_kind_str(kind).into()),
    );
    obj.insert("source".into(), serde_json::Value::String(source.into()));
    obj.insert("value".into(), serde_json::Value::String(payload));
    obj.insert(
        "mime".into(),
        match media.mime_type() {
            Some(m) => serde_json::Value::String(m),
            None => serde_json::Value::Null,
        },
    );
    Ok(serde_json::Value::Object(obj))
}

fn read_media_value(vm: &BexVm, value: Value) -> Option<Arc<baml_builtins2::MediaValue>> {
    let ptr = match value {
        Value::Object(p) => p,
        _ => return None,
    };
    let inst = match vm.get_object(ptr) {
        Object::Instance(inst) => inst,
        _ => return None,
    };
    // Media classes have a single `_data: $rust_type` field.
    let data_value = *inst.fields.first()?;
    let data_ptr = match data_value {
        Value::Object(p) => p,
        _ => return None,
    };
    match vm.get_object(data_ptr) {
        Object::RustData(arc) => arc.clone().downcast::<baml_builtins2::MediaValue>().ok(),
        _ => None,
    }
}

// ─── Typed JSON deserialize ──────────────────────────────────────────────────

/// Parse a JSON string and coerce it to a VM `Value` of the given runtime
/// `Ty`.
///
/// Throws `JsonParseError` for invalid JSON and `JsonDecodeError` when the
/// parsed JSON does not match the target type.
pub fn json_from_string_typed(vm: &mut BexVm, s: &str, ty: &Ty) -> Result<Value, VmRustFnError> {
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        let msg = e.to_string();
        match throw_json_parse_error(vm, msg) {
            Ok(v) => VmRustFnError::Thrown(v),
            Err(e) => VmRustFnError::InternalError(e),
        }
    })?;
    let mut path = String::new();
    ty_serde_to_value(vm, &parsed, ty, &mut path)
}

/// Walk a parsed `serde_json::Value` driven by `ty`, allocating VM values.
/// Throws `JsonDecodeError` on shape mismatch.
fn ty_serde_to_value(
    vm: &mut BexVm,
    json: &serde_json::Value,
    ty: &Ty,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    match ty {
        Ty::Null { .. } => match json {
            serde_json::Value::Null => Ok(Value::Null),
            _ => Err(raise_decode(vm, "expected null", path)),
        },

        Ty::Bool { .. } => match json {
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            _ => Err(raise_decode(vm, "expected boolean", path)),
        },

        Ty::Int { .. } => match json {
            serde_json::Value::Number(n) => n
                .as_i64()
                .map(Value::Int)
                .ok_or_else(|| raise_decode(vm, "expected integer", path)),
            _ => Err(raise_decode(vm, "expected integer", path)),
        },

        Ty::Float { .. } => match json {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(Value::Float(f))
                } else {
                    Err(raise_decode(vm, "expected number", path))
                }
            }
            _ => Err(raise_decode(vm, "expected number", path)),
        },

        Ty::String { .. } => match json {
            serde_json::Value::String(s) => Ok(vm.alloc_string(s.clone())),
            _ => Err(raise_decode(vm, "expected string", path)),
        },

        Ty::Optional(inner, _) => match json {
            serde_json::Value::Null => Ok(Value::Null),
            _ => ty_serde_to_value(vm, json, inner, path),
        },

        Ty::List(elem, _) => match json {
            serde_json::Value::Array(arr) => {
                let mut items = Vec::with_capacity(arr.len());
                for (i, item) in arr.iter().enumerate() {
                    let saved_len = path.len();
                    use std::fmt::Write;
                    write!(path, "[{i}]").ok();
                    let v = ty_serde_to_value(vm, item, elem, path)?;
                    path.truncate(saved_len);
                    items.push(v);
                }
                Ok(Value::Object(vm.tlab.alloc(Object::Array(items))))
            }
            _ => Err(raise_decode(vm, "expected array", path)),
        },

        Ty::Map { value: vty, .. } => match json {
            serde_json::Value::Object(map) => {
                let mut entries: IndexMap<String, Value> = IndexMap::with_capacity(map.len());
                for (k, val) in map {
                    let saved_len = path.len();
                    use std::fmt::Write;
                    write!(path, "[{k:?}]").ok();
                    let v = ty_serde_to_value(vm, val, vty, path)?;
                    path.truncate(saved_len);
                    entries.insert(k.clone(), v);
                }
                Ok(Value::Object(vm.tlab.alloc(Object::Map(entries))))
            }
            _ => Err(raise_decode(vm, "expected object", path)),
        },

        Ty::TypeAlias(name, _) if name.display_name.as_str() == BAML_JSON_JSON => {
            Ok(serde_to_value(vm, json))
        }

        Ty::TypeAlias(_, _) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            Ok(serde_to_value(vm, json))
        }

        Ty::Class(qtn, type_args, _) => {
            if let Some(kind) = media_kind_from_fqn(qtn.display_name.as_str()) {
                return deserialize_media(vm, json, kind, qtn, path);
            }
            deserialize_class_instance(vm, json, qtn, type_args, path)
        }

        Ty::Enum(qtn, _) => match json {
            serde_json::Value::String(s) => deserialize_enum_variant(vm, qtn, s, path),
            _ => Err(raise_decode(vm, "expected enum variant string", path)),
        },

        Ty::EnumVariant(qtn, name, _) => match json {
            serde_json::Value::String(s) if s == name.as_str() => {
                deserialize_enum_variant(vm, qtn, s, path)
            }
            _ => Err(raise_decode(
                vm,
                format!("expected enum variant `{name}`"),
                path,
            )),
        },

        Ty::Media(kind, _) => deserialize_media_by_kind(vm, json, *kind, path),

        Ty::Uint8Array { .. } => Err(raise_decode(
            vm,
            "uint8array requires explicit encoding (use from_base64() or from_hex())",
            path,
        )),

        Ty::Union(members, _) => {
            // Try each member structurally; first match wins.
            for member in members {
                let mut tmp_path = path.clone();
                if let Ok(v) = ty_serde_to_value(vm, json, member, &mut tmp_path) {
                    return Ok(v);
                }
            }
            Err(raise_decode(vm, "no union member matched", path))
        }

        Ty::Literal(lit, _) => match (lit, json) {
            (baml_type::Literal::Bool(b), serde_json::Value::Bool(jb)) if b == jb => {
                Ok(Value::Bool(*jb))
            }
            (baml_type::Literal::String(s), serde_json::Value::String(js)) if s == js => {
                Ok(vm.alloc_string(js.clone()))
            }
            (baml_type::Literal::Int(expected), serde_json::Value::Number(n)) => {
                if let Some(actual) = n.as_i64() {
                    if *expected == actual {
                        return Ok(Value::Int(actual));
                    }
                }
                Err(raise_decode(vm, "literal int mismatch", path))
            }
            (baml_type::Literal::Float(s), serde_json::Value::Number(n)) => {
                if let (Ok(expected), Some(actual)) = (s.parse::<f64>(), n.as_f64()) {
                    if (expected - actual).abs() < f64::EPSILON {
                        return Ok(Value::Float(actual));
                    }
                }
                Err(raise_decode(vm, "literal float mismatch", path))
            }
            _ => Err(raise_decode(vm, "literal mismatch", path)),
        },

        Ty::Opaque(name, _) => Err(raise_decode(
            vm,
            format!("cannot deserialize opaque type `{name}`"),
            path,
        )),

        Ty::Function { .. }
        | Ty::Future(_, _)
        | Ty::WatchAccessor(_, _)
        | Ty::BuiltinUnknown { .. }
        | Ty::Void { .. } => {
            // `Ty::Void` reaches here for generic class fields whose
            // declared type was a TypeVar.  Fall back to untyped conversion
            // so generic-position fields still round-trip when the JSON is
            // a `json`-shaped value.
            Ok(serde_to_value(vm, json))
        }
    }
}

fn deserialize_class_instance(
    vm: &mut BexVm,
    json: &serde_json::Value,
    qtn: &TypeName,
    type_args: &[Ty],
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let map = match json {
        serde_json::Value::Object(m) => m,
        _ => {
            return Err(raise_decode(
                vm,
                format!("expected JSON object for class `{qtn}`"),
                path,
            ));
        }
    };

    let key = class_lookup_key(qtn);
    let class_ptr = vm
        .resolved_class_names
        .get(&key)
        .copied()
        .ok_or_else(|| raise_decode(vm, format!("class `{qtn}` not found"), path))?;
    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return Err(raise_decode(vm, format!("`{qtn}` is not a class"), path));
        }
    };

    // Per BEP-038: JSON interchange uses raw field names — `@alias` /
    // `@skip` are ignored on the from_string path the same way they are on
    // to_string.  Every declared field must be provided (or, for
    // `Optional<T>`, may be absent).
    let mut field_values: Vec<Value> = Vec::with_capacity(class_fields.len());
    for cf in &class_fields {
        let saved_len = path.len();
        use std::fmt::Write;
        write!(path, ".{}", cf.name).ok();
        // Substitute class-level type-args into the field's template so a
        // `Container<User>::item` field decodes against `User` rather than
        // the erased `Ty::Void`.
        let field_ty = cf.field_template.substitute(type_args);
        let field_json_owned;
        let field_json: &serde_json::Value = if let Some(v) = map.get(cf.name.as_str()) {
            v
        } else if matches!(field_ty, Ty::Optional(_, _)) {
            field_json_owned = serde_json::Value::Null;
            &field_json_owned
        } else {
            return Err(raise_decode(
                vm,
                format!("missing required field `{}`", cf.name),
                path,
            ));
        };
        let v = ty_serde_to_value(vm, field_json, &field_ty, path)?;
        path.truncate(saved_len);
        field_values.push(v);
    }

    Ok(Value::Object(vm.tlab.alloc(Object::Instance(Instance {
        class: class_ptr,
        class_type_args: type_args.to_vec(),
        fields: field_values,
    }))))
}

fn deserialize_enum_variant(
    vm: &mut BexVm,
    qtn: &TypeName,
    variant_name: &str,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let key = class_lookup_key(qtn);
    let enm_ptr = vm
        .resolved_class_names
        .get(&key)
        .copied()
        .ok_or_else(|| raise_decode(vm, format!("enum `{qtn}` not found"), path))?;
    let idx = match vm.get_object(enm_ptr) {
        Object::Enum(e) => e.variants.iter().position(|v| v.name == variant_name),
        _ => {
            return Err(raise_decode(vm, format!("`{qtn}` is not an enum"), path));
        }
    };
    match idx {
        Some(i) => Ok(vm.alloc_variant(enm_ptr, i)),
        None => Err(raise_decode(
            vm,
            format!("unknown variant `{variant_name}` for enum `{qtn}`"),
            path,
        )),
    }
}

fn deserialize_media_by_kind(
    vm: &mut BexVm,
    json: &serde_json::Value,
    kind: MediaKind,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let class_short = match kind {
        MediaKind::Image => "Image",
        MediaKind::Audio => "Audio",
        MediaKind::Video => "Video",
        MediaKind::Pdf => "Pdf",
        MediaKind::Generic => {
            return Err(raise_decode(
                vm,
                "cannot deserialize generic media — type must be concrete (image|audio|video|pdf)",
                path,
            ));
        }
    };
    let fqn_string = format!("baml.media.{class_short}");
    let qtn = TypeName::from_dotted_path(&fqn_string);
    deserialize_media(vm, json, kind, &qtn, path)
}

fn deserialize_media(
    vm: &mut BexVm,
    json: &serde_json::Value,
    kind: MediaKind,
    qtn: &TypeName,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let map = match json {
        serde_json::Value::Object(m) => m,
        _ => {
            return Err(raise_decode(
                vm,
                "expected tagged media object {kind, source, value, mime}",
                path,
            ));
        }
    };
    let source = map
        .get("source")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| raise_decode(vm, "media object missing `source`", path))?;
    let value_str = map
        .get("value")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| raise_decode(vm, "media object missing `value`", path))?;
    let mime = map.get("mime").and_then(serde_json::Value::as_str);

    let media_arc: Arc<baml_builtins2::MediaValue> = match source {
        "url" => baml_builtins2::MediaValue::from_url(kind, value_str, mime),
        "file" => baml_builtins2::MediaValue::from_file(kind, value_str, mime),
        "base64" | "inline" => baml_builtins2::MediaValue::from_base64(kind, value_str, mime),
        other => {
            return Err(raise_decode(
                vm,
                format!("unknown media source `{other}` (expected url|file|base64)"),
                path,
            ));
        }
    };

    let key = class_lookup_key(qtn);
    let class_ptr = vm
        .resolved_class_names
        .get(&key)
        .copied()
        .ok_or_else(|| raise_decode(vm, format!("media class `{qtn}` not found"), path))?;
    let data_val = vm.alloc_rust_data(media_arc);
    Ok(vm.alloc_instance(class_ptr, vec![data_val]))
}
