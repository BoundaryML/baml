//! Native handlers for `baml.json` namespace:
//! `parse`, `stringify`, `stringify_pretty`, `to_json`, `from_string<T>`.
//!
//! `from_string<T>` reads its type-arg `T` from `vm.current_call_type_args()`,
//! populated by the call-instruction handler from the leading `LoadType`
//! operand. See `BexVm::pending_call_type_args`.

// `path: &mut String` callees need ownership for `truncate` and `write!`.
// Match arms that throw via the VM error helpers read clearer than
// `let-else` since both branches contribute to the diagnostic.
#![allow(
    clippy::ptr_arg,
    clippy::manual_let_else,
    clippy::items_after_statements
)]

use std::sync::Arc;

use baml_type::MediaKind;
use bex_vm_types::{ForwardingMap, RealizedTy, TyTemplate};

/// The runtime type of an untyped `json` value: the recursive `baml.json.json`
/// alias (`null | bool | int | bigint | float | string | json[] | map<string, json>`).
/// Recursive aliases stay opaque in `RealizedTy`, so this is the most precise
/// element/value type available for containers parsed from untyped JSON.
pub(super) fn json_alias_ty(vm: &BexVm) -> RealizedTy {
    let head = vm
        .stdlib_heads()
        .json_alias()
        .unwrap_or_else(|| unreachable!("`baml.json.json` is declared by the stdlib"));
    RealizedTy::TypeAlias(head)
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

use bex_heap::TlabHolder;
use bex_vm_types::{
    HeapPtr, RustDataArc as _, ValueKind,
    types::{Array, Instance, Map, Object, Value},
};
use indexmap::IndexMap;

use super::{
    BamlNamespaceJson, Continuation, NativeCallResult, PackageBamlImpl, make_to_json_callee,
};
use crate::{
    BexVm,
    errors::{VmInternalError, VmRustFnError},
};

const STRING_MAP_REQUIRED: &str = "JSON requires maps with string keys";

fn string_map_entries(
    vm: &BexVm,
    map: &Map,
) -> Result<Vec<(bex_vm_types::BexStr, Value)>, &'static str> {
    if !baml_type::normalize::is_string_key(map.key_ty.as_ty(), vm) {
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

/// Entry point for `baml._to_json_default`. Collects the
/// override-bearing sub-values (pass 1), dispatches `to_json` on each in order
/// (pass 2), then renders structurally splicing in the override results (pass 3).
pub(super) fn render_to_json_honoring_overrides(vm: &mut BexVm, value: Value) -> NativeCallResult {
    let mut pending: Vec<HeapPtr> = Vec::new();
    if let Err(e) = collect_to_json_overrides(vm, value, &mut pending) {
        return NativeCallResult::Error(e.into());
    }

    let Some(&first_ptr) = pending.first() else {
        return render_to_json_done(vm, value, &pending, &[]);
    };
    match make_to_json_callee(vm, Value::object(first_ptr)) {
        Err(e) => NativeCallResult::Error(e.into()),
        Ok(Some(callee)) => NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(ToJsonWalkContinuation {
                root: value,
                pending,
                results: Vec::new(),
            }),
        },
        // Pass 1 collected `first_ptr` as an override; pass 2 must agree.
        Ok(None) => NativeCallResult::Error(
            VmInternalError::OverrideWalkSkew {
                interface: "ToJson",
            }
            .into(),
        ),
    }
}

/// Whether `value`'s runtime type carries a `baml.ToJson` override.
/// Shares `make_to_json_callee`'s rule resolution but allocates
/// nothing on the VM heap, so it is safe during the allocation-free pre-order
/// collection.
/// A resolver error PROPAGATES (`shim_rule_method`'s own contract: never a
/// silent structural fallback) — swallowing it here would let the
/// collection pass disagree with the dispatch pass.
fn has_to_json_override(vm: &BexVm, value: Value) -> Result<bool, VmInternalError> {
    Ok(matches!(
        super::shim_rule_method(vm, value, crate::lang_items::BAML_TO_JSON, "to_json")?,
        Some(resolved) if !resolved.is_default
    ))
}

/// Pre-order DFS collecting, by heap pointer and in render order, every
/// sub-value of `value` whose runtime class overrides `baml.ToJson`. An override
/// node is recorded and *not* descended into — its `to_json` owns its whole
/// subtree. Immutable and allocation-free so the collector cannot move objects
/// mid-walk. Matches `render_to_serde`'s traversal order (array elements, then
/// map values, then instance fields) so the two stay index-aligned. A media
/// instance is treated as a leaf, matching `render_to_serde`, which emits its
/// tagged form without descending into the opaque `_data` field.
fn collect_to_json_overrides(
    vm: &BexVm,
    value: Value,
    out: &mut Vec<HeapPtr>,
) -> Result<(), VmInternalError> {
    let ValueKind::Object(ptr) = value.kind() else {
        return Ok(());
    };
    if has_to_json_override(vm, value)? {
        out.push(ptr);
        return Ok(());
    }
    // Snapshot children (owned), dropping the heap borrow before recursing.
    let children: Vec<Value> = match vm.get_object(ptr) {
        Object::Array(values) => values.to_vec(),
        Object::Map(map) => map
            .snapshot_entries()
            .into_iter()
            .map(|(_, value)| value)
            .collect(),
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
        collect_to_json_overrides(vm, v, out)?;
    }
    Ok(())
}

/// Whether `inst`'s class is one of the builtin media classes.
fn is_media_instance(vm: &BexVm, inst: &Instance) -> bool {
    match vm.get_object(inst.class) {
        Object::Class(c) => vm
            .stdlib_heads()
            .media_kind(bex_vm_types::TypeHead::new(inst.class, c.type_tag))
            .is_some(),
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
/// ...) raises `SerializationError`. A node whose runtime class overrides
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
        Object::Bigint(b) => Snap::Leaf(bigint_to_serde(b)),
        Object::Array(values) => Snap::Seq(values.to_vec()),
        Object::Map(map) => match string_map_entries(vm, map) {
            Ok(entries) => Snap::Entries(
                entries
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect(),
            ),
            Err(message) => Snap::Unserializable(message),
        },
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
            let (class_head, field_names) = match vm.get_object(class_ptr) {
                Object::Class(c) => (
                    bex_vm_types::TypeHead::new(class_ptr, c.type_tag),
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
            if let Some(kind) = vm.stdlib_heads().media_kind(class_head) {
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
        self.results.push(json_value_to_serde(vm, value));

        // Dispatch the next override, if any; otherwise render. Every pending
        // pointer was collected as an override by pass 1, so a pass-2 miss is
        // a skew between the two passes, not a fallback case.
        if let Some(&next_ptr) = self.pending.get(self.results.len()) {
            return match make_to_json_callee(vm, Value::object(next_ptr)) {
                Err(e) => NativeCallResult::Error(e.into()),
                Ok(Some(callee)) => NativeCallResult::YieldToCall {
                    callee,
                    args: vec![],
                    type_args: vec![],
                    continuation: self,
                },
                Ok(None) => NativeCallResult::Error(
                    VmInternalError::OverrideWalkSkew {
                        interface: "ToJson",
                    }
                    .into(),
                ),
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

    fn apply_forwarding(&mut self, forwarding: &ForwardingMap) {
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

const JSON_PARSE_ERROR: baml_base::LangItem = crate::lang_items::BAML_JSON_PARSE_ERROR;
const JSON_DECODE_ERROR: baml_base::LangItem = crate::lang_items::BAML_JSON_DECODE_ERROR;
const JSON_SERIALIZATION_ERROR: baml_base::LangItem =
    crate::lang_items::BAML_JSON_SERIALIZATION_ERROR;

// ─── Trait implementation ─────────────────────────────────────────────────────

impl BamlNamespaceJson for PackageBamlImpl {
    fn parse(vm: &mut BexVm, s: &bex_str::BexStr) -> Result<Value, VmRustFnError> {
        json_parse(vm, s.as_str())
    }

    fn stringify(vm: &mut BexVm, j: &Value) -> bex_str::BexStr {
        let json_val = json_value_to_serde(vm, *j);
        let s = serde_json::to_string(&json_val)
            .unwrap_or_else(|e| unreachable!("JSON value failed to stringify: {e}"));
        bex_str::BexStr::from(s)
    }

    fn stringify_pretty(vm: &mut BexVm, j: &Value) -> bex_str::BexStr {
        let json_val = json_value_to_serde(vm, *j);
        let s = serde_json::to_string_pretty(&json_val)
            .unwrap_or_else(|e| unreachable!("JSON value failed to stringify: {e}"));
        bex_str::BexStr::from(s)
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
        // `baml.json.to_json` is a thin alias for the override-honoring
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
        // The public throws-never API accepts `json`, not arbitrary maps.
        match j.as_object_ptr() {
            Some(ptr) => match vm.get_object(ptr) {
                Object::Map(m) => string_map_entries(vm, m)
                    .unwrap_or_else(|e| unreachable!("baml.json.field received invalid json: {e}"))
                    .into_iter()
                    .find_map(|(k, value)| (k.as_str() == key.as_str()).then_some(value))
                    .unwrap_or(Value::NULL),
                _ => Value::NULL,
            },
            None => Value::NULL,
        }
    }
}

// ─── Parse ────────────────────────────────────────────────────────────────────

/// Parse a JSON string and return a `json`-typed VM value.
///
/// The `json` type alias is `null | bool | int | bigint | float | string | json[] | map<string, json>`,
/// which maps directly onto VM value kinds:
/// - JSON `null`   → `Value::NULL`
/// - JSON `bool`   → tagged Bool
/// - JSON integer  → tagged i63 when representable, otherwise a heap-boxed
///   `Object::Bigint`
/// - JSON float    → heap-boxed `Object::Float`
/// - JSON `string` → heap-boxed `Object::String`
/// - JSON array    → heap-boxed `Object::Array`
/// - JSON object   → heap-boxed `Object::Map`
///
/// On failure, throws a `baml.json.ParseError { message }` instance.
pub fn json_parse(vm: &mut BexVm, s: &str) -> Result<Value, VmRustFnError> {
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        let msg = e.to_string();
        VmRustFnError::thrown_fresh(throw_json_parse_error(vm, msg))
    })?;
    validate_json_bigint_bounds(&parsed)
        .map_err(|message| VmRustFnError::thrown_fresh(throw_json_parse_error(vm, message)))?;
    Ok(serde_to_value(vm, &parsed))
}

// ─── Error throwers ───────────────────────────────────────────────────────────

fn throw_json_parse_error(vm: &mut BexVm, message: String) -> Value {
    let class_ptr = vm.lang_object(JSON_PARSE_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val]))
}

fn throw_json_decode_error(vm: &mut BexVm, message: String, path: &str) -> Value {
    let class_ptr = vm.lang_object(JSON_DECODE_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val, path_val]))
}

fn throw_json_serialization_error(
    vm: &mut BexVm,
    message: String,
    path: &str,
    reason: &str,
) -> Value {
    let class_ptr = vm.lang_object(JSON_SERIALIZATION_ERROR);
    let message_val = Value::object(vm.alloc_string(message));
    let path_val = Value::object(vm.alloc_string(path.to_string()));
    let reason_val = Value::object(vm.alloc_string(reason.to_string()));
    Value::object(vm.alloc_instance(class_ptr, vec![message_val, path_val, reason_val]))
}

fn raise_decode(vm: &mut BexVm, message: impl Into<String>, path: &str) -> VmRustFnError {
    VmRustFnError::thrown_fresh(throw_json_decode_error(vm, message.into(), path))
}

fn raise_serialize(
    vm: &mut BexVm,
    message: impl Into<String>,
    path: &str,
    reason: &str,
) -> VmRustFnError {
    VmRustFnError::thrown_fresh(throw_json_serialization_error(
        vm,
        message.into(),
        path,
        reason,
    ))
}

// ─── serde_json ↔ VM Value conversion (untyped) ──────────────────────────────

/// Convert a `serde_json::Value` into a VM `Value`.
///
/// JSON numbers: i63-representable integers become integers, larger integral
/// tokens become bigints, and tokens with a fraction or exponent become
/// heap-boxed floats.
pub fn serde_to_value(vm: &mut BexVm, v: &serde_json::Value) -> Value {
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
pub fn value_to_serde(vm: &BexVm, v: Value) -> Result<serde_json::Value, ValueToSerdeError> {
    value_to_serde_at(vm, v, &mut String::new(), &mut Vec::new())
}

/// The error for a JSON object or array that contains itself: the walks
/// refuse it where they meet it again instead of descending forever.
const CYCLIC_JSON: &str = "cyclic json value: the document contains itself";

#[derive(Debug)]
pub struct ValueToSerdeError {
    message: &'static str,
    path: String,
}

/// Convert a statically checked `json` value at a throws-never boundary.
pub(super) fn json_value_to_serde(vm: &BexVm, v: Value) -> serde_json::Value {
    value_to_serde(vm, v)
        .unwrap_or_else(|e| unreachable!("invalid statically typed json value: {e:?}"))
}

// BUG: a `json` value that contains itself (`m["next"] = m`) never finishes
// rendering through `render_to_serde` and the string renderers, which recurse
// through the cycle; this walk and the typed decoder (`Decoder::open`) refuse
// it, hashing panics on it, and `==` terminates. The same open-set check
// belongs there.
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
        | Object::TypeAlias(_)
        | Object::Interface(_)
        | Object::Package(_)
        | Object::ImplRule(_)
        | Object::Function(_)
        | Object::Future(_)
        | Object::Type(_)
        | Object::Uint8Array(_)
        | Object::RustData(_)
        | Object::Closure(_)
        | Object::BoundMethod(_)
        | Object::GenericFunction(_)
        | Object::HostClosure(_)
        | Object::Cell(_) => serde_json::Value::Null,
        Object::Tombstone => Object::tombstone_reached(),
        #[cfg(feature = "heap_debug")]
        Object::Sentinel(_) => serde_json::Value::Null,
    })
}

fn untyped_value_to_serde(
    vm: &mut BexVm,
    value: Value,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    value_to_serde_at(vm, value, path, &mut Vec::new())
        .map_err(|e| raise_serialize(vm, e.message, &e.path, "map"))
}

// ─── Typed JSON serialize ────────────────────────────────────────────────────

/// Serialize a VM `Value` to a JSON string driven by the runtime `RealizedTy`.
///
/// Walks the value matching the shape of `ty`.  Throws
/// `SerializationError` for non-representable types (`uint8array`,
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
        RealizedTy::Null => Ok(serde_json::Value::Null),
        RealizedTy::Int | RealizedTy::Float | RealizedTy::Bool | RealizedTy::String => {
            untyped_value_to_serde(vm, value, path)
        }
        RealizedTy::Bigint => untyped_value_to_serde(vm, value, path),
        RealizedTy::Literal(_, _) => untyped_value_to_serde(vm, value, path),

        RealizedTy::List(elem) => {
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

        RealizedTy::Map { key, value: vty } => {
            if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
                return Err(raise_serialize(vm, STRING_MAP_REQUIRED, path, "map"));
            }
            let entries = match value.as_object_ptr() {
                Some(ptr) => match vm.get_object(ptr) {
                    Object::Map(m) => match string_map_entries(vm, m) {
                        Ok(entries) => entries,
                        Err(message) => return Err(raise_serialize(vm, message, path, "map")),
                    },
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

        RealizedTy::TypeAlias(head) if vm.stdlib_heads().is_json_alias(*head) => {
            untyped_value_to_serde(vm, value, path)
        }

        RealizedTy::TypeAlias(_) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            untyped_value_to_serde(vm, value, path)
        }

        RealizedTy::Class(head, _type_args) | RealizedTy::Interface(head, _type_args, _) => {
            serialize_class_instance(vm, value, *head, path)
        }

        RealizedTy::Enum(_) => match value.as_object_ptr() {
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

        RealizedTy::EnumVariant(_, name) => Ok(serde_json::Value::String(name.to_string())),

        RealizedTy::Media(kind) => serialize_media(vm, value, *kind, path),

        RealizedTy::Uint8Array => Err(raise_serialize(
            vm,
            "uint8array requires explicit encoding (use to_base64() or to_hex())",
            path,
            "uint8array",
        )),

        RealizedTy::Union(members) => {
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

        RealizedTy::Resource | RealizedTy::PromptAst => Err(raise_serialize(
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
        RealizedTy::Future(_, _) => Err(raise_serialize(
            vm,
            "cannot serialize future values",
            path,
            "future",
        )),
        RealizedTy::Unknown => Err(raise_serialize(
            vm,
            "cannot serialize unknown type",
            path,
            "unknown",
        )),
        // Type-level and opaque types carry no serializable runtime value:
        // reflection types (`Type`), opaque Rust state (`RustType`), and the
        // bottom type (`Never`).
        RealizedTy::Never | RealizedTy::RustType | RealizedTy::Type => Err(raise_serialize(
            vm,
            "cannot serialize compiler-only type",
            path,
            "compiler_only",
        )),
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
    head: bex_vm_types::TypeHead,
    path: &mut String,
) -> Result<serde_json::Value, VmRustFnError> {
    let named = baml_type::HeadDisplay::head_display_name(&head);
    let inst_ptr = match value.as_object_ptr() {
        Some(ptr) => ptr,
        None => {
            return Err(raise_serialize(
                vm,
                format!("expected class instance for `{named}`"),
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
                format!("expected class instance for `{named}`"),
                path,
                "class",
            ));
        }
    };

    if let Some(kind) = vm.stdlib_heads().media_kind(head) {
        return serialize_media(vm, value, kind, path);
    }

    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return Err(raise_serialize(
                vm,
                format!("instance class pointer for `{named}` is not a class"),
                path,
                "class",
            ));
        }
    };

    // Per BEP-038 (`@alias` / `@skip` are LLM-path-only): JSON interchange
    // always uses raw field names and includes every declared field, even
    // those marked `@skip`.  Aliased keys live exclusively on the
    // `ctx.output_format()` / FunctionSpec.parse LLM path.
    let mut out = serde_json::Map::with_capacity(class_fields.len());
    for (i, cf) in class_fields.iter().enumerate() {
        let Some(field_value) = field_values.get(i).copied() else {
            return Err(raise_serialize(
                vm,
                format!("class `{named}` has fewer fields than declared"),
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

/// Emit a tagged JSON object for a media value.
///
/// Shape: `{ "kind": "image"|..., "source": "url"|"base64", "value":
/// <data>, "mime": <mime_type-or-null> }`, with `"name": <name>` when the
/// value has one.  The `value` is the URL or the base64 payload depending on
/// `source`.
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
    } else {
        ("base64", media.base64().as_str().to_owned())
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
    if let Some(name) = media.name() {
        obj.insert("name".into(), serde_json::Value::String(name));
    }
    Ok(serde_json::Value::Object(obj))
}

pub(crate) fn read_media_value(vm: &BexVm, value: Value) -> Option<Arc<bex_vm_types::MediaValue>> {
    let ptr = value.as_object_ptr()?;
    let (class, data_value) = match vm.get_object(ptr) {
        Object::Instance(inst) => (inst.class, inst.fields.first()?.load()),
        _ => return None,
    };
    let class_head = match vm.get_object(class) {
        Object::Class(c) => bex_vm_types::TypeHead::new(class, c.type_tag),
        _ => return None,
    };
    vm.stdlib_heads().media_kind(class_head)?;

    let data_ptr = data_value.as_object_ptr()?;
    match vm.get_object(data_ptr) {
        Object::RustData(arc) => arc
            .clone()
            .downcast_payload::<bex_vm_types::MediaValue>()
            .ok(),
        _ => None,
    }
}

// ─── Typed JSON deserialize ──────────────────────────────────────────────────

/// Parse a JSON string and coerce it to a VM `Value` of the given runtime
/// `RealizedTy`.
///
/// Throws `ParseError` for invalid JSON and `DecodeError` when the
/// parsed JSON does not match the target type.
pub fn json_from_string_typed(
    vm: &mut BexVm,
    s: &str,
    ty: &RealizedTy,
) -> Result<Value, VmRustFnError> {
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        let msg = e.to_string();
        VmRustFnError::thrown_fresh(throw_json_parse_error(vm, msg))
    })?;
    validate_json_bigint_bounds(&parsed).map_err(|message| raise_decode(vm, message, ""))?;
    let mut path = String::new();
    ty_serde_to_value(vm, &parsed, ty, &mut path)
}

/// Convert a bigint to the arbitrary-precision JSON number representation used
/// by `serde_json`. `BigInt`'s decimal display is always valid JSON number syntax.
fn bigint_to_serde(value: &num_bigint::BigInt) -> serde_json::Value {
    match value.to_string().parse::<serde_json::Number>() {
        Ok(number) => serde_json::Value::Number(number),
        Err(_) => unreachable!("BigInt decimal display must be a valid JSON number"),
    }
}

/// Decode signed decimal text without passing through `f64`. Check the size
/// before parsing so untrusted JSON cannot allocate an unbounded integer.
fn parse_decimal_bigint(text: &str) -> Option<num_bigint::BigInt> {
    let (negative, digits) = match text.as_bytes().first() {
        Some(b'-') => (true, &text[1..]),
        Some(b'+') => (false, &text[1..]),
        _ => (false, text),
    };
    if digits.is_empty()
        || digits.len() > baml_type::MAX_BIGINT_DECIMAL_DIGITS
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let magnitude = num_bigint::BigInt::parse_bytes(digits.as_bytes(), 10)?;
    let value = if negative { -magnitude } else { magnitude };
    (value.bits() <= baml_type::MAX_BIGINT_BITS).then_some(value)
}

/// Decode an integral JSON number token without passing through `f64`.
fn parse_json_bigint(json: &serde_json::Value) -> Option<num_bigint::BigInt> {
    let serde_json::Value::Number(number) = json else {
        return None;
    };
    parse_decimal_bigint(&number.to_string())
}

/// Reject integral JSON tokens that cannot inhabit the runtime's bounded
/// bigint representation. Without this check, they would fall through to
/// `f64` and could silently become infinity (then serialize as `null`).
fn validate_json_bigint_bounds(json: &serde_json::Value) -> Result<(), String> {
    match json {
        serde_json::Value::Number(number) => {
            let text = number.to_string();
            let integral = !text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E'));
            if integral && parse_decimal_bigint(&text).is_none() {
                return Err(format!(
                    "integral JSON number exceeds the {}-bit bigint limit",
                    baml_type::MAX_BIGINT_BITS
                ));
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                validate_json_bigint_bounds(item)?;
            }
        }
        serde_json::Value::Object(entries) => {
            for value in entries.values() {
                validate_json_bigint_bounds(value)?;
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => {}
    }
    Ok(())
}

/// Typed bigint decoding also accepts decimal text in a JSON string. This is
/// intentionally separate from untyped JSON conversion, where a JSON string
/// must remain a string.
fn parse_typed_json_bigint(json: &serde_json::Value) -> Option<num_bigint::BigInt> {
    match json {
        serde_json::Value::Number(_) => parse_json_bigint(json),
        serde_json::Value::String(text) => parse_decimal_bigint(text),
        _ => None,
    }
}

/// Walk a parsed `serde_json::Value` driven by `ty`, allocating VM values.
/// Throws `DecodeError` on shape mismatch.
fn ty_serde_to_value(
    vm: &mut BexVm,
    json: &serde_json::Value,
    ty: &RealizedTy,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    match ty {
        RealizedTy::Null => match json {
            serde_json::Value::Null => Ok(Value::NULL),
            _ => Err(raise_decode(vm, "expected null", path)),
        },

        RealizedTy::Bool => match json {
            serde_json::Value::Bool(b) => Ok(Value::bool(*b)),
            _ => Err(raise_decode(vm, "expected boolean", path)),
        },

        RealizedTy::Int => match json {
            serde_json::Value::Number(n) => n.as_i64().and_then(Value::try_int).ok_or_else(|| {
                raise_decode(
                    vm,
                    "expected integer in the BAML int range [-2^62, 2^62 - 1]",
                    path,
                )
            }),
            _ => Err(raise_decode(vm, "expected integer", path)),
        },

        RealizedTy::Bigint => {
            let bigint = parse_typed_json_bigint(json).ok_or_else(|| {
                raise_decode(vm, "expected integral JSON number or numeric string", path)
            })?;
            vm.try_alloc_bigint(Arc::new(bigint)).map_err(Into::into)
        }

        RealizedTy::Float => match json {
            serde_json::Value::Number(n) => {
                if let Some(f) = n.as_f64() {
                    Ok(Value::object(vm.alloc_float(f)))
                } else {
                    Err(raise_decode(vm, "expected number", path))
                }
            }
            _ => Err(raise_decode(vm, "expected number", path)),
        },

        RealizedTy::String => match json {
            serde_json::Value::String(s) => Ok(Value::object(vm.alloc_string(s.clone()))),
            _ => Err(raise_decode(vm, "expected string", path)),
        },

        RealizedTy::List(elem) => match json {
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

        RealizedTy::Map { key, .. } if !baml_type::normalize::is_string_key(key.as_ty(), vm) => {
            Err(raise_decode(vm, STRING_MAP_REQUIRED, path))
        }
        RealizedTy::Map { key, value: vty } => match json {
            serde_json::Value::Object(map) => {
                let mut entries: IndexMap<bex_vm_types::BexStr, Value> =
                    IndexMap::with_capacity(map.len());
                for (k, val) in map {
                    if !baml_type::normalize::is_subtype(
                        &baml_type::Ty::Literal(
                            baml_type::Literal::String(k.clone()),
                            baml_type::Freshness::Regular,
                        ),
                        key.as_ty(),
                        vm,
                    ) {
                        return Err(raise_decode(
                            vm,
                            "map key does not match declared key type",
                            path,
                        ));
                    }
                    let v = with_path_segment(path, format_args!("[{k:?}]"), |p| {
                        ty_serde_to_value(vm, val, vty, p)
                    })?;
                    entries.insert(bex_vm_types::BexStr::from(k.as_str()), v);
                }
                Ok(Value::object(vm.tlab.alloc_map(
                    (**key).clone(),
                    (**vty).clone(),
                    entries,
                )))
            }
            _ => Err(raise_decode(vm, "expected object", path)),
        },

        RealizedTy::TypeAlias(head) if vm.stdlib_heads().is_json_alias(*head) => {
            Ok(serde_to_value(vm, json))
        }

        RealizedTy::TypeAlias(_) => {
            // Unknown / cross-package recursive aliases: fall back to untyped.
            Ok(serde_to_value(vm, json))
        }

        RealizedTy::Class(head, type_args) => {
            if let Some(kind) = vm.stdlib_heads().media_kind(*head) {
                return deserialize_media(vm, json, kind, *head, path);
            }
            deserialize_class_instance(vm, json, *head, type_args, path)
        }

        RealizedTy::Interface(head, type_args, _) => {
            deserialize_class_instance(vm, json, *head, type_args, path)
        }

        RealizedTy::Enum(head) => match json {
            serde_json::Value::String(s) => deserialize_enum_variant(vm, *head, s, path),
            _ => Err(raise_decode(vm, "expected enum variant string", path)),
        },

        RealizedTy::EnumVariant(head, name) => match json {
            serde_json::Value::String(s) if s == name.as_str() => {
                deserialize_enum_variant(vm, *head, s, path)
            }
            _ => Err(raise_decode(
                vm,
                format!("expected enum variant `{name}`"),
                path,
            )),
        },

        RealizedTy::Media(kind) => deserialize_media_by_kind(vm, json, *kind, path),

        RealizedTy::Uint8Array => Err(raise_decode(
            vm,
            "uint8array requires explicit encoding (use from_base64() or from_hex())",
            path,
        )),

        RealizedTy::Union(members) => {
            // Try each member structurally; first match wins.
            for member in members {
                let mut tmp_path = path.clone();
                if let Ok(v) = ty_serde_to_value(vm, json, member, &mut tmp_path) {
                    return Ok(v);
                }
            }
            Err(raise_decode(vm, "no union member matched", path))
        }

        RealizedTy::Literal(lit, _) => match (lit, json) {
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
            (baml_type::Literal::Bigint(expected), json) => {
                let actual = parse_typed_json_bigint(json).ok_or_else(|| {
                    raise_decode(vm, "expected integral JSON number or numeric string", path)
                })?;
                if &actual != expected {
                    return Err(raise_decode(vm, "literal bigint mismatch", path));
                }
                vm.try_alloc_bigint(Arc::new(actual)).map_err(Into::into)
            }
            _ => Err(raise_decode(vm, "literal mismatch", path)),
        },

        RealizedTy::Resource | RealizedTy::PromptAst => {
            Err(raise_decode(vm, "cannot deserialize opaque type", path))
        }

        RealizedTy::Function { .. } | RealizedTy::Future(_, _) | RealizedTy::Unknown => {
            // These variants do not provide a concrete JSON schema to validate
            // against here. Preserve structural JSON conversion for values
            // whose shape is already JSON-representable.
            Ok(serde_to_value(vm, json))
        }

        // Type-level and opaque types are not valid decode targets: reflection
        // types (`Type`), opaque Rust state (`RustType`), and the bottom type
        // (`Never`).
        RealizedTy::Never | RealizedTy::RustType | RealizedTy::Type => {
            Err(raise_decode(vm, "cannot decode compiler-only type", path))
        }
    }
}

fn deserialize_class_instance(
    vm: &mut BexVm,
    json: &serde_json::Value,
    head: bex_vm_types::TypeHead,
    type_args: &[RealizedTy],
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let named = baml_type::HeadDisplay::head_display_name(&head);
    let map = match json {
        serde_json::Value::Object(m) => m,
        _ => {
            return Err(raise_decode(
                vm,
                format!("expected JSON object for class `{named}`"),
                path,
            ));
        }
    };

    let class_ptr = head.ptr();
    let class_fields = match vm.get_object(class_ptr) {
        Object::Class(c) => c.fields.clone(),
        _ => {
            return Err(raise_decode(vm, format!("`{named}` is not a class"), path));
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
    head: bex_vm_types::TypeHead,
    variant_name: &str,
    path: &mut String,
) -> Result<Value, VmRustFnError> {
    let named = baml_type::HeadDisplay::head_display_name(&head);
    let enm_ptr = head.ptr();
    let idx = match vm.get_object(enm_ptr) {
        Object::Enum(e) => e.variants.iter().position(|v| v.name == variant_name),
        _ => {
            return Err(raise_decode(vm, format!("`{named}` is not an enum"), path));
        }
    };
    match idx {
        Some(i) => Ok(Value::object(vm.alloc_variant(enm_ptr, i))),
        None => Err(raise_decode(
            vm,
            format!("unknown variant `{variant_name}` for enum `{named}`"),
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
    let Some(wrapper) = kind.wrapper_class() else {
        return Err(raise_decode(
            vm,
            "cannot deserialize generic media - type must be concrete (image|audio|video|pdf)",
            path,
        ));
    };
    let head = vm
        .lang_head(wrapper)
        .ok_or_else(|| raise_decode(vm, format!("media class `{wrapper}` not found"), path))?;
    deserialize_media(vm, json, kind, head, path)
}

fn deserialize_media(
    vm: &mut BexVm,
    json: &serde_json::Value,
    kind: MediaKind,
    head: bex_vm_types::TypeHead,
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
    let tagged_kind = map
        .get("kind")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| raise_decode(vm, "media object missing `kind`", path))?;
    if tagged_kind != kind.tag_str() {
        return Err(raise_decode(
            vm,
            format!(
                "media kind mismatch: expected `{}`, got `{tagged_kind}`",
                kind.tag_str()
            ),
            path,
        ));
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

    let name = map.get("name").and_then(serde_json::Value::as_str);

    let media_arc: Arc<bex_vm_types::MediaValue> = match (source, name) {
        ("url", _) => bex_vm_types::MediaValue::from_url(kind, value_str, mime),
        ("base64" | "inline", Some(name)) => {
            bex_vm_types::MediaValue::from_file_content(kind, name, value_str.into(), mime)
        }
        ("base64" | "inline", None) => {
            bex_vm_types::MediaValue::from_base64(kind, value_str.into(), mime)
        }
        // Reading a path out of data would let any JSON name a file to read.
        ("file", _) => {
            return Err(raise_decode(
                vm,
                "media source `file` is not supported: a file is read when the value is built \
                 (use `from_file`), so JSON carries its content as `base64`",
                path,
            ));
        }
        (other, _) => {
            return Err(raise_decode(
                vm,
                format!("unknown media source `{other}` (expected url|base64)"),
                path,
            ));
        }
    };

    let class_ptr = head.ptr();
    let data_val = Value::object(vm.alloc_rust_data(media_arc));
    Ok(Value::object(vm.alloc_instance(class_ptr, vec![data_val])))
}

// ─── from_json dispatcher ────────────────────────────────────────────────────
//
// `baml.json.to<T>(j)`: the decode counterpart of `baml.json.from` /
// `baml.ToJson`, and the mirror of `==`: `baml.ops.equals_equals` lets a
// provided `Equals.eq` win and compares everything else structurally,
// recursively. Here the target type drives the walk; wherever it meets a
// declaration — class, interface, or enum, at the root or any depth, under an
// optional, in a list, in a map — a `baml.FromJson` rule that provides
// `from_json` builds the value, and anything else decodes structurally, its
// children by the same rule. The one place shape decides is a union: its
// member is chosen by the first structural match, because a provided
// `from_json` that throws unwinds the whole walk and cannot be tried and
// fallen through; the chosen member then decodes by the rule above.
//
// The walk suspends only to call a provided `from_json`. It keeps the
// containers it is inside on a stack of its own ([`Decoder`]), so its depth is
// neither a Rust recursion nor a chain of VM frames, and it resumes exactly
// where it yielded. A JSON value that contains itself is refused when the
// walk meets a container it already has open.

/// Reads the target type `T` from the call's type-args and decodes `j` as it,
/// the root structurally — the `baml._from_json_structural_default` native,
/// the body every implicit `baml.FromJson` implementation runs. The root's
/// own rule is not consulted (it IS this body); its fields, elements and
/// values honor theirs.
pub(super) fn json_to_structural_default(vm: &mut BexVm, j: Value) -> NativeCallResult {
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
    Decoder { open: Vec::new() }.decode(vm, j, &ty, Root::Structural)
}

/// Decode `j` as `ty`, honoring `baml.FromJson` at every depth.
fn json_to_dispatch(vm: &mut BexVm, j: Value, ty: &RealizedTy) -> NativeCallResult {
    Decoder { open: Vec::new() }.decode(vm, j, ty, Root::Dispatch)
}

/// How the ROOT value is decoded. Everything below it is dispatched.
#[derive(Clone, Copy)]
enum Root {
    /// A `baml.FromJson` rule providing `from_json` for the root type builds
    /// it.
    Dispatch,
    /// The root decodes structurally whatever its rule provides: this walk is
    /// the implicit implementation's own body.
    Structural,
}

/// What decoding one value as one type comes to.
enum Decoded {
    /// The value, decoded in place.
    Value(Value),
    /// A container: its children decode next, one at a time.
    Open(Container),
    /// A `baml.FromJson` rule provides `from_json` for the type: the method
    /// builds the value, and the walk resumes with what it returns.
    Provided {
        from_json: HeapPtr,
        frame: Vec<RealizedTy>,
        json: Value,
    },
}

/// A container the walk is inside: the JSON object or array it was opened
/// over, what is left to decode, and what it has decoded so far. The next
/// child is the one after the last result.
enum Container {
    Class {
        source: HeapPtr,
        class: HeapPtr,
        type_args: Vec<RealizedTy>,
        /// Each field's JSON value beside its realized type, in slot order.
        fields: Vec<(Value, RealizedTy)>,
        decoded: Vec<Value>,
    },
    List {
        source: HeapPtr,
        elements: Vec<Value>,
        element: RealizedTy,
        decoded: Vec<Value>,
    },
    Map {
        source: HeapPtr,
        entries: Vec<(bex_vm_types::BexStr, Value)>,
        key: RealizedTy,
        value: RealizedTy,
        decoded: IndexMap<bex_vm_types::BexStr, Value>,
    },
}

impl Container {
    /// The JSON object or array this container was opened over.
    fn source(&self) -> HeapPtr {
        match self {
            Self::Class { source, .. } | Self::List { source, .. } | Self::Map { source, .. } => {
                *source
            }
        }
    }

    /// The next child to decode — its JSON value and its type — or `None`
    /// once every child is. Every child goes through the full dispatch,
    /// whatever its type.
    fn next(&self) -> Option<(Value, RealizedTy)> {
        match self {
            Self::Class {
                fields, decoded, ..
            } => fields
                .get(decoded.len())
                .map(|(json, ty)| (*json, ty.clone())),
            Self::List {
                elements,
                element,
                decoded,
                ..
            } => elements
                .get(decoded.len())
                .map(|json| (*json, element.clone())),
            Self::Map {
                entries,
                value,
                decoded,
                ..
            } => entries
                .get(decoded.len())
                .map(|(_, json)| (*json, value.clone())),
        }
    }

    /// The path segment of the next child — `.field`, `[index]`, or
    /// `["key"]`, the notation `DecodeError.path` uses — or `None` once every
    /// child is decoded.
    fn segment(&self, vm: &BexVm) -> Option<String> {
        match self {
            Self::Class { class, decoded, .. } => match vm.get_object(*class) {
                Object::Class(class) => class
                    .fields
                    .get(decoded.len())
                    .map(|field| format!(".{}", field.name)),
                _ => unreachable!("an open class container holds a class"),
            },
            Self::List {
                elements, decoded, ..
            } => (decoded.len() < elements.len()).then(|| format!("[{}]", decoded.len())),
            Self::Map {
                entries, decoded, ..
            } => entries
                .get(decoded.len())
                .map(|(key, _)| format!("[{:?}]", key.as_str())),
        }
    }

    /// Record the next child's decoded value.
    fn push(&mut self, child: Value) {
        match self {
            Self::Class { decoded, .. } | Self::List { decoded, .. } => decoded.push(child),
            Self::Map {
                entries, decoded, ..
            } => {
                // A JSON object's keys are distinct, so the next entry is the
                // one after the last result.
                let key = entries[decoded.len()].0.clone();
                decoded.insert(key, child);
            }
        }
    }

    /// The container's value, once every child is decoded.
    fn finish(self, vm: &mut BexVm) -> Value {
        Value::object(match self {
            Self::Class {
                class,
                type_args,
                decoded,
                ..
            } => vm.tlab.alloc(Object::Instance(Instance::new(
                class,
                type_args.into(),
                decoded,
            ))),
            // Type-directed: the list carries the element type it decoded.
            Self::List {
                element, decoded, ..
            } => vm.tlab.alloc(Object::Array(Array::new(element, decoded))),
            // Type-directed: the declared key type (string-keyed, checked at
            // opening) and the value type it decoded.
            Self::Map {
                key,
                value,
                decoded,
                ..
            } => vm.tlab.alloc_map(key, value, decoded),
        })
    }

    /// Every value and declaration this container holds across a yield.
    fn roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.source());
        self.values(&mut |value| roots.extend(value.as_object_ptr()));
        if let Self::Class { class, .. } = self {
            roots.push(*class);
        }
        self.types(&mut |ty| {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        });
    }

    /// Repoint everything [`Self::roots`] reported after a collection.
    fn forward(&mut self, forwarding: &ForwardingMap) {
        match self {
            Self::Class { source, .. } | Self::List { source, .. } | Self::Map { source, .. } => {
                if let Some(moved) = forwarding.get(source) {
                    *source = *moved;
                }
            }
        }
        self.values_mut(&mut |value| {
            if let Some(moved) = value
                .as_object_ptr()
                .and_then(|ptr| forwarding.get(&ptr).copied())
            {
                *value = Value::object(moved);
            }
        });
        if let Self::Class { class, .. } = self
            && let Some(moved) = forwarding.get(class)
        {
            *class = *moved;
        }
        self.types_mut(&mut |ty| {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(moved) = forwarding.get(&head.ptr())
                {
                    head.forward_to(*moved);
                }
            });
        });
    }

    /// Every value this container holds: the JSON left to decode and what it
    /// has decoded.
    fn values(&self, visit: &mut dyn FnMut(&Value)) {
        match self {
            Self::Class {
                fields, decoded, ..
            } => fields
                .iter()
                .map(|(json, _)| json)
                .chain(decoded)
                .for_each(visit),
            Self::List {
                elements, decoded, ..
            } => elements.iter().chain(decoded).for_each(visit),
            Self::Map {
                entries, decoded, ..
            } => entries
                .iter()
                .map(|(_, json)| json)
                .chain(decoded.values())
                .for_each(visit),
        }
    }

    /// [`Self::values`], mutably.
    fn values_mut(&mut self, visit: &mut dyn FnMut(&mut Value)) {
        match self {
            Self::Class {
                fields, decoded, ..
            } => fields
                .iter_mut()
                .map(|(json, _)| json)
                .chain(decoded)
                .for_each(visit),
            Self::List {
                elements, decoded, ..
            } => elements.iter_mut().chain(decoded).for_each(visit),
            Self::Map {
                entries, decoded, ..
            } => entries
                .iter_mut()
                .map(|(_, json)| json)
                .chain(decoded.values_mut())
                .for_each(visit),
        }
    }

    /// Every type this container carries.
    fn types(&self, visit: &mut dyn FnMut(&RealizedTy)) {
        match self {
            Self::Class {
                type_args, fields, ..
            } => type_args
                .iter()
                .chain(fields.iter().map(|(_, ty)| ty))
                .for_each(visit),
            Self::List { element, .. } => visit(element),
            Self::Map { key, value, .. } => {
                visit(key);
                visit(value);
            }
        }
    }

    /// [`Self::types`], mutably.
    fn types_mut(&mut self, visit: &mut dyn FnMut(&mut RealizedTy)) {
        match self {
            Self::Class {
                type_args, fields, ..
            } => type_args
                .iter_mut()
                .chain(fields.iter_mut().map(|(_, ty)| ty))
                .for_each(visit),
            Self::List { element, .. } => visit(element),
            Self::Map { key, value, .. } => {
                visit(key);
                visit(value);
            }
        }
    }
}

/// The walk's state: the containers it is inside, outermost first. It is its
/// own continuation — what a provided `from_json` returns is the next child of
/// the innermost container, or the whole result when there is none.
struct Decoder {
    open: Vec<Container>,
}

impl Decoder {
    /// Decode `json` as `ty` (the root, per `root`) inside the open
    /// containers, then keep walking.
    fn decode(
        mut self,
        vm: &mut BexVm,
        json: Value,
        ty: &RealizedTy,
        root: Root,
    ) -> NativeCallResult {
        match decode_one(vm, json, ty, root, "") {
            Ok(Decoded::Value(value)) => self.resume(vm, value),
            Ok(Decoded::Open(container)) => match self.open(vm, container, "") {
                Ok(()) => self.walk(vm),
                Err(error) => NativeCallResult::Error(error),
            },
            Ok(Decoded::Provided {
                from_json,
                frame,
                json,
            }) => NativeCallResult::YieldToCall {
                callee: from_json,
                args: vec![json],
                type_args: frame,
                continuation: Box::new(self),
            },
            Err(error) => NativeCallResult::Error(error),
        }
    }

    /// Enter `container`, found at `path`. A JSON object or array the walk
    /// already has open is a value that contains itself: decoding it as a type
    /// would never end.
    fn open(
        &mut self,
        vm: &mut BexVm,
        container: Container,
        path: &str,
    ) -> Result<(), VmRustFnError> {
        let source = container.source();
        if self.open.iter().any(|open| open.source() == source) {
            return Err(raise_decode(vm, CYCLIC_JSON, path));
        }
        self.open.push(container);
        Ok(())
    }

    /// The path of the next child of the innermost container, from the
    /// root: each open container's segment for the child it is on. The
    /// innermost may have none left (it is about to finish); every outer one
    /// is on the child that opened the next.
    fn path(&self, vm: &BexVm) -> String {
        self.open
            .iter()
            .filter_map(|container| container.segment(vm))
            .collect()
    }

    /// `value` is the next child of the innermost container; with none open
    /// it is the result.
    fn resume(mut self, vm: &mut BexVm, value: Value) -> NativeCallResult {
        match self.open.last_mut() {
            Some(container) => {
                container.push(value);
                self.walk(vm)
            }
            None => NativeCallResult::Done(value),
        }
    }

    /// Decode children until one yields or the outermost container is done.
    fn walk(mut self, vm: &mut BexVm) -> NativeCallResult {
        loop {
            let path = self.path(vm);
            let container = self
                .open
                .last_mut()
                .unwrap_or_else(|| unreachable!("the walk runs inside an open container"));
            match container.next() {
                Some((json, ty)) => match decode_one(vm, json, &ty, Root::Dispatch, &path) {
                    Ok(Decoded::Value(value)) => container.push(value),
                    Ok(Decoded::Open(child)) => {
                        if let Err(error) = self.open(vm, child, &path) {
                            return NativeCallResult::Error(error);
                        }
                    }
                    Ok(Decoded::Provided {
                        from_json,
                        frame,
                        json,
                    }) => {
                        return NativeCallResult::YieldToCall {
                            callee: from_json,
                            args: vec![json],
                            type_args: frame,
                            continuation: Box::new(self),
                        };
                    }
                    Err(error) => return NativeCallResult::Error(error),
                },
                None => {
                    let finished = self
                        .open
                        .pop()
                        .unwrap_or_else(|| unreachable!("the container was just read"))
                        .finish(vm);
                    match self.open.last_mut() {
                        Some(parent) => parent.push(finished),
                        None => return NativeCallResult::Done(finished),
                    }
                }
            }
        }
    }
}

impl Continuation for Decoder {
    fn call(self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.resume(vm, value)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for container in &self.open {
            container.roots(&mut roots);
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &ForwardingMap) {
        for container in &mut self.open {
            container.forward(forwarding);
        }
    }
}

/// Decode `json` as `ty`, found at `path`, one level: its value if it decodes
/// in place, the container to walk if it has children, or the `from_json`
/// that builds it.
///
/// - a nullable union: `null` is `null`, anything else decodes as the
///   non-null payload;
/// - any other union: the first member `json` matches structurally is the
///   one, and decodes by the rules below;
/// - a list or map opens, to decode each element or value;
/// - a class, interface, or enum (media aside): a `baml.FromJson` rule that
///   provides `from_json` wins — unless this is a [`Root::Structural`] root,
///   whose rule is the walk itself — else a class opens to decode each field
///   and an enum decodes in place;
/// - everything else (primitives, media, literals, type aliases) decodes
///   structurally — none can carry an override.
fn decode_one(
    vm: &mut BexVm,
    json: Value,
    ty: &RealizedTy,
    root: Root,
    path: &str,
) -> Result<Decoded, VmRustFnError> {
    match ty {
        RealizedTy::Union(members) if members.iter().any(RealizedTy::is_null) => {
            if json.is_null() {
                Ok(Decoded::Value(Value::NULL))
            } else {
                decode_one(vm, json, &ty.strip_null(), Root::Dispatch, path)
            }
        }
        RealizedTy::Union(members) => {
            // Membership by shape: a provided `from_json` that throws unwinds
            // the whole walk, so a member cannot be tried through its rule
            // and fallen through to the next.
            let serde = leaf_serde(vm, json, path)?;
            let member = members.iter().find(|member| {
                let mut probe = path.to_string();
                ty_serde_to_value(vm, &serde, member, &mut probe).is_ok()
            });
            match member {
                Some(member) => decode_one(vm, json, &member.clone(), Root::Dispatch, path),
                None => Err(raise_decode(vm, "no union member matched", path)),
            }
        }
        RealizedTy::List(element) => open_list(vm, json, element, path).map(Decoded::Open),
        RealizedTy::Map { key, value } => open_map(vm, json, key, value, path).map(Decoded::Open),
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _)
            if vm.stdlib_heads().media_kind(*head).is_some() =>
        {
            decode_leaf(vm, json, ty, path).map(Decoded::Value)
        }
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _) | RealizedTy::Enum(head) => {
            let provided = match root {
                Root::Dispatch => provided_from_json(vm, *head, ty),
                Root::Structural => None,
            };
            match (provided, ty) {
                (Some(provided), _) => provided.map(|(from_json, frame)| Decoded::Provided {
                    from_json,
                    frame,
                    json,
                }),
                (
                    None,
                    RealizedTy::Class(_, type_args) | RealizedTy::Interface(_, type_args, _),
                ) => open_class(vm, json, *head, type_args, path).map(Decoded::Open),
                (None, _) => decode_leaf(vm, json, ty, path).map(Decoded::Value),
            }
        }
        _ => decode_leaf(vm, json, ty, path).map(Decoded::Value),
    }
}

/// The VM `json` value at `path` as `serde_json::Value`, for a leaf or a
/// union candidate: a value that contains itself is a decode error at the
/// place the walk met it again.
fn leaf_serde(vm: &mut BexVm, json: Value, path: &str) -> Result<serde_json::Value, VmRustFnError> {
    value_to_serde(vm, json).map_err(|e| raise_decode(vm, e.message, &format!("{path}{}", e.path)))
}

/// Decode `json` at `path` as `ty` structurally, in place: convert the VM
/// `json` value to `serde_json::Value` and run `ty_serde_to_value`. For the
/// types that carry no `baml.FromJson` rule of their own.
fn decode_leaf(
    vm: &mut BexVm,
    json: Value,
    ty: &RealizedTy,
    path: &str,
) -> Result<Value, VmRustFnError> {
    let serde = leaf_serde(vm, json, path)?;
    let mut path = path.to_string();
    ty_serde_to_value(vm, &serde, ty, &mut path)
}

/// The entries of the JSON object `json` at `path`: a string-keyed map and
/// nothing else — `expected` names what was wanted when it is not an object.
fn json_object_entries(
    vm: &mut BexVm,
    json: Value,
    expected: impl FnOnce() -> String,
    path: &str,
) -> Result<Vec<(bex_vm_types::BexStr, Value)>, VmRustFnError> {
    let entries = match json.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::Map(map)) => string_map_entries(vm, map).map_err(str::to_owned),
        Some(_) | None => Err(expected()),
    };
    entries.map_err(|message| raise_decode(vm, message, path))
}

/// The class at `head` (instantiated with `type_args`), opened over the JSON
/// object `json`: each declared field's JSON value beside its realized type.
/// An absent optional (`T?`) field is `null`; an absent required one is an
/// error.
fn open_class(
    vm: &mut BexVm,
    json: Value,
    head: bex_vm_types::TypeHead,
    type_args: &[RealizedTy],
    path: &str,
) -> Result<Container, VmRustFnError> {
    let named = baml_type::HeadDisplay::head_display_name(&head);
    let object: IndexMap<bex_vm_types::BexStr, Value> = json_object_entries(
        vm,
        json,
        || format!("expected JSON object for class `{named}`"),
        path,
    )?
    .into_iter()
    .collect();
    let source = json
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("the entries were read from a JSON object"));
    let class = head.ptr();
    let class_fields = match vm.get_object(class) {
        Object::Class(class) => class.fields.clone(),
        _ => return Err(raise_decode(vm, format!("`{named}` is not a class"), path)),
    };
    let mut fields: Vec<(Value, RealizedTy)> = Vec::with_capacity(class_fields.len());
    for field in &class_fields {
        let field_ty = vm.realize_field_ty(&field.field_template, type_args);
        let field_json = match object.get(field.name.as_str()) {
            Some(value) => *value,
            // Optional (`T?` == `T | null`) fields may be absent → null.
            None if field_ty.is_nullable_union() => Value::NULL,
            None => {
                return Err(raise_decode(
                    vm,
                    format!("missing required field `{}`", field.name),
                    path,
                ));
            }
        };
        fields.push((field_json, field_ty));
    }
    Ok(Container::Class {
        source,
        class,
        type_args: type_args.to_vec(),
        decoded: Vec::with_capacity(fields.len()),
        fields,
    })
}

/// A list of `element`, opened over the JSON array `json`.
fn open_list(
    vm: &mut BexVm,
    json: Value,
    element: &RealizedTy,
    path: &str,
) -> Result<Container, VmRustFnError> {
    match json.as_object_ptr().map(|ptr| (ptr, vm.get_object(ptr))) {
        Some((source, Object::Array(array))) => {
            let elements = array.to_vec();
            Ok(Container::List {
                source,
                decoded: Vec::with_capacity(elements.len()),
                elements,
                element: element.clone(),
            })
        }
        Some(_) | None => Err(raise_decode(vm, "expected JSON array", path)),
    }
}

/// A map from `key` to `value`, opened over the JSON object `json`. JSON
/// objects have string keys, so `key` must be a string type and every key
/// written must be one of it.
fn open_map(
    vm: &mut BexVm,
    json: Value,
    key: &RealizedTy,
    value: &RealizedTy,
    path: &str,
) -> Result<Container, VmRustFnError> {
    if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
        return Err(raise_decode(vm, STRING_MAP_REQUIRED, path));
    }
    let entries = json_object_entries(vm, json, || "expected JSON object".to_string(), path)?;
    let source = json
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("the entries were read from a JSON object"));
    for (written, _) in &entries {
        if !baml_type::normalize::is_subtype(
            &baml_type::Ty::Literal(
                baml_type::Literal::String(written.to_string()),
                baml_type::Freshness::Regular,
            ),
            key.as_ty(),
            vm,
        ) {
            return Err(raise_decode(
                vm,
                "map key does not match declared key type",
                path,
            ));
        }
    }
    Ok(Container::Map {
        source,
        decoded: IndexMap::with_capacity(entries.len()),
        entries,
        key: key.clone(),
        value: value.clone(),
    })
}

/// The `from_json` a `baml.FromJson` rule PROVIDES for `ty`, the class,
/// interface, or enum type at `head`, with the rule's realized frame — the
/// deserialize counterpart of the `ToJson` shim (`make_to_json_callee`),
/// resolved through the impl rules like it (so blanket and out-of-body impls
/// and runtime-declared classes all reach their provided method). `None` for a
/// type whose rule is absent or adopts a default body, which the walk decodes
/// structurally.
fn provided_from_json(
    vm: &mut BexVm,
    head: bex_vm_types::TypeHead,
    ty: &RealizedTy,
) -> Option<Result<(HeapPtr, Vec<RealizedTy>), VmRustFnError>> {
    // Type-directed dispatch: there is no receiver value, so the resolver
    // roots in the world that owns the target declaration (its package for a
    // runtime class; the lexical frame's world for static declarations).
    let from_json_head = vm.lang_head(crate::lang_items::BAML_FROM_JSON)?;
    let owner = match vm.get_object(head.ptr()) {
        Object::Class(class) => class
            .owner
            .package()
            .and_then(|owner| vm.runtime_owner(owner)),
        Object::Enum(enum_def) => enum_def
            .owner
            .package()
            .and_then(|owner| vm.runtime_owner(owner)),
        // An interface head carries the same owner edge; rooting it the
        // same way keeps runtime-declared interfaces symmetric with
        // runtime classes instead of falling to the lexical world.
        Object::Interface(iface) => vm.runtime_owner(iface.owner),
        _ => None,
    };
    let resolver = match owner {
        Some(owner) => super::resolve::ImplResolver::for_package(vm, owner),
        None => super::resolve::ImplResolver::new(vm),
    };
    let (rule, bound_args) = resolver.resolve_implements_rule(ty, from_json_head, &[])?;
    let resolved = match resolver.rule_method_impl(&rule, "from_json") {
        Ok(resolved) => resolved,
        Err(error) => return Some(Err(error.into())),
    };
    if resolved.is_default {
        // The rule adopts the structural default body: the walk decodes the
        // type structurally itself.
        return None;
    }
    Some(
        resolver
            .realize_frame(&resolved.method.frame, &bound_args)
            .map(|frame| (resolved.method.fqn, frame))
            .map_err(Into::into),
    )
}
