//! Native handlers for `baml.json` namespace:
//! `parse`, `stringify`, `stringify_pretty`, `to_json`, `to_string`,
//! `from_string<T>`, `from_json<T>`, `deserialize<T>`.
//!
//! `from_string<T>`, `from_json<T>` and `deserialize<T>` read their type-arg
//! `T` from `vm.current_call_type_args()`, populated by the call-instruction
//! handler from the leading `LoadType` operand. See
//! `BexVm::pending_call_type_args`.

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

/// What the override-honoring `to_json` walk materializes its rendered
/// `serde_json::Value` as once every override has run.
#[derive(Clone, Copy)]
enum ToJsonFinish {
    /// A `json` value on the VM heap (`baml.json.to_json`, `baml.json.from`).
    Json,
    /// Compact JSON text (`baml.json.to_string`): the skeleton is serialized
    /// straight from serde space, never materialized as a `json` value.
    String,
}

/// Entry point for `baml._to_json_default` and `baml.json.to_json`. Collects
/// the override-bearing sub-values (pass 1), dispatches `to_json` on each in
/// order (pass 2), then renders structurally splicing in the override results
/// (pass 3).
pub(super) fn render_to_json_honoring_overrides(vm: &mut BexVm, value: Value) -> NativeCallResult {
    render_to_json_as(vm, value, ToJsonFinish::Json)
}

/// Entry point for `baml.json.to_string`: the same three passes as
/// [`render_to_json_honoring_overrides`], finishing as JSON text. Equivalent to
/// `stringify(to_json(value))` — the `json` tree that composition built and
/// immediately re-read is what this skips.
pub(super) fn render_to_json_string_honoring_overrides(
    vm: &mut BexVm,
    value: Value,
) -> NativeCallResult {
    render_to_json_as(vm, value, ToJsonFinish::String)
}

fn render_to_json_as(vm: &mut BexVm, value: Value, finish: ToJsonFinish) -> NativeCallResult {
    let mut pending: Vec<HeapPtr> = Vec::new();
    if let Err(e) = collect_to_json_overrides(vm, value, &mut pending) {
        return NativeCallResult::Error(e.into());
    }

    let Some(&first_ptr) = pending.first() else {
        return render_to_json_done(vm, value, &pending, &[], finish);
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
                finish,
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
/// precomputed override `results` by pre-order position), then materialize it
/// per `finish`: the `json` value with a single `serde_to_value`, or its
/// compact text with a single `serde_json::to_string`.
///
/// The text is byte-identical to `stringify(to_json(root))`: every number the
/// walk renders — an `int` as its decimal, a finite `float` through
/// `Number::from_f64`, a non-finite one as `null`, a `bigint` as its decimal —
/// reads back through `serde_to_value` and `value_to_serde` to the same text,
/// and strings, arrays and objects (whose keys are distinct) round-trip as
/// they are.
fn render_to_json_done(
    vm: &mut BexVm,
    root: Value,
    pending: &[HeapPtr],
    results: &[serde_json::Value],
    finish: ToJsonFinish,
) -> NativeCallResult {
    let mut counter = 0;
    let mut path = String::new();
    match render_to_serde(vm, root, pending, results, &mut counter, &mut path) {
        Ok(serde) => NativeCallResult::Done(match finish {
            ToJsonFinish::Json => serde_to_value(vm, &serde),
            ToJsonFinish::String => {
                let text = serde_json::to_string(&serde)
                    .unwrap_or_else(|e| unreachable!("JSON value failed to stringify: {e}"));
                Value::object(vm.alloc_string(text))
            }
        }),
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
    /// What the rendered skeleton becomes once every override has run.
    finish: ToJsonFinish,
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
        render_to_json_done(vm, self.root, &self.pending, &self.results, self.finish)
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

    fn to_string(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        // `stringify(to_json(value))` without the intermediate `json` tree:
        // the same override-honoring walk, serialized from its serde skeleton.
        render_to_json_string_honoring_overrides(vm, *value)
    }

    fn deserialize(vm: &mut BexVm, s: &bex_str::BexStr) -> NativeCallResult {
        let ty = match vm.current_call_type_args().first().cloned() {
            Some(t) => t,
            None => {
                return NativeCallResult::Error(VmRustFnError::InternalError(
                    VmInternalError::MissingNativeFunction {
                        name: "baml.json.deserialize: missing type argument".to_string(),
                    },
                ));
            }
        };
        json_deserialize(vm, s.as_str(), &ty)
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
    let parsed = parse_document(vm, s)?;
    Ok(serde_to_value(vm, &parsed))
}

/// `s` as one JSON document in serde space: what `parse` reads before it
/// materializes the `json` value. Throws `ParseError` for a syntax error and
/// for an integral number outside the bigint limit, so every reader of text
/// that goes through here (`parse`, `deserialize<T>`) raises the same errors.
fn parse_document(vm: &mut BexVm, s: &str) -> Result<serde_json::Value, VmRustFnError> {
    let parsed: serde_json::Value = serde_json::from_str(s).map_err(|e| {
        let msg = e.to_string();
        VmRustFnError::thrown_fresh(throw_json_parse_error(vm, msg))
    })?;
    validate_json_bigint_bounds(&parsed)
        .map_err(|message| VmRustFnError::thrown_fresh(throw_json_parse_error(vm, message)))?;
    Ok(parsed)
}

// ─── deserialize<T> (parse + override-honoring decode, fused) ────────────────
//
// `baml.json.deserialize<T>(s)` is `from_json<T>(parse(s))`: `parse` builds
// the whole document as a tree of `json` maps and arrays on the VM heap, and
// the decoder walks that tree once and discards it. When nothing in `T` can
// observe the tree — no class, interface or enum reachable from `T` provides
// a `baml.FromJson` override — the decoder's every leaf is `ty_serde_to_value`
// over the leaf's serde form, so decoding `T` straight from the parsed serde
// document is the same walk minus the tree. That is the fused path.
//
// Equivalence, case by case, for an override-free `T`:
// - ParseError: both paths read the text with `parse_document`.
// - the leaf serde values the decoder sees are `value_to_serde` of what
//   `serde_to_value` built from the parsed document. That round trip is the
//   identity except on two number spellings (`-0`, and a float that overflows
//   `f64`), so a document containing either takes the two-pass path
//   (`numbers_round_trip`).
// - containers: the decoder's list, map and class openings check the same
//   conditions as the structural arms (array/object shape, string keys, key
//   literal membership, missing required field, absent optional as `null`)
//   and build the same `Array`/`Map`/`Instance` (same element, key and value
//   types, same field order, same entry order — `preserve_order` keeps the
//   object's order and both paths insert in it).
// - unions: a nullable union strips `null` and decodes the rest; any other
//   union is decided by the first member that decodes structurally. The
//   structural arm tries the members in order, so the member chosen and the
//   value built are the same.
// - a value the structural pass accepts is therefore one the decoder accepts,
//   with the same result. Where the structural pass REJECTS, the decoder's
//   error can differ in message and `path` (a missing field is reported at
//   the class, `expected JSON array` against `expected array`, a nullable
//   payload's own error against `no union member matched`), so a rejection
//   is not reported: the two-pass path runs over the already-parsed document
//   and raises the decoder's own error.
//
// A `T` that reaches an override, and a document whose numbers do not round
// trip, take the two-pass path from the start.

/// `baml.json.deserialize<T>`: decode the JSON text `s` as `ty`, honoring
/// `baml.FromJson` at every depth. Throws `ParseError` for text that is not
/// one JSON document and `DecodeError` where the document does not fit `ty`.
pub fn json_deserialize(vm: &mut BexVm, s: &str, ty: &RealizedTy) -> NativeCallResult {
    let parsed = match parse_document(vm, s) {
        Ok(parsed) => parsed,
        Err(error) => return NativeCallResult::Error(error),
    };
    match reaches_from_json_override(vm, ty) {
        Ok(false) if numbers_round_trip(&parsed) => {
            let mut path = String::new();
            match ty_serde_to_value(vm, &parsed, ty, &mut path) {
                Ok(value) => return NativeCallResult::Done(value),
                // A rejection: the decoder reports it, in its own words.
                Err(VmRustFnError::Thrown { .. }) => {}
                Err(error) => return NativeCallResult::Error(error),
            }
        }
        Ok(_) => {}
        Err(error) => return NativeCallResult::Error(error),
    }
    let json = serde_to_value(vm, &parsed);
    json_to_dispatch(vm, json, ty)
}

/// Whether every number in `parsed` reads back to the same text after
/// `serde_to_value` then `value_to_serde` — the round trip the two-pass path
/// puts every leaf through. An integral token is its own canonical decimal
/// (JSON forbids leading zeros and `+`) except `-0`, which reads as the int
/// `0`; a token with a fraction or exponent reads as an `f64` and comes back
/// through `Number::from_f64`, which spells the same `f64` (its text may
/// differ, but every typed arm reads it as the same number) unless the token
/// overflowed, when it comes back as `null`.
fn numbers_round_trip(parsed: &serde_json::Value) -> bool {
    match parsed {
        serde_json::Value::Number(number) => {
            let text = number.as_str();
            if text.bytes().any(|byte| matches!(byte, b'.' | b'e' | b'E')) {
                number.as_f64().is_some()
            } else {
                text != "-0"
            }
        }
        serde_json::Value::Array(items) => items.iter().all(numbers_round_trip),
        serde_json::Value::Object(entries) => entries.values().all(numbers_round_trip),
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::String(_) => true,
    }
}

/// The most declarations [`reaches_from_json_override`] visits before it
/// gives up and answers `true`; a type graph larger than this (a generic
/// class recursing at ever-new arguments) takes the two-pass path.
const OVERRIDE_WALK_LIMIT: usize = 64;

/// Whether decoding as `ty` can meet a `baml.FromJson` rule that PROVIDES
/// `from_json`: the walk the decoder makes, over the types rather than a
/// value. A class or interface is asked through `provided_from_json`, then
/// its fields (realized against its type args); an enum is asked; a list,
/// map or union descends; everything else — primitives, literals, media,
/// aliases (opaque to the decoder too) — decodes structurally. A resolver
/// error counts as reachable: the two-pass path raises it.
fn reaches_from_json_override(vm: &mut BexVm, ty: &RealizedTy) -> Result<bool, VmRustFnError> {
    let mut visited: Vec<RealizedTy> = Vec::new();
    reaches_from_json_override_at(vm, ty, &mut visited)
}

fn reaches_from_json_override_at(
    vm: &mut BexVm,
    ty: &RealizedTy,
    visited: &mut Vec<RealizedTy>,
) -> Result<bool, VmRustFnError> {
    match ty {
        RealizedTy::Union(members) => {
            for member in members {
                if reaches_from_json_override_at(vm, member, visited)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        RealizedTy::List(element) => reaches_from_json_override_at(vm, element, visited),
        RealizedTy::Map { key, value } => Ok(reaches_from_json_override_at(vm, key, visited)?
            || reaches_from_json_override_at(vm, value, visited)?),
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _)
            if vm.stdlib_heads().media_kind(*head).is_some() =>
        {
            Ok(false)
        }
        RealizedTy::Class(head, type_args) | RealizedTy::Interface(head, type_args, _) => {
            if visited.contains(ty) {
                return Ok(false);
            }
            if visited.len() >= OVERRIDE_WALK_LIMIT {
                return Ok(true);
            }
            visited.push(ty.clone());
            if provided_from_json(vm, *head, ty).is_some() {
                return Ok(true);
            }
            let field_tys: Vec<RealizedTy> = match vm.get_object(head.ptr()) {
                Object::Class(class) => class
                    .fields
                    .iter()
                    .map(|field| vm.realize_field_ty(&field.field_template, type_args))
                    .collect(),
                _ => Vec::new(),
            };
            for field_ty in &field_tys {
                if reaches_from_json_override_at(vm, field_ty, visited)? {
                    return Ok(true);
                }
            }
            Ok(false)
        }
        RealizedTy::Enum(head) => Ok(provided_from_json(vm, *head, ty).is_some()),
        _ => Ok(false),
    }
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
        match decode_one(vm, json, ty, root, &[]) {
            Ok(Decoded::Value(value)) => self.resume(vm, value),
            Ok(Decoded::Open(container)) => match self.open(vm, container) {
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

    /// Enter `container`, the next child of the innermost open one. A JSON
    /// object or array the walk already has open is a value that contains
    /// itself: decoding it as a type would never end.
    fn open(&mut self, vm: &mut BexVm, container: Container) -> Result<(), VmRustFnError> {
        let source = container.source();
        if self.open.iter().any(|open| open.source() == source) {
            return Err(raise_decode_at(vm, CYCLIC_JSON, &self.open));
        }
        self.open.push(container);
        Ok(())
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
            let next = self
                .open
                .last()
                .unwrap_or_else(|| unreachable!("the walk runs inside an open container"))
                .next();
            match next {
                Some((json, ty)) => match decode_one(vm, json, &ty, Root::Dispatch, &self.open) {
                    Ok(Decoded::Value(value)) => self
                        .open
                        .last_mut()
                        .unwrap_or_else(|| unreachable!("the container was just read"))
                        .push(value),
                    Ok(Decoded::Open(child)) => {
                        if let Err(error) = self.open(vm, child) {
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

/// The path of the value the walk is on, from the root: each of the `open`
/// containers' segment for the child it is on, outermost first. The innermost
/// may have none left (it is about to finish); every outer one is on the
/// child that opened the next. Rendered only when an error names it.
fn path_at(vm: &BexVm, open: &[Container]) -> String {
    open.iter()
        .filter_map(|container| container.segment(vm))
        .collect()
}

/// A `DecodeError` at the value the walk is on inside the `open` containers.
fn raise_decode_at(
    vm: &mut BexVm,
    message: impl Into<String>,
    open: &[Container],
) -> VmRustFnError {
    let path = path_at(vm, open);
    raise_decode(vm, message, &path)
}

/// Decode `json` as `ty`, the value the walk is on inside the `open`
/// containers, one level: its value if it decodes in place, the container to
/// walk if it has children, or the `from_json` that builds it.
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
    open: &[Container],
) -> Result<Decoded, VmRustFnError> {
    match ty {
        RealizedTy::Union(members) if members.iter().any(RealizedTy::is_null) => {
            if json.is_null() {
                Ok(Decoded::Value(Value::NULL))
            } else {
                decode_one(vm, json, &ty.strip_null(), Root::Dispatch, open)
            }
        }
        RealizedTy::Union(members) => {
            // Membership by shape: a provided `from_json` that throws unwinds
            // the whole walk, so a member cannot be tried through its rule
            // and fallen through to the next. A member that does not match
            // is passed over without building its error; the one error is
            // raised once none does.
            check_json(vm, json, open)?;
            let member = match discriminated_member(vm, members, json) {
                Discriminated::Member(member) => json_matches(vm, json, member).then_some(member),
                Discriminated::NoMember => None,
                Discriminated::Undiscriminated => {
                    members.iter().find(|member| json_matches(vm, json, member))
                }
            };
            match member {
                Some(member) => decode_one(vm, json, &member.clone(), Root::Dispatch, open),
                None => Err(raise_decode_at(vm, "no union member matched", open)),
            }
        }
        RealizedTy::List(element) => open_list(vm, json, element, open).map(Decoded::Open),
        RealizedTy::Map { key, value } => open_map(vm, json, key, value, open).map(Decoded::Open),
        RealizedTy::Class(head, _) | RealizedTy::Interface(head, _, _)
            if vm.stdlib_heads().media_kind(*head).is_some() =>
        {
            decode_leaf(vm, json, ty, open).map(Decoded::Value)
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
                ) => open_class(vm, json, *head, type_args, open).map(Decoded::Open),
                (None, _) => decode_leaf(vm, json, ty, open).map(Decoded::Value),
            }
        }
        _ => decode_leaf(vm, json, ty, open).map(Decoded::Value),
    }
}

// ─── Structural decoding in place ────────────────────────────────────────────
//
// A leaf and a union candidate used to be converted whole to
// `serde_json::Value` and decoded through `ty_serde_to_value`, which builds a
// `DecodeError` — a heap object with a message and a path — for every member
// that fails. The walk below reads the VM `json` value in place instead:
// [`Tok`] is the JSON token `value_to_serde` would make of it, [`json_matches`]
// is `ty_serde_to_value(..).is_ok()` without the allocation, and
// [`decode_leaf`] builds the value or the one error. What the conversion
// refused — a value that contains itself, a map without string keys — is
// refused first by [`check_json`], at the same place with the same path. The
// types whose decoding IS the conversion (`json` and the opaque types, copied
// whole) and media (a tagged object) still go through serde.

/// Refuse what `value_to_serde` refuses in `json`: a JSON object or array
/// that contains itself, or a map without string keys, as a decode error at
/// the place the walk met it — the value's path, then the way down to it in
/// the `[i]` / `["key"]` notation of the untyped conversion.
fn check_json(vm: &mut BexVm, json: Value, open: &[Container]) -> Result<(), VmRustFnError> {
    let mut active = Vec::new();
    let mut inner = Vec::new();
    match check_json_at(vm, json, &mut active, &mut inner) {
        Ok(()) => Ok(()),
        Err(message) => {
            use std::fmt::Write;
            let mut path = path_at(vm, open);
            for segment in &inner {
                match segment {
                    Segment::Index(index) => {
                        let _ = write!(path, "[{index}]");
                    }
                    Segment::Key(key) => match key.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
                        Some(Object::String(key)) => {
                            let _ = write!(path, "[{key:?}]");
                        }
                        _ => unreachable!("the key was read as a string"),
                    },
                }
            }
            Err(raise_decode(vm, message, &path))
        }
    }
}

/// A step down from a JSON container: an array index or an object key (the
/// key's string value, read back when the path is rendered).
enum Segment {
    Index(usize),
    Key(Value),
}

/// [`check_json`]'s walk: `active` holds the containers it is inside, `inner`
/// the way down to `json`, left where it is on an error.
fn check_json_at(
    vm: &BexVm,
    json: Value,
    active: &mut Vec<HeapPtr>,
    inner: &mut Vec<Segment>,
) -> Result<(), &'static str> {
    let Some(ptr) = json.as_object_ptr() else {
        return Ok(());
    };
    match vm.get_object(ptr) {
        Object::Array(array) => {
            if active.contains(&ptr) {
                return Err(CYCLIC_JSON);
            }
            active.push(ptr);
            let elements = array.lock();
            for (index, element) in elements.iter().enumerate() {
                inner.push(Segment::Index(index));
                check_json_at(vm, *element, active, inner)?;
                inner.pop();
            }
            active.pop();
        }
        Object::Map(map) => {
            if active.contains(&ptr) {
                return Err(CYCLIC_JSON);
            }
            // Every key is checked before any value is walked, as the
            // conversion snapshots the entries.
            if !baml_type::normalize::is_string_key(map.key_ty.as_ty(), vm) {
                return Err(STRING_MAP_REQUIRED);
            }
            let entries = map.lock();
            if !entries.keys().all(|key| string_tok(vm, *key).is_some()) {
                return Err(STRING_MAP_REQUIRED);
            }
            active.push(ptr);
            for (key, value) in entries.iter() {
                inner.push(Segment::Key(*key));
                check_json_at(vm, *value, active, inner)?;
                inner.pop();
            }
            active.pop();
        }
        _ => {}
    }
    Ok(())
}

/// A VM `json` value as the JSON token `value_to_serde` makes of it, read in
/// place: a non-finite float and any object with no JSON representation are
/// `null`, an enum variant is its name.
enum Tok<'a> {
    Null,
    Bool(bool),
    Int(i64),
    Float(f64),
    Bigint(&'a num_bigint::BigInt),
    Str(&'a str),
    Array(&'a Array),
    Object(&'a Map),
}

fn tok(vm: &BexVm, json: Value) -> Tok<'_> {
    let ptr = match json.kind() {
        ValueKind::Null | ValueKind::OmittedArg => return Tok::Null,
        ValueKind::Bool(b) => return Tok::Bool(b),
        ValueKind::Int(i) => return Tok::Int(i),
        ValueKind::Object(ptr) => ptr,
    };
    match vm.get_object(ptr) {
        Object::Float(f) if f.is_finite() => Tok::Float(*f),
        Object::Float(_) => Tok::Null,
        Object::String(s) => Tok::Str(s.as_str()),
        Object::Bigint(b) => Tok::Bigint(b),
        Object::Array(array) => Tok::Array(array),
        Object::Map(map) => Tok::Object(map),
        Object::Variant(var) => match vm.get_object(var.enm) {
            Object::Enum(e) => e
                .variants
                .get(var.index)
                .map_or(Tok::Null, |variant| Tok::Str(variant.name.as_str())),
            _ => Tok::Null,
        },
        Object::Tombstone => Object::tombstone_reached(),
        _ => Tok::Null,
    }
}

/// `json` as a JSON string token, if it is one.
fn string_tok(vm: &BexVm, json: Value) -> Option<&str> {
    match tok(vm, json) {
        Tok::Str(s) => Some(s),
        _ => None,
    }
}

impl Tok<'_> {
    /// A JSON number token.
    fn is_number(&self) -> bool {
        matches!(self, Tok::Int(_) | Tok::Float(_) | Tok::Bigint(_))
    }

    /// `serde_json::Number::as_i64` of the token: the integral tokens that
    /// fit. A float's text (`1.0`, `1e21`) never parses as an integer.
    fn as_i64(&self) -> Option<i64> {
        match self {
            Tok::Int(i) => Some(*i),
            Tok::Bigint(b) => i64::try_from(*b).ok(),
            _ => None,
        }
    }

    /// `serde_json::Number::as_f64` of the token: the number tokens, read as
    /// a finite float.
    #[allow(clippy::cast_precision_loss)] // `i64 as f64` rounds as parsing its text does
    fn as_f64(&self) -> Option<f64> {
        match self {
            Tok::Int(i) => Some(*i as f64),
            Tok::Float(f) => Some(*f),
            Tok::Bigint(b) => b.to_string().parse::<f64>().ok().filter(|f| f.is_finite()),
            _ => None,
        }
    }

    /// [`parse_typed_json_bigint`] of the token: an integral number, or a
    /// string of decimal digits.
    fn typed_bigint(&self) -> Option<num_bigint::BigInt> {
        match self {
            Tok::Int(i) => Some((*i).into()),
            // Allocated under the bit limit, so its text always parses back.
            Tok::Bigint(b) => Some((*b).clone()),
            Tok::Str(s) => parse_decimal_bigint(s),
            _ => None,
        }
    }

    /// Whether [`Self::typed_bigint`] would find one, without building it.
    fn has_typed_bigint(&self) -> bool {
        match self {
            Tok::Int(_) | Tok::Bigint(_) => true,
            Tok::Str(s) => parse_decimal_bigint(s).is_some(),
            _ => false,
        }
    }
}

/// The value under `name` in the JSON object `entries`, if any.
fn object_get(vm: &BexVm, entries: &bex_vm_types::types::MapData, name: &str) -> Option<Value> {
    entries
        .iter()
        .find(|(key, _)| string_tok(vm, **key) == Some(name))
        .map(|(_, value)| *value)
}

/// The index of `name` among the variants of the enum at `head`, if `head`
/// is an enum that has it.
fn enum_variant_index(vm: &BexVm, head: bex_vm_types::TypeHead, name: &str) -> Option<usize> {
    match vm.get_object(head.ptr()) {
        Object::Enum(e) => e.variants.iter().position(|variant| variant.name == name),
        _ => None,
    }
}

/// Whether `json` decodes as `ty` structurally — whether `ty_serde_to_value`
/// succeeds on it — read in place, allocating nothing. The membership test a
/// union chooses its member by: a candidate that would fail is passed over
/// without its error being built. `json` has passed [`check_json`].
fn json_matches(vm: &BexVm, json: Value, ty: &RealizedTy) -> bool {
    let token = tok(vm, json);
    match ty {
        RealizedTy::Null => matches!(token, Tok::Null),
        RealizedTy::Bool => matches!(token, Tok::Bool(_)),
        RealizedTy::Int => token.as_i64().and_then(Value::try_int).is_some(),
        RealizedTy::Bigint => token.has_typed_bigint(),
        RealizedTy::Float => token.as_f64().is_some(),
        RealizedTy::String => matches!(token, Tok::Str(_)),
        RealizedTy::List(element) => match token {
            Tok::Array(array) => array
                .lock()
                .iter()
                .all(|item| json_matches(vm, *item, element)),
            _ => false,
        },
        RealizedTy::Map { key, .. } if !baml_type::normalize::is_string_key(key.as_ty(), vm) => {
            false
        }
        RealizedTy::Map { key, value } => match token {
            Tok::Object(map) => map.lock().iter().all(|(written, item)| {
                string_tok(vm, *written).is_some_and(|written| {
                    baml_type::normalize::is_subtype(
                        &baml_type::Ty::Literal(
                            baml_type::Literal::String(written.to_string()),
                            baml_type::Freshness::Regular,
                        ),
                        key.as_ty(),
                        vm,
                    )
                }) && json_matches(vm, *item, value)
            }),
            _ => false,
        },
        RealizedTy::TypeAlias(_) => true,
        RealizedTy::Class(head, type_args) => match vm.stdlib_heads().media_kind(*head) {
            Some(kind) => media_matches(vm, &token, kind),
            None => class_matches(vm, &token, *head, type_args),
        },
        RealizedTy::Interface(head, type_args, _) => class_matches(vm, &token, *head, type_args),
        RealizedTy::Enum(head) => match token {
            Tok::Str(s) => enum_variant_index(vm, *head, s).is_some(),
            _ => false,
        },
        RealizedTy::EnumVariant(head, name) => match token {
            Tok::Str(s) if s == name.as_str() => enum_variant_index(vm, *head, s).is_some(),
            _ => false,
        },
        RealizedTy::Media(kind) => {
            kind.wrapper_class()
                .and_then(|wrapper| vm.lang_head(wrapper))
                .is_some()
                && media_matches(vm, &token, *kind)
        }
        RealizedTy::Uint8Array => false,
        RealizedTy::Union(members) => members.iter().any(|member| json_matches(vm, json, member)),
        RealizedTy::Literal(lit, _) => literal_matches(lit, &token),
        RealizedTy::Resource | RealizedTy::PromptAst => false,
        RealizedTy::Function { .. } | RealizedTy::Future(_, _) | RealizedTy::Unknown => true,
        RealizedTy::Never | RealizedTy::RustType | RealizedTy::Type => false,
    }
}

/// [`json_matches`] for a literal type: the `RealizedTy::Literal` arm of
/// `ty_serde_to_value`, as a test.
fn literal_matches(lit: &baml_type::Literal, token: &Tok<'_>) -> bool {
    match (lit, token) {
        (baml_type::Literal::Bool(b), Tok::Bool(jb)) => b == jb,
        (baml_type::Literal::String(s), Tok::Str(js)) => s == js,
        (baml_type::Literal::Int(expected), token) if token.is_number() => {
            token.as_i64() == Some(*expected)
        }
        (baml_type::Literal::Float(s), token) if token.is_number() => {
            matches!((s.parse::<f64>(), token.as_f64()), (Ok(expected), Some(actual)) if (expected - actual).abs() < f64::EPSILON)
        }
        (baml_type::Literal::Bigint(expected), token) => {
            token.typed_bigint().as_ref() == Some(expected)
        }
        _ => false,
    }
}

/// [`json_matches`] for the class or interface at `head`: a JSON object with
/// every declared field decoding as its type, an absent optional field being
/// `null`.
fn class_matches(
    vm: &BexVm,
    token: &Tok<'_>,
    head: bex_vm_types::TypeHead,
    type_args: &[RealizedTy],
) -> bool {
    let Tok::Object(map) = token else {
        return false;
    };
    let Object::Class(class) = vm.get_object(head.ptr()) else {
        return false;
    };
    let entries = map.lock();
    class.fields.iter().all(|field| {
        let field_ty = vm.realize_field_ty(&field.field_template, type_args);
        match object_get(vm, &entries, &field.name) {
            Some(value) => json_matches(vm, value, &field_ty),
            // `null` decodes as a nullable union through its `null` member.
            None => field_ty.is_nullable_union(),
        }
    })
}

/// [`json_matches`] for media of `kind`: the tagged object `deserialize_media`
/// reads — `kind` naming it, a `source` it knows, and a string `value`.
fn media_matches(vm: &BexVm, token: &Tok<'_>, kind: MediaKind) -> bool {
    let Tok::Object(map) = token else {
        return false;
    };
    let entries = map.lock();
    let string_field = |name: &str| object_get(vm, &entries, name).and_then(|v| string_tok(vm, v));
    string_field("kind") == Some(kind.tag_str())
        && matches!(string_field("source"), Some("url" | "base64" | "inline"))
        && string_field("value").is_some()
}

/// Whether `ty` is a class decoded by its fields: not media, and declared.
fn plain_class(vm: &BexVm, ty: &RealizedTy) -> Option<(bex_vm_types::TypeHead, Box<[RealizedTy]>)> {
    match ty {
        RealizedTy::Class(head, type_args)
            if vm.stdlib_heads().media_kind(*head).is_none()
                && matches!(vm.get_object(head.ptr()), Object::Class(_)) =>
        {
            Some((*head, type_args.clone()))
        }
        _ => None,
    }
}

/// The string literal the class at `head` declares field `name` as, if it
/// does.
fn string_literal_field(
    vm: &BexVm,
    head: bex_vm_types::TypeHead,
    type_args: &[RealizedTy],
    name: &str,
) -> Option<String> {
    let Object::Class(class) = vm.get_object(head.ptr()) else {
        return None;
    };
    let field = class.fields.iter().find(|field| field.name == name)?;
    match vm.realize_field_ty(&field.field_template, type_args) {
        RealizedTy::Literal(baml_type::Literal::String(value), _) => Some(value),
        _ => None,
    }
}

/// The member of a union of classes that the JSON object `json` names by
/// its discriminant, when the union has one: a field every member declares
/// as a string literal of its own (`tool: "search"` | `tool: "calc"` |
/// `tool: "finish"`). Only the member whose literal is written can match, so
/// it is the one probing in order would find, and no member — the written
/// value is no member's, or absent — is what probing would find too. A union
/// not of this shape, or `json` not an object, is undiscriminated: the
/// members are probed in order.
fn discriminated_member<'m>(
    vm: &BexVm,
    members: &'m [RealizedTy],
    json: Value,
) -> Discriminated<'m> {
    let Tok::Object(map) = tok(vm, json) else {
        return Discriminated::Undiscriminated;
    };
    let Some((first_head, first_args)) = members.first().and_then(|first| plain_class(vm, first))
    else {
        return Discriminated::Undiscriminated;
    };
    let Object::Class(first) = vm.get_object(first_head.ptr()) else {
        return Discriminated::Undiscriminated;
    };
    'fields: for field in &first.fields {
        let Some(first_value) = string_literal_field(vm, first_head, &first_args, &field.name)
        else {
            continue;
        };
        let mut values = vec![first_value];
        for member in &members[1..] {
            let Some((head, type_args)) = plain_class(vm, member) else {
                return Discriminated::Undiscriminated;
            };
            let Some(value) = string_literal_field(vm, head, &type_args, &field.name) else {
                continue 'fields;
            };
            if values.contains(&value) {
                continue 'fields;
            }
            values.push(value);
        }
        let entries = map.lock();
        let written = object_get(vm, &entries, &field.name).and_then(|v| string_tok(vm, v));
        return match written.and_then(|written| values.iter().position(|value| value == written)) {
            Some(index) => Discriminated::Member(&members[index]),
            None => Discriminated::NoMember,
        };
    }
    Discriminated::Undiscriminated
}

/// What a union's discriminant says about the member a JSON object is
/// ([`discriminated_member`]).
enum Discriminated<'m> {
    /// The member whose literal is written.
    Member(&'m RealizedTy),
    /// The written value is no member's, or the field is absent.
    NoMember,
    /// The union has no discriminant, or the value is not an object.
    Undiscriminated,
}

/// The VM `json` value the walk is on as `serde_json::Value`, for the leaf
/// types decoded through serde: a value that contains itself is a decode
/// error at the place the walk met it again.
fn leaf_serde(
    vm: &mut BexVm,
    json: Value,
    open: &[Container],
) -> Result<serde_json::Value, VmRustFnError> {
    value_to_serde(vm, json).map_err(|e| {
        let path = format!("{}{}", path_at(vm, open), e.path);
        raise_decode(vm, e.message, &path)
    })
}

/// Decode `json`, the value the walk is on, as `ty` structurally, in place.
/// For the types that carry no `baml.FromJson` rule of their own.
fn decode_leaf(
    vm: &mut BexVm,
    json: Value,
    ty: &RealizedTy,
    open: &[Container],
) -> Result<Value, VmRustFnError> {
    match ty {
        RealizedTy::Null
        | RealizedTy::Bool
        | RealizedTy::Int
        | RealizedTy::Bigint
        | RealizedTy::Float
        | RealizedTy::String
        | RealizedTy::Enum(_)
        | RealizedTy::EnumVariant(_, _)
        | RealizedTy::Literal(_, _)
        | RealizedTy::Uint8Array
        | RealizedTy::Resource
        | RealizedTy::PromptAst
        | RealizedTy::Never
        | RealizedTy::RustType
        | RealizedTy::Type => {
            check_json(vm, json, open)?;
            let built = scalar_leaf(vm, json, ty);
            match built {
                Ok(Leaf::Value(value)) => Ok(value),
                Ok(Leaf::Float(f)) => Ok(Value::object(vm.alloc_float(f))),
                Ok(Leaf::Str(s)) => Ok(Value::object(vm.alloc_string(s))),
                Ok(Leaf::Bigint(b)) => vm.try_alloc_bigint(Arc::new(b)).map_err(Into::into),
                Ok(Leaf::Variant(enm, index)) => Ok(Value::object(vm.alloc_variant(enm, index))),
                Err(message) => Err(raise_decode_at(vm, message, open)),
            }
        }
        // `json` and the opaque types are the value copied whole, as the
        // conversion copies it; media is read from its tagged object. The
        // containers never reach here: they open.
        RealizedTy::TypeAlias(_)
        | RealizedTy::Function { .. }
        | RealizedTy::Future(_, _)
        | RealizedTy::Unknown
        | RealizedTy::Media(_)
        | RealizedTy::Class(_, _)
        | RealizedTy::Interface(_, _, _)
        | RealizedTy::List(_)
        | RealizedTy::Map { .. }
        | RealizedTy::Union(_) => {
            let serde = leaf_serde(vm, json, open)?;
            let mut path = path_at(vm, open);
            ty_serde_to_value(vm, &serde, ty, &mut path)
        }
    }
}

/// What a scalar leaf decodes to, before it is allocated.
enum Leaf {
    Value(Value),
    Float(f64),
    Str(bex_vm_types::BexStr),
    Bigint(num_bigint::BigInt),
    Variant(HeapPtr, usize),
}

/// The string `json` is, to allocate as a VM string: a string object's own
/// text is shared, anything else (an enum variant's name) is copied.
fn shared_str(vm: &BexVm, json: Value, text: &str) -> bex_vm_types::BexStr {
    match json.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::String(s)) => s.clone(),
        _ => bex_vm_types::BexStr::from(text),
    }
}

/// The scalar arms of `ty_serde_to_value`, read from the VM `json` value in
/// place: the leaf to allocate, or the error's message.
fn scalar_leaf(vm: &BexVm, json: Value, ty: &RealizedTy) -> Result<Leaf, String> {
    let token = tok(vm, json);
    match ty {
        RealizedTy::Null => match token {
            Tok::Null => Ok(Leaf::Value(Value::NULL)),
            _ => Err("expected null".to_string()),
        },
        RealizedTy::Bool => match token {
            Tok::Bool(b) => Ok(Leaf::Value(Value::bool(b))),
            _ => Err("expected boolean".to_string()),
        },
        RealizedTy::Int if token.is_number() => token
            .as_i64()
            .and_then(Value::try_int)
            .map(Leaf::Value)
            .ok_or_else(|| "expected integer in the BAML int range [-2^62, 2^62 - 1]".to_string()),
        RealizedTy::Int => Err("expected integer".to_string()),
        RealizedTy::Bigint => token
            .typed_bigint()
            .map(Leaf::Bigint)
            .ok_or_else(|| "expected integral JSON number or numeric string".to_string()),
        RealizedTy::Float => token
            .as_f64()
            .map(Leaf::Float)
            .ok_or_else(|| "expected number".to_string()),
        RealizedTy::String => match token {
            Tok::Str(s) => Ok(Leaf::Str(shared_str(vm, json, s))),
            _ => Err("expected string".to_string()),
        },
        RealizedTy::Enum(head) => match token {
            Tok::Str(s) => enum_variant_leaf(vm, *head, s),
            _ => Err("expected enum variant string".to_string()),
        },
        RealizedTy::EnumVariant(head, name) => match token {
            Tok::Str(s) if s == name.as_str() => enum_variant_leaf(vm, *head, s),
            _ => Err(format!("expected enum variant `{name}`")),
        },
        RealizedTy::Literal(lit, _) => match (lit, token) {
            (baml_type::Literal::Bool(b), Tok::Bool(jb)) if *b == jb => {
                Ok(Leaf::Value(Value::bool(jb)))
            }
            (baml_type::Literal::String(s), Tok::Str(js)) if s == js => {
                Ok(Leaf::Str(shared_str(vm, json, js)))
            }
            (baml_type::Literal::Int(expected), token) if token.is_number() => {
                match token.as_i64() {
                    Some(actual) if *expected == actual => Ok(Leaf::Value(Value::int(actual))),
                    _ => Err("literal int mismatch".to_string()),
                }
            }
            (baml_type::Literal::Float(s), token) if token.is_number() => {
                match (s.parse::<f64>(), token.as_f64()) {
                    (Ok(expected), Some(actual)) if (expected - actual).abs() < f64::EPSILON => {
                        Ok(Leaf::Float(actual))
                    }
                    _ => Err("literal float mismatch".to_string()),
                }
            }
            (baml_type::Literal::Bigint(expected), token) => {
                let actual = token
                    .typed_bigint()
                    .ok_or_else(|| "expected integral JSON number or numeric string".to_string())?;
                if &actual != expected {
                    return Err("literal bigint mismatch".to_string());
                }
                Ok(Leaf::Bigint(actual))
            }
            _ => Err("literal mismatch".to_string()),
        },
        RealizedTy::Uint8Array => Err(
            "uint8array requires explicit encoding (use from_base64() or from_hex())".to_string(),
        ),
        RealizedTy::Resource | RealizedTy::PromptAst => {
            Err("cannot deserialize opaque type".to_string())
        }
        RealizedTy::Never | RealizedTy::RustType | RealizedTy::Type => {
            Err("cannot decode compiler-only type".to_string())
        }
        RealizedTy::TypeAlias(_)
        | RealizedTy::Function { .. }
        | RealizedTy::Future(_, _)
        | RealizedTy::Unknown
        | RealizedTy::Media(_)
        | RealizedTy::Class(_, _)
        | RealizedTy::Interface(_, _, _)
        | RealizedTy::List(_)
        | RealizedTy::Map { .. }
        | RealizedTy::Union(_) => unreachable!("`decode_leaf` decodes these through serde"),
    }
}

/// The variant `name` of the enum at `head`, with `deserialize_enum_variant`'s
/// errors.
fn enum_variant_leaf(vm: &BexVm, head: bex_vm_types::TypeHead, name: &str) -> Result<Leaf, String> {
    let Object::Enum(e) = vm.get_object(head.ptr()) else {
        let named = baml_type::HeadDisplay::head_display_name(&head);
        return Err(format!("`{named}` is not an enum"));
    };
    match e.variants.iter().position(|variant| variant.name == name) {
        Some(index) => Ok(Leaf::Variant(head.ptr(), index)),
        None => {
            let named = baml_type::HeadDisplay::head_display_name(&head);
            Err(format!("unknown variant `{name}` for enum `{named}`"))
        }
    }
}

/// The entries of the JSON object `json`, the value the walk is on: a
/// string-keyed map and nothing else — `expected` names what was wanted when it is not an object.
fn json_object_entries(
    vm: &mut BexVm,
    json: Value,
    expected: impl FnOnce() -> String,
    open: &[Container],
) -> Result<Vec<(bex_vm_types::BexStr, Value)>, VmRustFnError> {
    let entries = match json.as_object_ptr().map(|ptr| vm.get_object(ptr)) {
        Some(Object::Map(map)) => string_map_entries(vm, map).map_err(str::to_owned),
        Some(_) | None => Err(expected()),
    };
    entries.map_err(|message| raise_decode_at(vm, message, open))
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
    open: &[Container],
) -> Result<Container, VmRustFnError> {
    let named = baml_type::HeadDisplay::head_display_name(&head);
    let object: IndexMap<bex_vm_types::BexStr, Value> = json_object_entries(
        vm,
        json,
        || format!("expected JSON object for class `{named}`"),
        open,
    )?
    .into_iter()
    .collect();
    let source = json
        .as_object_ptr()
        .unwrap_or_else(|| unreachable!("the entries were read from a JSON object"));
    let class = head.ptr();
    let class_fields = match vm.get_object(class) {
        Object::Class(class) => class.fields.clone(),
        _ => {
            return Err(raise_decode_at(
                vm,
                format!("`{named}` is not a class"),
                open,
            ));
        }
    };
    let mut fields: Vec<(Value, RealizedTy)> = Vec::with_capacity(class_fields.len());
    for field in &class_fields {
        let field_ty = vm.realize_field_ty(&field.field_template, type_args);
        let field_json = match object.get(field.name.as_str()) {
            Some(value) => *value,
            // Optional (`T?` == `T | null`) fields may be absent → null.
            None if field_ty.is_nullable_union() => Value::NULL,
            None => {
                return Err(raise_decode_at(
                    vm,
                    format!("missing required field `{}`", field.name),
                    open,
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
    open: &[Container],
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
        Some(_) | None => Err(raise_decode_at(vm, "expected JSON array", open)),
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
    open: &[Container],
) -> Result<Container, VmRustFnError> {
    if !baml_type::normalize::is_string_key(key.as_ty(), vm) {
        return Err(raise_decode_at(vm, STRING_MAP_REQUIRED, open));
    }
    let entries = json_object_entries(vm, json, || "expected JSON object".to_string(), open)?;
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
            return Err(raise_decode_at(
                vm,
                "map key does not match declared key type",
                open,
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
