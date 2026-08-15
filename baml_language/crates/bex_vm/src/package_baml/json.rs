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

use baml_type::{MediaKind, RealizedTy, TyTemplate, TypeName};

/// FQN of the recursive `json` type alias declared in `baml.json`.
/// Mirrors `baml_base::qualified_name::BAML_JSON_JSON`; inlined here to
/// avoid dragging the whole `baml_base` crate into `bex_vm` deps.
const BAML_JSON_JSON: &str = "baml.json.json";

/// The runtime type of an untyped `json` value: the recursive `baml.json.json`
/// alias (`null | bool | int | float | string | json[] | map<string, json>`).
/// Recursive aliases stay opaque in `RealizedTy`, so this is the most precise
/// element/value type available for containers parsed from untyped JSON.
pub(super) fn json_alias_ty() -> RealizedTy {
    RealizedTy::TypeAlias(
        TypeName::from_dotted_path(BAML_JSON_JSON),
        baml_type::TyAttr::default(),
    )
}

/// Run `f` with `seg` appended to `path`, then restore `path` to its prior
/// length. Used to track the JSON pointer during recursive (de)serialization
/// without mutating the buffer's owner contract.
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

/// Build the runtime registration key for a `RealizedTy::Class(qtn, ...)` /
/// `RealizedTy::Enum(qtn, _)` lookup against `BexVm::resolved_class_names`.
///
/// Compiler-side `display_name` strips the `user.` prefix from
/// user-defined types for nicer diagnostic strings, but
/// the runtime registration uses the full `package.namespace.name` form.
/// We rebuild that form here from `module_path + name`; for builtin types
/// (where `display_name` already encodes the full path) this also works
/// because `module_path` is the same path split on dots.
fn class_lookup_key(qtn: &TypeName) -> String {
    qtn.render_dotted(false)
}
use std::collections::HashMap;

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, ValueKind,
    types::{Array, Instance, Map, Object, Value},
};
use indexmap::IndexMap;

use super::{
    BamlNamespaceJson, Continuation, NativeCallResult, PackageBamlImpl,
    make_to_json_override_callee, to_json_override_fn_name,
};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

// ─── baml.json.from / baml.ToJson default (override-honoring structural json) ──
//
// `baml.json.from(value)` and the `baml.ToJson` interface default body both
// render `value` to a `json` value, honoring `baml.ToJson` overrides at every
// depth: `value`'s own override (if any) wins, and any *nested* value whose
// runtime class overrides `to_json` is rendered via that override rather than
// structurally. The json analog of `string.from` / `baml.ToString` in `root.rs`.
//
// Three passes, mirroring `render_to_string_honoring_overrides`:
//   1. `collect_to_json_overrides` — pre-order DFS recording every sub-value
//      whose runtime class overrides `baml.ToJson` (allocation-free).
//   2. one `YieldToCall` per override, dispatching its `to_json`; each result
//      `json` value is normalized to `serde_json::Value` immediately so the
//      continuation holds no extra heap roots.
//   3. `render_to_serde` — structural walk to a `serde_json::Value`, splicing the
//      override results in by pre-order position, then `serde_to_value` once.
//
// Building the structural skeleton in serde space and materializing the `json`
// value with a single `serde_to_value` at the end keeps the walk allocation-free
// (no GC can move `pending`/`root` mid-render), unlike a value-kind walk that
// allocated heap arrays/maps as it descended.

/// Entry point for `baml._to_json_default` / `baml._to_json_shim`. Collects the
/// override-bearing sub-values (pass 1), dispatches `to_json` on each in order
/// (pass 2), then renders structurally splicing in the override results (pass 3).
pub(super) fn render_to_json_honoring_overrides(vm: &mut BexVm, value: Value) -> NativeCallResult {
    let mut pending: Vec<HeapPtr> = Vec::new();
    collect_to_json_overrides(vm, value, &mut pending);

    let Some(&first_ptr) = pending.first() else {
        return render_to_json_done(vm, value, &pending, &[]);
    };
    match make_to_json_override_callee(vm, Value::object(first_ptr)) {
        Some(callee) => NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(ToJsonWalkContinuation {
                root: value,
                pending,
                results: Vec::new(),
            }),
        },
        None => render_to_json_done(vm, value, &pending, &[]),
    }
}

/// Whether `value`'s runtime class carries an in-body `baml.ToJson` override.
/// Shares `make_to_json_override_callee`'s resolution but allocates nothing on
/// the VM heap, so it is safe during the allocation-free pre-order collection.
fn has_to_json_override(vm: &BexVm, value: Value) -> bool {
    to_json_override_fn_name(vm, value)
        .and_then(|name| vm.find_function_by_name(&name))
        .is_some()
}

/// Pre-order DFS collecting, by heap pointer and in render order, every
/// sub-value of `value` whose runtime class overrides `baml.ToJson`. An override
/// node is recorded and *not* descended into — its `to_json` owns its whole
/// subtree. Immutable and allocation-free so the collector cannot move objects
/// mid-walk. Matches `render_to_serde`'s traversal order (array elements, then
/// map values, then instance fields) so the two stay index-aligned. A media
/// instance is treated as a leaf, matching `render_to_serde`, which emits its
/// tagged form without descending into the opaque `_data` field.
fn collect_to_json_overrides(vm: &BexVm, value: Value, out: &mut Vec<HeapPtr>) {
    let ValueKind::Object(ptr) = value.kind() else {
        return;
    };
    if has_to_json_override(vm, value) {
        out.push(ptr);
        return;
    }
    // Snapshot children (owned), dropping the heap borrow before recursing.
    let children: Vec<Value> = match vm.get_object(ptr) {
        Object::Array(values) => values.to_vec(),
        Object::Map(map) => map.to_index_map().values().copied().collect(),
        Object::Instance(inst) => {
            // Media instances render as a leaf (their tagged form); their single
            // `_data` field is opaque `RustData` and carries no overrides, so
            // skipping the descent both matches `render_to_serde` and is sound.
            if is_media_instance(vm, inst) {
                Vec::new()
            } else {
                inst.field_values().collect()
            }
        }
        _ => Vec::new(),
    };
    for v in children {
        collect_to_json_overrides(vm, v, out);
    }
}

/// Whether `inst`'s class is one of the builtin media classes.
fn is_media_instance(vm: &BexVm, inst: &Instance) -> bool {
    match vm.get_object(inst.class) {
        Object::Class(c) => media_kind_from_fqn(c.name.render_dotted(false).as_str()).is_some(),
        _ => false,
    }
}

/// Pass 3: render `root` structurally to a `serde_json::Value` (splicing the
/// precomputed override `results` by pre-order position), then materialize the
/// `json` value with a single `serde_to_value`.
fn render_to_json_done(
    vm: &mut BexVm,
    root: Value,
    pending: &[HeapPtr],
    results: &[serde_json::Value],
) -> NativeCallResult {
    let mut counter = 0;
    let mut path = String::new();
    match render_to_serde(vm, root, pending, results, &mut counter, &mut path) {
        Ok(serde) => NativeCallResult::Done(serde_to_value(vm, &serde)),
        Err(e) => NativeCallResult::Error(e),
    }
}

/// Structural json rendering used by `baml.json.from` / the `baml.ToJson`
/// default. Mirrors `root.rs`'s `render_to_string`, but produces a
/// `serde_json::Value` instead of a `String` and can fail: a value with no json
/// representation (`uint8array` without explicit encoding, functions, futures,
/// ...) raises `JsonSerializationError`. A node whose runtime class overrides
/// `baml.ToJson` (recorded pre-order in `pending` by `collect_to_json_overrides`)
/// is rendered via its precomputed `results[*counter]`. Because collect and
/// render share the same pre-order, `pending[*counter]` is exactly the next
/// override node, so the check is a pointer compare. With an empty `pending` this
/// is a pure structural walk. Allocation-free w.r.t. the VM heap on the success
/// path (builds only serde + Rust strings), so GC cannot move `pending`/`root`
/// mid-render.
fn render_to_serde(
    vm: &mut BexVm,
    value: Value,
    pending: &[HeapPtr],
    results: &[serde_json::Value],
    counter: &mut usize,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    let ptr = match value.kind() {
        ValueKind::Null | ValueKind::OmittedArg => return Ok(serde_json::Value::Null),
        ValueKind::Bool(b) => return Ok(serde_json::Value::Bool(b)),
        ValueKind::Int(i) => return Ok(serde_json::Value::Number(i.into())),
        ValueKind::Object(ptr) => ptr,
    };

    // Override node: splice its precomputed `to_json` result in.
    if pending.get(*counter) == Some(&ptr) {
        let rendered = results
            .get(*counter)
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        *counter += 1;
        return Ok(rendered);
    }

    // Snapshot the node (owned), dropping the heap borrow before recursing.
    enum Snap {
        Leaf(serde_json::Value),
        Seq(Vec<Value>),
        Entries(Vec<(String, Value)>),
        Instance {
            class_ptr: HeapPtr,
            fields: Vec<Value>,
        },
        Variant {
            enm: HeapPtr,
            index: usize,
        },
        Unserializable(&'static str),
    }
    let snap = match vm.get_object(ptr) {
        Object::Float(f) => Snap::Leaf(
            serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
        ),
        Object::String(s) => Snap::Leaf(serde_json::Value::String(s.to_string())),
        Object::Bigint(b) => Snap::Leaf(serde_json::Value::String(b.to_string())),
        Object::Array(values) => Snap::Seq(values.to_vec()),
        Object::Map(map) => Snap::Entries(
            map.to_index_map()
                .into_iter()
                .map(|(k, v)| (k.as_str().to_string(), v))
                .collect(),
        ),
        Object::Instance(inst) => Snap::Instance {
            class_ptr: inst.class,
            fields: inst.field_values().collect(),
        },
        Object::Variant(var) => Snap::Variant {
            enm: var.enm,
            index: var.index,
        },
        Object::Uint8Array(_) => Snap::Unserializable(
            "uint8array requires explicit encoding (use to_base64() or to_hex())",
        ),
        _ => Snap::Unserializable("value has no json representation"),
    };

    match snap {
        Snap::Leaf(v) => Ok(v),
        Snap::Seq(values) => {
            let mut out = Vec::with_capacity(values.len());
            for (i, v) in values.into_iter().enumerate() {
                let elem = with_path_segment(path, format_args!("[{i}]"), |p| {
                    render_to_serde(vm, v, pending, results, counter, p)
                })?;
                out.push(elem);
            }
            Ok(serde_json::Value::Array(out))
        }
        Snap::Entries(entries) => {
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                let val = with_path_segment(path, format_args!("[{k:?}]"), |p| {
                    render_to_serde(vm, v, pending, results, counter, p)
                })?;
                out.insert(k, val);
            }
            Ok(serde_json::Value::Object(out))
        }
        Snap::Instance { class_ptr, fields } => {
            let (class_fqn, field_names) = match vm.get_object(class_ptr) {
                Object::Class(c) => (
                    c.name.render_dotted(false),
                    c.fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>(),
                ),
                _ => {
                    return Err(raise_serialize(
                        vm,
                        "instance class pointer is not a class",
                        path,
                        "class",
                    ));
                }
            };
            // Media instances render to their tagged form, not a field map.
            if let Some(kind) = media_kind_from_fqn(&class_fqn) {
                return serialize_media(vm, value, kind, path);
            }
            let mut out = serde_json::Map::with_capacity(fields.len());
            for (i, fv) in fields.into_iter().enumerate() {
                let name = field_names.get(i).cloned().unwrap_or_else(|| i.to_string());
                let fj = with_path_segment(path, format_args!(".{name}"), |p| {
                    render_to_serde(vm, fv, pending, results, counter, p)
                })?;
                out.insert(name, fj);
            }
            Ok(serde_json::Value::Object(out))
        }
        Snap::Variant { enm, index } => {
            let name = match vm.get_object(enm) {
                Object::Enum(e) => e
                    .variants
                    .get(index)
                    .map(|v| v.name.clone())
                    .unwrap_or_default(),
                _ => {
                    return Err(raise_serialize(
                        vm,
                        "enum variant points to non-enum object",
                        path,
                        "enum",
                    ));
                }
            };
            Ok(serde_json::Value::String(name))
        }
        Snap::Unserializable(msg) => Err(raise_serialize(vm, msg, path, "unserializable")),
    }
}

/// Drives pass 2/3 of `render_to_json_honoring_overrides`: accumulates each
/// override's `to_json` result (normalized to `serde_json::Value` so no extra
/// heap roots are held), dispatches the next, and on completion renders the
/// structural skeleton with the override results spliced in. The number of
/// results gathered so far IS the index of the next override to dispatch.
struct ToJsonWalkContinuation {
    /// The value being rendered (its structural skeleton is walked in pass 3).
    root: Value,
    /// Override-bearing sub-values, in render order (pass-1 output).
    pending: Vec<HeapPtr>,
    /// Override results so far, as serde values (no heap roots to track).
    results: Vec<serde_json::Value>,
}

impl Continuation for ToJsonWalkContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // `value` is the override's returned `json` value; normalize to serde now
        // so we hold no extra heap root for it across the next dispatch.
        self.results.push(value_to_serde(vm, value));

        // Dispatch the next override, if any (and resolvable); otherwise render.
        if let Some(&next_ptr) = self.pending.get(self.results.len())
            && let Some(callee) = make_to_json_override_callee(vm, Value::object(next_ptr))
        {
            return NativeCallResult::YieldToCall {
                callee,
                args: vec![],
                type_args: vec![],
                continuation: self,
            };
        }
        render_to_json_done(vm, self.root, &self.pending, &self.results)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = self.pending.clone();
        if let Some(ptr) = self.root.as_object_ptr() {
            roots.push(ptr);
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(ptr) = self.root.as_object_ptr()
            && let Some(&new_ptr) = forwarding.get(&ptr)
        {
            self.root = Value::object(new_ptr);
        }
        for ptr in &mut self.pending {
            if let Some(&new_ptr) = forwarding.get(ptr) {
                *ptr = new_ptr;
            }
        }
    }
}

// ─── Constants ────────────────────────────────────────────────────────────────

const JSON_PARSE_ERROR_FQN: &str = "baml.json.JsonParseError";
const JSON_DECODE_ERROR_FQN: &str = "baml.json.JsonDecodeError";
const JSON_SERIALIZATION_ERROR_FQN: &str = "baml.json.JsonSerializationError";

// ─── Trait implementation ─────────────────────────────────────────────────────

impl BamlNamespaceJson for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        json_parse(vm, s.as_str())
    }

    fn stringify(vm: &mut BexVm, j: &Value) -> bex_str::BexStr {
        let json_val = value_to_serde(vm, *j);
        let s = serde_json::to_string(&json_val).unwrap_or_else(|_| "null".to_string());
        bex_str::BexStr::from(s)
    }

    fn stringify_pretty(vm: &mut BexVm, j: &Value) -> bex_str::BexStr {
        let json_val = value_to_serde(vm, *j);
        let s = serde_json::to_string_pretty(&json_val).unwrap_or_else(|_| "null".to_string());
        bex_str::BexStr::from(s)
    }

    fn to_string(vm: &mut BexVm, v: &Value) -> Result<bex_str::BexStr, VmRustFnError> {
        let ty = vm
            .current_call_type_args()
            .first()
            .cloned()
            .ok_or_else(|| {
                VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                    name: "baml.json.to_string: missing type argument".to_string(),
                })
            })?;
        json_to_string_typed(vm, *v, &ty).map(bex_str::BexStr::from)
    }

    fn from_string(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        let ty = vm
            .current_call_type_args()
            .first()
            .cloned()
            .ok_or_else(|| {
                VmRustFnError::InternalError(VmInternalError::MissingNativeFunction {
                    name: "baml.json.from_string: missing type argument".to_string(),
                })
            })?;
        json_from_string_typed(vm, s.as_str(), &ty)
    }

    fn to_json(vm: &mut BexVm, v: &Value) -> NativeCallResult {
        // `baml.json.to_json<T>` is now a thin alias for the override-honoring
        // structural walker that backs `baml.json.from<T>`. Kept as a stable named
        // entry point for `baml.json.serialize` and host callers; the magic
        // per-class `to_json` method it used to dispatch to is gone.
        render_to_json_honoring_overrides(vm, *v)
    }

    fn from_json(vm: &mut BexVm, j: &Value) -> NativeCallResult {
        // `baml.json.from_json<T>` is now a thin alias for `baml.json.to<T>` (the
        // override-honoring decoder). Kept for back-compat / the `deserialize`
        // wrapper; the magic per-class `from_json` it used to dispatch is gone.
        let ty = match vm.current_call_type_args().first().cloned() {
            Some(t) => t,
            None => {
                return NativeCallResult::Error(VmRustFnError::InternalError(
                    VmInternalError::MissingNativeFunction {
                        name: "baml.json.from_json: missing type argument".to_string(),
                    },
                ));
            }
        };
        json_to_dispatch(vm, *j, &ty)
    }

    fn field(vm: &mut BexVm, j: &Value, key: &bex_str::BexStr) -> Value {
        match j.as_object_ptr() {
            Some(ptr) => match vm.get_object(ptr) {
                Object::Map(m) => m.get(key.as_str()).unwrap_or(Value::NULL),
                _ => Value::NULL,
            },
            None => Value::NULL,
        }
    }
}

// ─── Parse ────────────────────────────────────────────────────────────────────

/// Parse a JSON string and return a `json`-typed VM value.
///
/// The `json` type alias is `null | bool | int | float | string | json[] | map<string, json>`,
/// which maps directly onto VM value kinds:
/// - JSON `null`   → `Value::NULL`
/// - JSON `bool`   → tagged Bool
/// - JSON integer  → tagged i63 (out-of-range falls through to float, see
///   [`serde_to_value`])
/// - JSON float    → heap-boxed `Object::Float`
/// - JSON `string` → heap-boxed `Object::String`
/// - JSON array    → heap-boxed `Object::Array`
/// - JSON object   → heap-boxed `Object::Map`
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
    let class_ptr = vm.lookup_type_by_fqn(JSON_PARSE_ERROR_FQN).ok_or_else(|| {
        VmInternalError::MissingNativeFunction {
            name: JSON_PARSE_ERROR_FQN.to_string(),
        }
    })?;
    let message_val = Value::object(vm.alloc_string(message));
    Ok(Value::object(
        vm.alloc_instance(class_ptr, vec![message_val]),
    ))
}

fn throw_json_decode_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
) -> Result<Value, VmInternalError> {
    let class_ptr = vm
        .lookup_type_by_fqn(JSON_DECODE_ERROR_FQN)
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: JSON_DECODE_ERROR_FQN.to_string(),
        })?;
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    Ok(Value::object(
        vm.alloc_instance(class_ptr, vec![message_val, path_val]),
    ))
}

fn throw_json_serialization_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
    reason: &str,
) -> Result<Value, VmInternalError> {
    let class_ptr = vm
        .lookup_type_by_fqn(JSON_SERIALIZATION_ERROR_FQN)
        .ok_or_else(|| VmInternalError::MissingNativeFunction {
            name: JSON_SERIALIZATION_ERROR_FQN.to_string(),
        })?;
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    let reason_val = Value::object(vm.alloc_string(reason.to_string()));
    Ok(Value::object(vm.alloc_instance(
        class_ptr,
        vec![message_val, path_val, reason_val],
    )))
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

// ─── serde_json ↔ VM Value conversion (untyped) ──────────────────────────────

/// Convert a `serde_json::Value` into a VM `Value`.
///
/// JSON numbers: i63-representable integers become integers; anything that
/// overflows the i63 range (or doesn't parse as `i64` to begin with) falls
/// through to a heap-boxed float, with the usual f64 precision loss above
/// 2^53. Matches SAP's disambiguation behaviour.
pub fn serde_to_value(vm: &mut BexVm, v: &serde_json::Value) -> Value {
    match v {
        serde_json::Value::Null => Value::NULL,
        serde_json::Value::Bool(b) => Value::bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64()
                && let Some(v) = Value::try_int(i)
            {
                v
            } else if let Some(f) = n.as_f64() {
                Value::object(vm.alloc_float(f))
            } else {
                // Only reachable with serde_json's `arbitrary_precision`
                // feature (not enabled here). NaN is a sentinel for "we
                // were handed a number we can't represent at all"; if you
                // hit this in practice, refuse arbitrary-precision input
                // upstream rather than relying on this fallback.
                Value::object(vm.alloc_float(f64::NAN))
            }
        }
        serde_json::Value::String(s) => Value::object(vm.alloc_string(s.clone())),
        serde_json::Value::Array(arr) => {
            let items: Vec<Value> = arr.iter().map(|elem| serde_to_value(vm, elem)).collect();
            // Untyped JSON: elements are `json` values.
            Value::object(
                vm.tlab
                    .alloc(Object::Array(Array::new(json_alias_ty(), items))),
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
            Value::object(vm.tlab.alloc(Object::Map(Map::new(
                RealizedTy::string(),
                json_alias_ty(),
                entries,
            ))))
        }
    }
}

/// Convert a VM `Value` into a `serde_json::Value`, ignoring declared types.
///
/// Used for `RealizedTy::TypeAlias(BAML_JSON_JSON)` and for class fields whose runtime
/// field type is intentionally untyped or unavailable.
pub fn value_to_serde(vm: &BexVm, v: Value) -> serde_json::Value {
    use bex_vm_types::ValueKind;
    match v.kind() {
        ValueKind::Null => serde_json::Value::Null,
        ValueKind::Bool(b) => serde_json::Value::Bool(b),
        ValueKind::Int(i) => serde_json::Value::Number(i.into()),
        ValueKind::OmittedArg => serde_json::Value::Null,
        ValueKind::Object(ptr) => match vm.get_object(ptr) {
            Object::Float(f) => serde_json::Number::from_f64(*f)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null),
            Object::String(s) => serde_json::Value::String(s.to_string()),
            Object::Array(arr) => {
                let arr = arr.to_vec();
                serde_json::Value::Array(arr.iter().map(|el| value_to_serde(vm, *el)).collect())
            }
            Object::Map(map) => {
                let map = map.to_index_map();
                let entries: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.to_string(), value_to_serde(vm, *v)))
                    .collect();
                serde_json::Value::Object(entries)
            }
            Object::Bigint(bi) => serde_json::Value::String(bi.to_string()),
            // An enum variant renders as its variant-name string, matching the
            // typed (`ty_value_to_serde`) and structural (`render_to_serde`)
            // walkers. This is reached when a variant flows through an arm that
            // delegates to the untyped converter — e.g. `ty_value_to_serde`'s
            // `RealizedTy::Union` arm for a field typed `E | ...` (including the
            // optional enum `E?` == `E | null`). Without this, such fields
            // serialized as `null` (B-728).
            Object::Variant(var) => {
                let (enm, index) = (var.enm, var.index);
                match vm.get_object(enm) {
                    Object::Enum(e) => e
                        .variants
                        .get(index)
                        .map(|v| serde_json::Value::String(v.name.clone()))
                        .unwrap_or(serde_json::Value::Null),
                    _ => serde_json::Value::Null,
                }
            }
            Object::Instance(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::Interface(_)
            | Object::Package(_)
            | Object::ImplRule(_)
            | Object::Function(_)
            | Object::Future(_)
            | Object::UnscheduledFuture(_)
            | Object::Collector(_)
            | Object::Type(_)
            | Object::Uint8Array(_)
            | Object::RustData(_)
            | Object::Closure(_)
            | Object::BoundMethod(_)
            | Object::GenericFunction(_)
            | Object::HostClosure(_)
            | Object::Cell(_) => serde_json::Value::Null,
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => serde_json::Value::Null,
        },
    }
}

// ─── Typed JSON serialize ────────────────────────────────────────────────────

/// Serialize a VM `Value` to a JSON string driven by the runtime `RealizedTy`.
///
/// Walks the value matching the shape of `ty`.  Throws
/// `JsonSerializationError` for non-representable types (`uint8array`,
/// function values, etc.).
pub fn json_to_string_typed(
    vm: &mut BexVm,
    v: Value,
    ty: &RealizedTy,
) -> Result<String, VmRustFnError> {
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
    ty: &RealizedTy,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    match ty {
        // Primitive shapes: emit the value directly through value_to_serde,
        // which is total for scalar values.
        RealizedTy::Null { .. } => Ok(serde_json::Value::Null),
        RealizedTy::Int { .. }
        | RealizedTy::Float { .. }
        | RealizedTy::Bool { .. }
        | RealizedTy::String { .. } => Ok(value_to_serde(vm, value)),
        RealizedTy::Bigint { .. } => Ok(value_to_serde(vm, value)),
        RealizedTy::Literal(_, _, _) => Ok(value_to_serde(vm, value)),

        RealizedTy::List(elem, _) => {
            let items = match value.as_object_ptr() {
                Some(ptr) => match vm.get_object(ptr) {
                    Object::Array(arr) => arr.to_vec(),
                    _ => return Err(raise_serialize(vm, "expected array", path, "list")),
                },
                None => return Err(raise_serialize(vm, "expected array", path, "list")),
            };
            let mut out = Vec::with_capacity(items.len());
            for (i, item) in items.into_iter().enumerate() {
                let elem_json = with_path_segment(path, format_args!("[{i}]"), |p| {
                    ty_value_to_serde(vm, item, elem, p)
                })?;
                out.push(elem_json);
            }
            Ok(serde_json::Value::Array(out))
        }

        RealizedTy::Map { value: vty, .. } => {
            let entries = match value.as_object_ptr() {
                Some(ptr) => match vm.get_object(ptr) {
                    Object::Map(m) => m.to_index_map(),
                    _ => return Err(raise_serialize(vm, "expected map", path, "map")),
                },
                None => return Err(raise_serialize(vm, "expected map", path, "map")),
            };
            let mut out = serde_json::Map::with_capacity(entries.len());
            for (k, v) in entries {
                let val_json = with_path_segment(path, format_args!("[{k:?}]"), |p| {
                    ty_value_to_serde(vm, v, vty, p)
                })?;
                out.insert(k.to_string(), val_json);
            }
            Ok(serde_json::Value::Object(out))
        }

        RealizedTy::TypeAlias(name, _) if name.display_name().as_str() == BAML_JSON_JSON => {
            Ok(value_to_serde(vm, value))
        }

        RealizedTy::TypeAlias(_, _) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            Ok(value_to_serde(vm, value))
        }

        RealizedTy::Class(qtn, _type_args, _) | RealizedTy::Interface(qtn, _type_args, _, _) => {
            serialize_class_instance(vm, value, qtn, path)
        }

        RealizedTy::Enum(_, _) => match value.as_object_ptr() {
            Some(ptr) => match vm.get_object(ptr) {
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
            None => Err(raise_serialize(vm, "expected enum variant", path, "enum")),
        },

        RealizedTy::EnumVariant(_, name, _) => Ok(serde_json::Value::String(name.to_string())),

        RealizedTy::Media(kind, _) => serialize_media(vm, value, *kind, path),

        RealizedTy::Uint8Array { .. } => Err(raise_serialize(
            vm,
            "uint8array requires explicit encoding (use to_base64() or to_hex())",
            path,
            "uint8array",
        )),

        RealizedTy::Union(members, _) => {
            // Select the first declared member that contains the runtime value,
            // using the same ordered, decidable membership relation as `is` and
            // typed match arms. Serialization then remains fully type-directed:
            // a class/media/uint8array member behaves exactly as it would outside
            // the union, and values outside every member fail the type contract.
            let member = members.iter().find(|member| {
                crate::type_match::value_matches_template(
                    vm,
                    value,
                    &TyTemplate::from((*member).clone()),
                    &[],
                )
                // A template built from a `RealizedTy` carries no frame refs
                // and no projections, so substitution cannot fail.
                .unwrap_or_else(|e| {
                    unreachable!("realized union-member template failed to substitute: {e}")
                })
            });
            match member {
                Some(member) => ty_value_to_serde(vm, value, member, path),
                None => Err(raise_serialize(
                    vm,
                    "value is not a member of the union",
                    path,
                    "union",
                )),
            }
        }

        RealizedTy::Resource { .. } | RealizedTy::PromptAst { .. } => Err(raise_serialize(
            vm,
            "cannot serialize opaque type",
            path,
            "opaque",
        )),

        // Compiler-only / non-representable variants.
        RealizedTy::Function { .. } => Err(raise_serialize(
            vm,
            "cannot serialize function values",
            path,
            "function",
        )),
        RealizedTy::Future(_, _, _) => Err(raise_serialize(
            vm,
            "cannot serialize future values",
            path,
            "future",
        )),
        RealizedTy::BuiltinUnknown { .. } => Err(raise_serialize(
            vm,
            "cannot serialize unknown type",
            path,
            "unknown",
        )),
        RealizedTy::Void { .. } => {
            // `void` has no declared JSON shape to validate against here.
            // Use structural serialization of the produced value.
            // Instantiated generic class fields normally use `field_template`
            // substitution before reaching this point.
            Ok(value_to_serde(vm, value))
        }

        // Type-level and opaque types carry no serializable runtime value:
        // reflection types (`Type`), opaque Rust state (`RustType`), and the
        // bottom type (`Never`).
        RealizedTy::Never { .. } | RealizedTy::RustType { .. } | RealizedTy::Type { .. } => {
            Err(raise_serialize(
                vm,
                "cannot serialize compiler-only type",
                path,
                "compiler_only",
            ))
        }
    }
}

/// Serialize a class instance: look up the runtime `Class`, iterate fields
/// by name, recurse on each field value with the declared `field_type`.
///
/// Special-cases media classes (`baml.media.Pdf`/`Audio`/`Video`/`Image`)
/// which are stored as `Object::Instance` with a `_data: Object::RustData`
/// field.  Detected by class FQN; a leading `RealizedTy::Media(_)` would have
/// already routed through `serialize_media`.
fn serialize_class_instance(
    vm: &mut BexVm,
    value: Value,
    qtn: &TypeName,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    let inst_ptr = match value.as_object_ptr() {
        Some(ptr) => ptr,
        None => {
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
            inst.field_values().collect::<Vec<_>>(),
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

    if let Some(kind) = media_kind_from_fqn(qtn.display_name().as_str()) {
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
        // Substitute class-level type-args into the field's template so
        // generic positions (`item: T` in `Container<T>`) resolve to the
        // concrete type carried on `Instance::class_type_args`.
        let field_ty = vm.realize_field_ty(&cf.field_template, &class_type_args);
        let field_json = with_path_segment(path, format_args!(".{}", cf.name), |p| {
            ty_value_to_serde(vm, field_value, &field_ty, p)
        })?;
        out.insert(cf.name.clone(), field_json);
    }
    Ok(serde_json::Value::Object(out))
}

/// The [`MediaKind`] of a media class FQN, or `None` for a non-media class. A
/// runtime media value is an `Object::Instance` of one of these std classes
/// (carrying a `$rust_type` `_data` field); there is no `Generic` media *value*.
pub(crate) fn media_kind_from_fqn(fqn: &str) -> Option<MediaKind> {
    match fqn {
        "baml.media.Image" => Some(MediaKind::Image),
        "baml.media.Audio" => Some(MediaKind::Audio),
        "baml.media.Video" => Some(MediaKind::Video),
        "baml.media.Pdf" => Some(MediaKind::Pdf),
        _ => None,
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
        serde_json::Value::String(kind.tag_str().into()),
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

pub(crate) fn read_media_value(
    vm: &BexVm,
    value: Value,
) -> Option<Arc<baml_builtins2::MediaValue>> {
    let ptr = value.as_object_ptr()?;
    let (class, data_value) = match vm.get_object(ptr) {
        Object::Instance(inst) => (inst.class, inst.fields.first()?.load()),
        _ => return None,
    };
    let class_name = match vm.get_object(class) {
        Object::Class(class) => class.name.render_dotted(false),
        _ => return None,
    };
    media_kind_from_fqn(class_name.as_str())?;

    let data_ptr = data_value.as_object_ptr()?;
    match vm.get_object(data_ptr) {
        Object::RustData(arc) => arc.clone().downcast::<baml_builtins2::MediaValue>().ok(),
        _ => None,
    }
}

// ─── Typed JSON deserialize ──────────────────────────────────────────────────

/// Parse a JSON string and coerce it to a VM `Value` of the given runtime
/// `RealizedTy`.
///
/// Throws `JsonParseError` for invalid JSON and `JsonDecodeError` when the
/// parsed JSON does not match the target type.
pub fn json_from_string_typed(
    vm: &mut BexVm,
    s: &str,
    ty: &RealizedTy,
) -> Result<Value, VmRustFnError> {
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
    ty: &RealizedTy,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    match ty {
        RealizedTy::Null { .. } => match json {
            serde_json::Value::Null => Ok(Value::NULL),
            _ => Err(raise_decode(vm, "expected null", path)),
        },

        RealizedTy::Bool { .. } => match json {
            serde_json::Value::Bool(b) => Ok(Value::bool(*b)),
            _ => Err(raise_decode(vm, "expected boolean", path)),
        },

        RealizedTy::Int { .. } => match json {
            serde_json::Value::Number(n) => n.as_i64().and_then(Value::try_int).ok_or_else(|| {
                raise_decode(
                    vm,
                    "expected integer in the BAML int range [-2^62, 2^62 - 1]",
                    path,
                )
            }),
            _ => Err(raise_decode(vm, "expected integer", path)),
        },

        // Bigint JSON decoding is not yet implemented (Phase 9+).
        RealizedTy::Bigint { .. } => Err(raise_decode(
            vm,
            "bigint JSON decoding not yet implemented",
            path,
        )),

        RealizedTy::Float { .. } => match json {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(Value::object(vm.alloc_float(f)))
                } else {
                    Err(raise_decode(vm, "expected number", path))
                }
            }
            _ => Err(raise_decode(vm, "expected number", path)),
        },

        RealizedTy::String { .. } => match json {
            serde_json::Value::String(s) => Ok(Value::object(vm.alloc_string(s.clone()))),
            _ => Err(raise_decode(vm, "expected string", path)),
        },

        RealizedTy::List(elem, _) => match json {
            serde_json::Value::Array(arr) => {
                let mut items = Vec::with_capacity(arr.len());
                for (i, item) in arr.iter().enumerate() {
                    let v = with_path_segment(path, format_args!("[{i}]"), |p| {
                        ty_serde_to_value(vm, item, elem, p)
                    })?;
                    items.push(v);
                }
                Ok(Value::object(
                    vm.tlab
                        .alloc(Object::Array(Array::new((**elem).clone(), items))),
                ))
            }
            _ => Err(raise_decode(vm, "expected array", path)),
        },

        RealizedTy::Map { value: vty, .. } => match json {
            serde_json::Value::Object(map) => {
                let mut entries: IndexMap<bex_vm_types::BexStr, Value> =
                    IndexMap::with_capacity(map.len());
                for (k, val) in map {
                    let v = with_path_segment(path, format_args!("[{k:?}]"), |p| {
                        ty_serde_to_value(vm, val, vty, p)
                    })?;
                    entries.insert(bex_vm_types::BexStr::from(k.as_str()), v);
                }
                // BAML maps are always string-keyed at runtime.
                Ok(Value::object(vm.tlab.alloc(Object::Map(Map::new(
                    RealizedTy::string(),
                    (**vty).clone(),
                    entries,
                )))))
            }
            _ => Err(raise_decode(vm, "expected object", path)),
        },

        RealizedTy::TypeAlias(name, _) if name.display_name().as_str() == BAML_JSON_JSON => {
            Ok(serde_to_value(vm, json))
        }

        RealizedTy::TypeAlias(_, _) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            Ok(serde_to_value(vm, json))
        }

        RealizedTy::Class(qtn, type_args, _) => {
            if let Some(kind) = media_kind_from_fqn(qtn.display_name().as_str()) {
                return deserialize_media(vm, json, kind, qtn, path);
            }
            deserialize_class_instance(vm, json, qtn, type_args, path)
        }

        RealizedTy::Interface(qtn, type_args, _, _) => {
            deserialize_class_instance(vm, json, qtn, type_args, path)
        }

        RealizedTy::Enum(qtn, _) => match json {
            serde_json::Value::String(s) => deserialize_enum_variant(vm, qtn, s, path),
            _ => Err(raise_decode(vm, "expected enum variant string", path)),
        },

        RealizedTy::EnumVariant(qtn, name, _) => match json {
            serde_json::Value::String(s) if s == name.as_str() => {
                deserialize_enum_variant(vm, qtn, s, path)
            }
            _ => Err(raise_decode(
                vm,
                format!("expected enum variant `{name}`"),
                path,
            )),
        },

        RealizedTy::Media(kind, _) => deserialize_media_by_kind(vm, json, *kind, path),

        RealizedTy::Uint8Array { .. } => Err(raise_decode(
            vm,
            "uint8array requires explicit encoding (use from_base64() or from_hex())",
            path,
        )),

        RealizedTy::Union(members, _) => {
            // Try each member structurally; first match wins.
            for member in members {
                let mut tmp_path = path.clone();
                if let Ok(v) = ty_serde_to_value(vm, json, member, &mut tmp_path) {
                    return Ok(v);
                }
            }
            Err(raise_decode(vm, "no union member matched", path))
        }

        RealizedTy::Literal(lit, _, _) => match (lit, json) {
            (baml_type::Literal::Bool(b), serde_json::Value::Bool(jb)) if b == jb => {
                Ok(Value::bool(*jb))
            }
            (baml_type::Literal::String(s), serde_json::Value::String(js)) if s == js => {
                Ok(Value::object(vm.alloc_string(js.clone())))
            }
            (baml_type::Literal::Int(expected), serde_json::Value::Number(n)) => {
                if let Some(actual) = n.as_i64() {
                    if *expected == actual {
                        return Ok(Value::int(actual));
                    }
                }
                Err(raise_decode(vm, "literal int mismatch", path))
            }
            (baml_type::Literal::Float(s), serde_json::Value::Number(n)) => {
                if let (Ok(expected), Some(actual)) = (s.parse::<f64>(), n.as_f64()) {
                    if (expected - actual).abs() < f64::EPSILON {
                        return Ok(Value::object(vm.alloc_float(actual)));
                    }
                }
                Err(raise_decode(vm, "literal float mismatch", path))
            }
            // Literal bigint decoding is not yet implemented (Phase 9+).
            (baml_type::Literal::Bigint(_), _) => Err(raise_decode(
                vm,
                "literal bigint JSON decoding not yet implemented",
                path,
            )),
            _ => Err(raise_decode(vm, "literal mismatch", path)),
        },

        RealizedTy::Resource { .. } | RealizedTy::PromptAst { .. } => {
            Err(raise_decode(vm, "cannot deserialize opaque type", path))
        }

        RealizedTy::Function { .. }
        | RealizedTy::Future(_, _, _)
        | RealizedTy::BuiltinUnknown { .. }
        | RealizedTy::Void { .. } => {
            // These variants do not provide a concrete JSON schema to validate
            // against here. Preserve structural JSON conversion for values
            // whose shape is already JSON-representable.
            Ok(serde_to_value(vm, json))
        }

        // Type-level and opaque types are not valid decode targets: reflection
        // types (`Type`), opaque Rust state (`RustType`), and the bottom type
        // (`Never`).
        RealizedTy::Never { .. } | RealizedTy::RustType { .. } | RealizedTy::Type { .. } => {
            Err(raise_decode(vm, "cannot decode compiler-only type", path))
        }
    }
}

fn deserialize_class_instance(
    vm: &mut BexVm,
    json: &serde_json::Value,
    qtn: &TypeName,
    type_args: &[RealizedTy],
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

    let class_ptr = vm
        .lookup_type(qtn)
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
        // Substitute class-level type-args into the field's template so a
        // `Container<User>::item` field decodes against `User` rather than
        // erased runtime metadata.
        let field_ty = vm.realize_field_ty(&cf.field_template, type_args);
        let v = with_path_segment(path, format_args!(".{}", cf.name), |p| {
            let field_json_owned;
            let field_json: &serde_json::Value = if let Some(v) = map.get(cf.name.as_str()) {
                v
            } else if field_ty.is_nullable_union() {
                // Optional (`T?` == `T | null`) fields may be absent → null.
                field_json_owned = serde_json::Value::Null;
                &field_json_owned
            } else {
                return Err(raise_decode(
                    vm,
                    format!("missing required field `{}`", cf.name),
                    p,
                ));
            };
            ty_serde_to_value(vm, field_json, &field_ty, p)
        })?;
        field_values.push(v);
    }

    Ok(Value::object(vm.tlab.alloc(Object::Instance(
        Instance::new(class_ptr, type_args.into(), field_values),
    ))))
}

fn deserialize_enum_variant(
    vm: &mut BexVm,
    qtn: &TypeName,
    variant_name: &str,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let enm_ptr = vm
        .lookup_type(qtn)
        .ok_or_else(|| raise_decode(vm, format!("enum `{qtn}` not found"), path))?;
    let idx = match vm.get_object(enm_ptr) {
        Object::Enum(e) => e.variants.iter().position(|v| v.name == variant_name),
        _ => {
            return Err(raise_decode(vm, format!("`{qtn}` is not an enum"), path));
        }
    };
    match idx {
        Some(i) => Ok(Value::object(vm.alloc_variant(enm_ptr, i))),
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

    // The envelope's `kind` tag is the discriminant, so it must agree with the
    // target media kind. Without this check nothing in a media decode can
    // reject a mismatched envelope, and the first-match-wins union arm in
    // `ty_serde_to_value` collapses every media kind onto the union's first
    // member (an audio envelope decoding as an `Image` through
    // `image | audio`). `Generic` is the "any media" tag and discriminates
    // nothing. An absent (or null) `kind` is accepted for any target —
    // hand-built `{source, value, mime}` objects stay decodable — which also
    // means such an envelope still selects a media union's first member.
    match map.get("kind") {
        None | Some(serde_json::Value::Null) => {}
        Some(serde_json::Value::String(tag))
            if kind == MediaKind::Generic
                || tag == kind.tag_str()
                || tag == MediaKind::Generic.tag_str() => {}
        Some(serde_json::Value::String(tag)) => {
            let expected = kind.tag_str();
            return Err(raise_decode(
                vm,
                format!("media kind mismatch: envelope is `{tag}`, expected `{expected}`"),
                path,
            ));
        }
        Some(_) => {
            return Err(raise_decode(
                vm,
                "media object `kind` must be a string",
                path,
            ));
        }
    }

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

    let class_ptr = vm
        .lookup_type(qtn)
        .ok_or_else(|| raise_decode(vm, format!("media class `{qtn}` not found"), path))?;
    let data_val = Value::object(vm.alloc_rust_data(media_arc));
    Ok(Value::object(vm.alloc_instance(class_ptr, vec![data_val])))
}

// ─── from_json dispatcher ────────────────────────────────────────────────────

/// Structural decode by converting `j` (a VM `json` value) to
/// `serde_json::Value` and running `ty_serde_to_value`. Used by
/// [`json_to_dispatch`] for leaf types (primitives, enums, media, literals)
/// that can never carry a `baml.FromJson` override.
fn structural_decode_value(vm: &mut BexVm, j: Value, ty: &RealizedTy) -> NativeCallResult {
    let serde = value_to_serde(vm, j);
    let mut path = String::new();
    match ty_serde_to_value(vm, &serde, ty, &mut path) {
        Ok(v) => NativeCallResult::Done(v),
        Err(e) => NativeCallResult::Error(e),
    }
}

/// Pass-through continuation used when dispatching `<fqn>.from_json(j)`: the
/// callee returns the constructed instance directly, so we just hand it back.
struct IdentityFromJsonCont;

impl Continuation for IdentityFromJsonCont {
    fn call(self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        NativeCallResult::Done(value)
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        Vec::new()
    }
    fn apply_forwarding(&mut self, _: &HashMap<HeapPtr, HeapPtr>) {}
}

// ── baml.json.to<T> / baml.FromJson dispatch ───────────────────────────────────
//
// The deserialize counterpart of `baml.json.from` / `baml.ToJson`. `json.to<T>`
// resolves a user `implements baml.FromJson { function from_json ... }` override
// on the target type `T` and dispatches it; otherwise it decodes structurally.
//
// F1 (additive): the structural fallback delegates to `json_from_json_dispatch`
// (the existing magic path — auto-derived per-field bodies still exist and honor
// nested overrides). F2 will retire the magic path and move the per-field
// override-honoring decode into the default itself.

/// Reads the target type `T` from the call's type-args and dispatches
/// `baml.json.to<T>(j)` — the `baml._from_json_shim` native.
pub(super) fn json_to_shim(vm: &mut BexVm, j: Value) -> NativeCallResult {
    let ty = match vm.current_call_type_args().first().cloned() {
        Some(t) => t,
        None => {
            return NativeCallResult::Error(VmRustFnError::InternalError(
                VmInternalError::MissingNativeFunction {
                    name: "baml.json.to: missing type argument".to_string(),
                },
            ));
        }
    };
    json_to_dispatch(vm, j, &ty)
}

/// Dispatch `json.to<T>(j)` — the override-honoring structural decode.
///
/// - nullable union: `null` short-circuits, else decode the non-null payload;
/// - list / map: decode each element/value through the driver (honoring nested
///   overrides);
/// - class / interface (non-media): a `baml.FromJson` override on the runtime
///   target wins; otherwise decode per-field via [`class_from_json_start`];
/// - everything else (primitives, enums, media, literals, type-aliases): a
///   structural decode (no overrides possible).
fn json_to_dispatch(vm: &mut BexVm, j: Value, ty: &RealizedTy) -> NativeCallResult {
    match ty {
        RealizedTy::Union(members, _) if members.iter().any(RealizedTy::is_null) => {
            if j.is_null() {
                NativeCallResult::Done(Value::NULL)
            } else {
                json_to_dispatch(vm, j, &ty.strip_null())
            }
        }
        RealizedTy::List(elem, _) => list_from_json_start(vm, j, elem),
        RealizedTy::Map { value: vty, .. } => map_from_json_start(vm, j, vty),
        RealizedTy::Class(qtn, type_args, _) | RealizedTy::Interface(qtn, type_args, _, _)
            if media_kind_from_fqn(qtn.display_name().as_str()).is_none() =>
        {
            match try_yield_interface_from_json(vm, j, ty) {
                Some(yld) => yld,
                None => class_from_json_start(vm, j, qtn, type_args),
            }
        }
        _ => structural_decode_value(vm, j, ty),
    }
}

/// Decode `j` into an instance of class `qtn` (instantiated with `type_args`) by
/// decoding each field through the `baml.json.to<FieldType>` driver and then
/// constructing the instance. The Rust counterpart of the per-field auto-derived
/// `from_json` body: a trampoline that yields once per field (so a field whose
/// type implements `baml.FromJson` is decoded via its override), which composes
/// with the surrounding list/map/class trampolines because each field is a
/// single driver dispatch.
fn class_from_json_start(
    vm: &mut BexVm,
    j: Value,
    qtn: &TypeName,
    type_args: &[RealizedTy],
) -> NativeCallResult {
    let map: IndexMap<bex_vm_types::BexStr, Value> = match j.as_object_ptr() {
        Some(p) => match vm.get_object(p) {
            Object::Map(m) => m.lock().iter().map(|(k, v)| (k.clone(), *v)).collect(),
            _ => {
                return NativeCallResult::Error(raise_decode(
                    vm,
                    format!("expected JSON object for class `{qtn}`"),
                    "",
                ));
            }
        },
        None => {
            return NativeCallResult::Error(raise_decode(
                vm,
                format!("expected JSON object for class `{qtn}`"),
                "",
            ));
        }
    };
    let class_ptr = match vm.lookup_type(qtn) {
        Some(p) => p,
        None => {
            return NativeCallResult::Error(raise_decode(
                vm,
                format!("class `{qtn}` not found"),
                "",
            ));
        }
    };
    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return NativeCallResult::Error(raise_decode(
                vm,
                format!("`{qtn}` is not a class"),
                "",
            ));
        }
    };
    // Resolve each field's json value + its (type-arg-substituted) field type.
    let mut fields: Vec<(Value, RealizedTy)> = Vec::with_capacity(class_fields.len());
    for cf in &class_fields {
        let field_ty = vm.realize_field_ty(&cf.field_template, type_args);
        let field_json = match map.get(cf.name.as_str()) {
            Some(v) => *v,
            // Optional (`T?` == `T | null`) fields may be absent → null.
            None if field_ty.is_nullable_union() => Value::NULL,
            None => {
                return NativeCallResult::Error(raise_decode(
                    vm,
                    format!("missing required field `{}`", cf.name),
                    "",
                ));
            }
        };
        fields.push((field_json, field_ty));
    }
    class_drive(vm, class_ptr, type_args.to_vec(), fields, Vec::new(), 0)
}

/// Drive the per-field decode from field `idx`, yielding to `baml.json.to` for
/// the next field that needs decoding and constructing the instance once every
/// field is decoded. `null` optional fields short-circuit without yielding.
fn class_drive(
    vm: &mut BexVm,
    class_ptr: HeapPtr,
    class_type_args: Vec<RealizedTy>,
    fields: Vec<(Value, RealizedTy)>,
    mut results: Vec<Value>,
    mut idx: usize,
) -> NativeCallResult {
    let to_fn = match vm.find_function_by_name("baml.json.to") {
        Some(f) => f,
        None => {
            return NativeCallResult::Error(VmRustFnError::InternalError(
                VmInternalError::MissingNativeFunction {
                    name: "baml.json.to not found".to_string(),
                },
            ));
        }
    };
    while idx < fields.len() {
        let (field_json, field_ty) = fields[idx].clone();
        if let Some(v) = optional_null_short_circuit(field_json, &field_ty) {
            results.push(v);
            idx += 1;
            continue;
        }
        return NativeCallResult::YieldToCall {
            callee: to_fn,
            args: vec![field_json],
            type_args: vec![field_ty],
            continuation: Box::new(ClassFromJsonCont {
                class_ptr,
                class_type_args,
                fields,
                results,
                idx,
            }),
        };
    }
    NativeCallResult::Done(Value::object(vm.tlab.alloc(Object::Instance(
        Instance::new(class_ptr, class_type_args.into(), results),
    ))))
}

/// Resumes [`class_drive`] after one field's `baml.json.to` decode returns.
struct ClassFromJsonCont {
    class_ptr: HeapPtr,
    class_type_args: Vec<RealizedTy>,
    fields: Vec<(Value, RealizedTy)>,
    results: Vec<Value>,
    idx: usize,
}

impl Continuation for ClassFromJsonCont {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        let next = self.idx + 1;
        class_drive(
            vm,
            self.class_ptr,
            self.class_type_args,
            self.fields,
            self.results,
            next,
        )
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = vec![self.class_ptr];
        for (fj, _) in &self.fields {
            if let Some(p) = fj.as_object_ptr() {
                roots.push(p);
            }
        }
        for v in &self.results {
            if let Some(p) = v.as_object_ptr() {
                roots.push(p);
            }
        }
        roots
    }
    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(&new) = forwarding.get(&self.class_ptr) {
            self.class_ptr = new;
        }
        for (fj, _) in &mut self.fields {
            if let Some(p) = fj.as_object_ptr() {
                if let Some(&new) = forwarding.get(&p) {
                    *fj = Value::object(new);
                }
            }
        }
        for v in &mut self.results {
            if let Some(p) = v.as_object_ptr() {
                if let Some(&new) = forwarding.get(&p) {
                    *v = Value::object(new);
                }
            }
        }
    }
}

/// If `ty` is a class/interface type whose runtime type carries an in-body
/// `implements baml.FromJson { function from_json ... }` override, returns a
/// `YieldToCall` dispatching `{fqn}.baml.FromJson.from_json(j)`. The deserialize
/// analog of `try_yield_user_from_json`, but keyed on the interface method name
/// rather than the magic `{fqn}.from_json`. Returns `None` for non-class types,
/// media, and types without the override (→ structural fallback).
fn try_yield_interface_from_json(
    vm: &mut BexVm,
    j: Value,
    ty: &RealizedTy,
) -> Option<NativeCallResult> {
    let (qtn, type_args) = match ty {
        RealizedTy::Class(qtn, type_args, _) | RealizedTy::Interface(qtn, type_args, _, _) => {
            (qtn, type_args)
        }
        _ => return None,
    };
    if media_kind_from_fqn(qtn.display_name().as_str()).is_some() {
        return None;
    }
    let from_json_name = format!("{}.baml.FromJson.from_json", class_lookup_key(qtn));
    let callee = vm.find_function_by_name(&from_json_name)?;
    Some(NativeCallResult::YieldToCall {
        callee,
        args: vec![j],
        type_args: type_args.clone(),
        continuation: Box::new(IdentityFromJsonCont),
    })
}

// ── List dispatch ─────────────────────────────────────────────────────────────

fn list_from_json_start(vm: &mut BexVm, j: Value, elem_ty: &RealizedTy) -> NativeCallResult {
    let array = match j.as_object_ptr() {
        Some(p) => match vm.get_object(p) {
            Object::Array(a) => a.to_vec(),
            _ => {
                return NativeCallResult::Error(raise_decode(vm, "expected JSON array", ""));
            }
        },
        None => {
            return NativeCallResult::Error(raise_decode(vm, "expected JSON array", ""));
        }
    };
    list_drive(vm, array, elem_ty.clone(), Vec::new(), 0)
}

/// Drive the list walk from index `idx`. Synchronously decodes elements that
/// don't require yielding and falls into a `ListFromJsonCont` for the first
/// element that does.
fn list_drive(
    vm: &mut BexVm,
    array: Vec<Value>,
    elem_ty: RealizedTy,
    mut results: Vec<Value>,
    mut idx: usize,
) -> NativeCallResult {
    while idx < array.len() {
        let curr = array[idx];
        // Optional element: null short-circuits without yielding.
        if let Some(v) = optional_null_short_circuit(curr, &elem_ty) {
            results.push(v);
            idx += 1;
            continue;
        }
        // Elements that may contain overrides decode via the `baml.json.to`
        // driver (one dispatch each, composing with this trampoline); leaf
        // elements decode structurally without yielding.
        if needs_driver_decode(&elem_ty) {
            let to_fn = match vm.find_function_by_name("baml.json.to") {
                Some(f) => f,
                None => return missing_to_driver(),
            };
            return NativeCallResult::YieldToCall {
                callee: to_fn,
                args: vec![curr],
                type_args: vec![elem_ty.clone()],
                continuation: Box::new(ListFromJsonCont {
                    array,
                    elem_ty,
                    results,
                    idx,
                }),
            };
        }
        match decode_value_sync(vm, curr, &elem_ty) {
            Ok(v) => {
                results.push(v);
                idx += 1;
            }
            Err(e) => return NativeCallResult::Error(e),
        }
    }
    // Type-directed: the result list carries the decode element type.
    let arr_val = Value::object(vm.tlab.alloc(Object::Array(Array::new(elem_ty, results))));
    NativeCallResult::Done(arr_val)
}

/// Whether decoding `ty` may need to dispatch a `baml.FromJson` override, so the
/// caller must yield to the `baml.json.to` driver rather than decode in place.
/// True for class/interface/list/map (after peeling an optional wrapper); false
/// for leaf types (primitives, enums, media, literals) — which decode
/// structurally and can never carry an override.
fn needs_driver_decode(ty: &RealizedTy) -> bool {
    matches!(
        peel_optional(ty),
        RealizedTy::Class(..)
            | RealizedTy::Interface(..)
            | RealizedTy::List(..)
            | RealizedTy::Map { .. }
    )
}

/// The `baml.json.to` driver function should always be registered; this is the
/// defensive error if it is somehow missing.
fn missing_to_driver() -> NativeCallResult {
    NativeCallResult::Error(VmRustFnError::InternalError(
        VmInternalError::MissingNativeFunction {
            name: "baml.json.to not found".to_string(),
        },
    ))
}

struct ListFromJsonCont {
    array: Vec<Value>,
    elem_ty: RealizedTy,
    results: Vec<Value>,
    idx: usize,
}

impl Continuation for ListFromJsonCont {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        let next_idx = self.idx + 1;
        list_drive(vm, self.array, self.elem_ty, self.results, next_idx)
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for v in self.array.iter().chain(self.results.iter()) {
            if let Some(p) = v.as_object_ptr() {
                roots.push(p);
            }
        }
        roots
    }
    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for v in self.array.iter_mut().chain(self.results.iter_mut()) {
            if let Some(p) = v.as_object_ptr() {
                if let Some(&new) = forwarding.get(&p) {
                    *v = Value::object(new);
                }
            }
        }
    }
}

// ── Map dispatch ──────────────────────────────────────────────────────────────

fn map_from_json_start(vm: &mut BexVm, j: Value, val_ty: &RealizedTy) -> NativeCallResult {
    let entries: Vec<(bex_vm_types::BexStr, Value)> = match j.as_object_ptr() {
        Some(p) => match vm.get_object(p) {
            Object::Map(m) => m.lock().iter().map(|(k, v)| (k.clone(), *v)).collect(),
            _ => {
                return NativeCallResult::Error(raise_decode(vm, "expected JSON object", ""));
            }
        },
        None => {
            return NativeCallResult::Error(raise_decode(vm, "expected JSON object", ""));
        }
    };
    map_drive(vm, entries, val_ty.clone(), IndexMap::new(), 0)
}

fn map_drive(
    vm: &mut BexVm,
    entries: Vec<(bex_vm_types::BexStr, Value)>,
    val_ty: RealizedTy,
    mut results: IndexMap<bex_vm_types::BexStr, Value>,
    mut idx: usize,
) -> NativeCallResult {
    while idx < entries.len() {
        let curr = entries[idx].1;
        let key = entries[idx].0.clone();
        if let Some(v) = optional_null_short_circuit(curr, &val_ty) {
            results.insert(key, v);
            idx += 1;
            continue;
        }
        if needs_driver_decode(&val_ty) {
            let to_fn = match vm.find_function_by_name("baml.json.to") {
                Some(f) => f,
                None => return missing_to_driver(),
            };
            return NativeCallResult::YieldToCall {
                callee: to_fn,
                args: vec![curr],
                type_args: vec![val_ty.clone()],
                continuation: Box::new(MapFromJsonCont {
                    entries,
                    val_ty,
                    results,
                    idx,
                }),
            };
        }
        match decode_value_sync(vm, curr, &val_ty) {
            Ok(v) => {
                results.insert(key, v);
                idx += 1;
            }
            Err(e) => return NativeCallResult::Error(e),
        }
    }
    // Type-directed: string keys, decode value type carried from `val_ty`.
    let map_val = Value::object(vm.tlab.alloc(Object::Map(Map::new(
        RealizedTy::string(),
        val_ty,
        results,
    ))));
    NativeCallResult::Done(map_val)
}

struct MapFromJsonCont {
    entries: Vec<(bex_vm_types::BexStr, Value)>,
    val_ty: RealizedTy,
    results: IndexMap<bex_vm_types::BexStr, Value>,
    idx: usize,
}

impl Continuation for MapFromJsonCont {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let key = self.entries[self.idx].0.clone();
        self.results.insert(key, value);
        let next_idx = self.idx + 1;
        map_drive(vm, self.entries, self.val_ty, self.results, next_idx)
    }
    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for (_, v) in &self.entries {
            if let Some(p) = v.as_object_ptr() {
                roots.push(p);
            }
        }
        for v in self.results.values() {
            if let Some(p) = v.as_object_ptr() {
                roots.push(p);
            }
        }
        roots
    }
    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for (_, v) in &mut self.entries {
            if let Some(p) = v.as_object_ptr() {
                if let Some(&new) = forwarding.get(&p) {
                    *v = Value::object(new);
                }
            }
        }
        for v in self.results.values_mut() {
            if let Some(p) = v.as_object_ptr() {
                if let Some(&new) = forwarding.get(&p) {
                    *v = Value::object(new);
                }
            }
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// If `ty` is a nullable union (`T | null`) and `v` is `Value::Null`, returns
/// `Some(Null)`. Otherwise `None` — caller should dispatch on the inner
/// (non-null) type.
fn optional_null_short_circuit(v: Value, ty: &RealizedTy) -> Option<Value> {
    if ty.is_nullable_union() && v.is_null() {
        Some(Value::NULL)
    } else {
        None
    }
}

/// Strip the outer nullable-union wrapper, if any. Used by the list/map walker
/// so that `T | null` element types still dispatch through `C.from_json` for
/// the non-null member.
fn peel_optional(ty: &RealizedTy) -> &RealizedTy {
    if let RealizedTy::Union(members, _) = ty {
        if members.iter().any(RealizedTy::is_null) {
            if let Some(inner) = members.iter().find(|m| !m.is_null()) {
                if members.iter().filter(|m| !m.is_null()).count() == 1 {
                    return inner;
                }
            }
        }
    }
    ty
}

/// Synchronous (no-yield) decode of `v` as `ty`. For class-with-override
/// elements the caller must use the yield path; this helper structurally
/// decodes everything else.
fn decode_value_sync(vm: &mut BexVm, v: Value, ty: &RealizedTy) -> Result<Value, VmRustFnError> {
    let serde = value_to_serde(vm, v);
    let mut path = String::new();
    ty_serde_to_value(vm, &serde, ty, &mut path)
}
