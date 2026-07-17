use std::{collections::HashMap, sync::Arc};

use bex_heap::TlabHolder;
use bex_vm_types::{
    FutureRead, HeapPtr, ObjectType, ValueKind,
    types::{Array, AtomicValueSlot, Instance, Map, Object, Value},
};
use indexmap::IndexMap;

use super::{
    BamlPackageBaml, Continuation, NativeCallResult, PackageBamlImpl, PassThroughContinuation,
    array::{
        NaturalDomain, compare_natural_values, is_primitive_array_values,
        validate_natural_order_with_vm,
    },
    make_compare_callee, make_to_string_callee,
};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmRustFnError},
};

impl BamlPackageBaml for PackageBamlImpl {
    fn deep_copy(vm: &mut BexVm, value: &Value) -> Value {
        let mut copied_objects = HashMap::new();
        deep_copy_value_recursive(vm, *value, &mut copied_objects)
    }

    fn deep_equals(vm: &BexVm, a: &Value, b: &Value) -> bool {
        let mut visited = HashMap::new();
        deep_equals_recursive(vm, *a, *b, &mut visited)
    }

    /// `baml._float_total_cmp(a, b)` — bit-exact `f64::total_cmp` three-way
    /// comparison backing `Comparable for float`. Kept in lockstep with the
    /// float domain of `compare_natural_values` (the `_rust_sort` fast path)
    /// so the two sort paths can never disagree on a float ordering.
    fn _float_total_cmp(a: f64, b: f64) -> i64 {
        match a.total_cmp(&b) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        }
    }

    /// `baml._is_primitive_array(arr)` — `Sortable.sort`'s dispatch guard:
    /// whether the first element's runtime tag is a natural-sort primitive
    /// (int/bigint/string/float). Empty arrays report `true`.
    fn _is_primitive_array(vm: &BexVm, arr: &[Value]) -> bool {
        is_primitive_array_values(vm, arr)
    }

    /// `baml._rust_sort(arr)` — the primitive fast path of `Sortable.sort`.
    /// Stable natural-order sort of a homogeneous primitive array, in place
    /// (the receiver's backing `Vec` is sorted or replaced; the returned value
    /// IS the receiver). The comparator is pure Rust — no per-pair yield to
    /// BAML — and the float domain uses `f64::total_cmp`, so no domain throws.
    /// The validation rejections are defensive only: the `_is_primitive_array`
    /// guard plus `T[]` homogeneity make them unreachable from `Sortable.sort`.
    fn _rust_sort(vm: &mut BexVm, arr: &Value) -> NativeCallResult {
        let domain = {
            let values = match vm.as_array(arr) {
                Ok(guard) => guard,
                Err(e) => return NativeCallResult::Error(e.into()),
            };
            if values.len() <= 1 {
                return NativeCallResult::Done(*arr);
            }
            match validate_natural_order_with_vm(vm, "sort", &values) {
                Ok(domain) => domain,
                Err(e) => return NativeCallResult::Error(e),
            }
        };
        if matches!(domain, NaturalDomain::Int) {
            // Int values are tag-only (no heap reads), so the comparator
            // needs no `&BexVm` and the backing Vec sorts truly in place.
            let mut values = match vm.as_array_mut(arr) {
                Ok(guard) => guard,
                Err(e) => return NativeCallResult::Error(e.into()),
            };
            values.sort_by(|left, right| {
                left.as_int()
                    .expect("validated int sort value should be int")
                    .cmp(
                        &right
                            .as_int()
                            .expect("validated int sort value should be int"),
                    )
            });
            return NativeCallResult::Done(*arr);
        }
        // Heap-read domains (float/bigint/string): the comparator needs
        // `&BexVm`, which conflicts with holding the array's mutable guard —
        // sort a snapshot of the backing Vec, then swap it back in. One Vec
        // (re)allocation; still no per-element BAML round trips or re-push.
        let mut values = match vm.as_array(arr) {
            Ok(guard) => guard.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        values.sort_by(|left, right| compare_natural_values(vm, domain, *left, *right));
        match vm.as_array_mut(arr) {
            Ok(mut guard) => *guard = values,
            Err(e) => return NativeCallResult::Error(e.into()),
        }
        NativeCallResult::Done(*arr)
    }

    /// `baml._compare_shim(a, b)` — the dispatch shim for the `Sortable`
    /// blanket `sort`'s comparator path. Resolves `Comparable.compare` on
    /// `a`'s runtime class and yields to it with `b`; the comparison's `int`
    /// result (or thrown error) is returned straight through. See
    /// `make_compare_callee` for why the sort cannot dispatch `compare`
    /// itself.
    fn _compare_shim(vm: &mut BexVm, a: &Value, b: &Value) -> NativeCallResult {
        let callee = match make_compare_callee(vm, *a) {
            Ok(ptr) => ptr,
            Err(e) => return NativeCallResult::Error(e),
        };
        NativeCallResult::YieldToCall {
            callee,
            args: vec![*b],
            type_args: vec![],
            continuation: Box::new(PassThroughContinuation),
        }
    }

    /// `baml._to_string_default(value)` and `baml._to_string_shim(value)` both
    /// render `value` for `string.from`, honoring `baml.ToString` overrides at
    /// every depth: `value`'s own override (if any) wins, and any *nested* value
    /// whose runtime class overrides `to_string` is rendered via that override
    /// rather than structurally. Everything else renders structurally (primitives
    /// naturally; containers/instances as `[a, b]` / `Class { f: v }`, with nested
    /// strings quoted). Total — `string.from` is `throws never`.
    ///
    /// `_to_string_default` is the `baml.ToString` interface's default body and
    /// the structural fallback; `_to_string_shim` backs `string.from`. They are
    /// identical: the walker already applies `value`'s own override when present,
    /// so an empty `implements baml.ToString {}` (whose runtime class carries no
    /// in-body override) still serializes structurally with nested overrides
    /// honored.
    fn _to_string_default(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        render_to_string_honoring_overrides(vm, *value)
    }

    fn _to_string_shim(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        render_to_string_honoring_overrides(vm, *value)
    }

    /// `baml._to_json_default(value)` and `baml._to_json_shim(value)` both render
    /// `value` to a `json` value for `baml.json.from`, honoring `baml.ToJson`
    /// overrides at every depth. The json analog of `_to_string_default` /
    /// `_to_string_shim`; both delegate to the override-honoring walker in
    /// `json.rs`. Unlike the string shims, this can throw `JsonSerializationError`
    /// for values with no json representation.
    fn _to_json_default(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        super::json::render_to_json_honoring_overrides(vm, *value)
    }

    fn _to_json_shim(vm: &mut BexVm, value: &Value) -> NativeCallResult {
        super::json::render_to_json_honoring_overrides(vm, *value)
    }

    /// `baml._from_json_shim<T>(j)` backs `baml.json.to<T>`: decode `j` into the
    /// target type `T` (read from the call's type-args), dispatching a user
    /// `implements baml.FromJson` override on `T` and otherwise decoding
    /// structurally. The deserialize analog of `_to_json_shim`.
    fn _from_json_shim(vm: &mut BexVm, j: &Value) -> NativeCallResult {
        super::json::json_to_shim(vm, *j)
    }

    /// `baml._cleanup_begin(value)` — BEP-042 `cleanup` run-once guard.
    ///
    /// Atomically test-and-sets `value`'s per-instance "cleaned" latch and
    /// returns `true` iff this is the first `cleanup` invocation on that
    /// instance (so the compiled guard runs the body); a later invocation —
    /// explicit, `defer`, or (Commit 2) the GC finalizer — returns `false` and
    /// the body is skipped. The latch is set on entry: a `cleanup` that throws
    /// is still considered cleaned and will not be retried.
    ///
    /// A non-instance receiver returns `true` defensively; the guard is only
    /// emitted for a class `cleanup(self)` method, whose `self` is an instance.
    fn _cleanup_begin(vm: &BexVm, value: &Value) -> bool {
        match value.as_object_ptr() {
            Some(ptr) => match vm.get_object(ptr) {
                Object::Instance(inst) => inst.cleaned.begin(),
                _ => true,
            },
            None => true,
        }
    }

    // ── Numeric-array reductions (formerly `baml.math.*`) ──────────────────────
    //
    // Private native backings for the `Summable` / `FloatStats` methods declared
    // in `containers.baml`. `expect_int` / `expect_float` are infallible reads:
    // the type system proves each element's tag before execution reaches here.

    /// `baml._sum_int(values)` — native backing for `int[].sum()`.
    ///
    /// Accumulates left-to-right from `0`, checking the running total against
    /// the `int` range at each step so overflow raises `IntegerOverflow` exactly
    /// like repeated `+` would. Both `acc` and each element are already in range,
    /// so the intermediate i64 add never wraps; the tighter range check is the
    /// meaningful bound. The empty array sums to `0`.
    fn _sum_int(values: &[Value]) -> Result<i64, VmRustFnError> {
        let mut acc: i64 = 0;
        for (index, value) in values.iter().enumerate() {
            let x = expect_int(*value, "_sum_int", index);
            match acc.checked_add(x) {
                Some(v) if (Value::INT_MIN..=Value::INT_MAX).contains(&v) => acc = v,
                _ => {
                    return Err(VmPanic::IntegerOverflow {
                        message: format!("{acc} + {x} overflows int"),
                    }
                    .into());
                }
            }
        }
        Ok(acc)
    }

    /// `baml._sum_float(values)` — native backing for `float[].sum()`.
    ///
    /// Sums left-to-right from `0.0`; the empty array sums to `0.0`. Never throws.
    fn _sum_float(vm: &BexVm, values: &[Value]) -> f64 {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| expect_float(vm, *value, "_sum_float", index))
            .sum()
    }

    /// `baml._mean_float(values)` — native backing for `float[].mean()`.
    ///
    /// Throws `InvalidArgument` when `values` is empty.
    #[allow(clippy::cast_precision_loss)]
    fn _mean_float(vm: &BexVm, values: &[Value]) -> Result<f64, VmRustFnError> {
        if values.is_empty() {
            return Err(VmBamlError::InvalidArgument {
                message: "float[].mean: cannot take the mean of an empty array".to_string(),
            }
            .into());
        }
        let n = values.len() as f64;
        Ok(Self::_sum_float(vm, values) / n)
    }

    /// `baml._median_float(values)` — native backing for `float[].median()`.
    ///
    /// Sorts a copy with `f64::total_cmp` (BAML's total float ordering, matching
    /// `float[].sort()`) so the caller's array is left untouched. Throws
    /// `InvalidArgument` when `values` is empty.
    fn _median_float(vm: &BexVm, values: &[Value]) -> Result<f64, VmRustFnError> {
        if values.is_empty() {
            return Err(VmBamlError::InvalidArgument {
                message: "float[].median: cannot take the median of an empty array".to_string(),
            }
            .into());
        }
        let mut sorted: Vec<f64> = values
            .iter()
            .enumerate()
            .map(|(index, value)| expect_float(vm, *value, "_median_float", index))
            .collect();
        sorted.sort_by(f64::total_cmp);
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            Ok(sorted[mid])
        } else {
            Ok(f64::midpoint(sorted[mid - 1], sorted[mid]))
        }
    }

    /// `baml._trunc_to_int(value)` — saturating truncation toward zero (formerly
    /// the public `baml.math.trunc`). Rust's `as` cast saturates to the `i64`
    /// range and maps NaN to `0`; never throws.
    #[allow(clippy::cast_possible_truncation)]
    fn _trunc_to_int(value: f64) -> i64 {
        value as i64
    }
}

/// Whether `value`'s runtime class carries an in-body `baml.ToString` override.
/// Shares `make_to_string_callee`'s resolution (so the two agree on every value
/// kind, including the non-instance `type` / `uint8array` implementors) but
/// allocates nothing on the VM heap, so it is safe to call during the
/// allocation-free pre-order collection pass.
fn has_to_string_override(vm: &BexVm, value: Value) -> bool {
    super::to_string_override_fn_name(vm, value)
        .and_then(|name| vm.find_function_by_name(&name))
        .is_some()
}

/// Pre-order DFS collecting, by heap pointer and in render order, every
/// sub-value of `value` whose runtime class overrides `baml.ToString`. An
/// override node is recorded and *not* descended into — its `to_string` owns its
/// whole subtree. Immutable and allocation-free so the garbage collector cannot
/// move objects mid-walk. Matches the traversal order of [`render_to_string`] so
/// the two stay index-aligned. (Like the structural renderer, this does not
/// guard against reference cycles — recursive *data* would already loop in the
/// pre-existing walker; recursive *types* such as trees are acyclic.)
fn collect_to_string_overrides(vm: &BexVm, value: Value, out: &mut Vec<HeapPtr>) {
    let ValueKind::Object(ptr) = value.kind() else {
        return;
    };
    if has_to_string_override(vm, value) {
        out.push(ptr);
        return;
    }
    // Snapshot children (owned), dropping the heap borrow / container lock before
    // recursing — same discipline as `render_to_string`'s `DisplaySnap`. The
    // child order (array elements, then map values, then instance fields) matches
    // the renderer so the two stay index-aligned.
    let children: Vec<Value> = match vm.get_object(ptr) {
        Object::Array(values) => values.to_vec(),
        Object::Map(map) => map.to_index_map().values().copied().collect(),
        Object::Instance(inst) => inst.fields.iter().map(AtomicValueSlot::load).collect(),
        _ => Vec::new(),
    };
    for v in children {
        collect_to_string_overrides(vm, v, out);
    }
}

/// Entry point shared by `_to_string_default` and `_to_string_shim`. Collects the
/// override-bearing sub-values (pass 1, sync), dispatches `to_string` on each in
/// order (pass 2, one `YieldToCall` per override via [`ToStringWalkContinuation`]),
/// then renders structurally splicing in the override results (pass 3). When the
/// value tree contains no overrides at all, renders fully structurally inline.
fn render_to_string_honoring_overrides(vm: &mut BexVm, value: Value) -> NativeCallResult {
    let mut pending: Vec<HeapPtr> = Vec::new();
    collect_to_string_overrides(vm, value, &mut pending);

    let Some(&first_ptr) = pending.first() else {
        return render_done(vm, value, &pending, &[]);
    };
    match make_to_string_callee(vm, Value::object(first_ptr)) {
        Some(callee) => NativeCallResult::YieldToCall {
            callee,
            args: vec![],
            type_args: vec![],
            continuation: Box::new(ToStringWalkContinuation {
                root: value,
                pending,
                results: Vec::new(),
            }),
        },
        None => render_done(vm, value, &pending, &[]),
    }
}

/// Pass 3: render `root` structurally, splicing the precomputed override
/// `results` in by their pre-order position in `pending`, and wrap as `Done`.
fn render_done(
    vm: &mut BexVm,
    root: Value,
    pending: &[HeapPtr],
    results: &[String],
) -> NativeCallResult {
    let mut counter = 0;
    let rendered = render_to_string(vm, root, false, pending, results, &mut counter);
    NativeCallResult::Done(Value::object(vm.alloc_string(rendered)))
}

/// Drives pass 2/3 of `render_to_string_honoring_overrides`: accumulates each
/// override's `to_string` result, dispatches the next, and on completion renders
/// the structural skeleton with the override results spliced in. The number of
/// results gathered so far IS the index of the next override to dispatch.
struct ToStringWalkContinuation {
    /// The value being rendered (its structural skeleton is walked in pass 3).
    root: Value,
    /// Override-bearing sub-values, in render order (pass-1 output).
    pending: Vec<HeapPtr>,
    /// Override results so far, as plain Rust strings (no heap roots to track).
    results: Vec<String>,
}

impl Continuation for ToStringWalkContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(
            vm.as_string(&value)
                .map(|s| s.as_str().to_string())
                .unwrap_or_default(),
        );

        // Dispatch the next override, if any (and resolvable); otherwise render.
        if let Some(&next_ptr) = self.pending.get(self.results.len())
            && let Some(callee) = make_to_string_callee(vm, Value::object(next_ptr))
        {
            return NativeCallResult::YieldToCall {
                callee,
                args: vec![],
                type_args: vec![],
                continuation: self,
            };
        }
        render_done(vm, self.root, &self.pending, &self.results)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = self.pending.clone();
        if let Some(ptr) = self.root.as_object_ptr() {
            roots.push(ptr);
        }
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        if let Some(ptr) = self.root.as_object_ptr() {
            if let Some(&new_ptr) = forwarding.get(&ptr) {
                self.root = Value::object(new_ptr);
            }
        }
        for ptr in &mut self.pending {
            if let Some(&new_ptr) = forwarding.get(ptr) {
                *ptr = new_ptr;
            }
        }
    }
}

/// Owned snapshot of a heap object, captured so the recursive walker never
/// holds a heap borrow or a container lock across a recursive call (mirrors the
/// snapshot-before-recurse discipline in `deep_equals_recursive`).
enum DisplaySnap {
    /// A finished leaf rendering (`5.0`, `null`, an enum variant name, ...).
    Leaf(String),
    /// A string's contents — quoted when nested, bare at top level.
    Str(String),
    /// An array (or `uint8array`) — elements rendered as `[a, b, c]`.
    Seq(Vec<Value>),
    /// A map — entries rendered as `{"k": v, ...}` (keys are always strings, and
    /// are quoted so keys containing `:`/`,` stay unambiguous).
    Entries(Vec<(String, Value)>),
    /// A class instance — `ClassName { field: value, ... }`.
    Instance(String, Vec<(String, Value)>),
}

/// Human-readable rendering used by `string.from` / the `baml.ToString` default.
/// Structural and total: every value type renders to *something* and nothing
/// throws. A node whose runtime class overrides `baml.ToString` (recorded
/// pre-order in `pending` by [`collect_to_string_overrides`]) is rendered via its
/// precomputed result (`results[*counter]`, produced by pass 2), spliced in bare
/// regardless of nesting. Because collect and render share the same pre-order,
/// `pending[*counter]` is exactly the next override node — so the check is a
/// pointer compare, not a per-node global lookup. With an empty `pending` this is
/// a pure structural walk.
fn render_to_string(
    vm: &BexVm,
    value: Value,
    nested: bool,
    pending: &[HeapPtr],
    results: &[String],
    counter: &mut usize,
) -> String {
    let ptr = match value.kind() {
        ValueKind::Null => return "null".to_string(),
        ValueKind::Int(i) => return i.to_string(),
        ValueKind::Bool(b) => return b.to_string(),
        ValueKind::OmittedArg => return String::new(),
        ValueKind::Object(ptr) => ptr,
    };

    // Override node: splice its precomputed `to_string` result in bare.
    if pending.get(*counter) == Some(&ptr) {
        let rendered = results.get(*counter).cloned().unwrap_or_default();
        *counter += 1;
        return rendered;
    }

    // Capture an owned snapshot, dropping the heap borrow / container lock
    // before recursing.
    let snap = match vm.get_object(ptr) {
        Object::String(s) => DisplaySnap::Str(s.as_str().to_string()),
        Object::Float(f) => DisplaySnap::Leaf(bex_vm_types::format_float(*f)),
        Object::Bigint(b) => DisplaySnap::Leaf(b.to_string()),
        Object::Array(values) => DisplaySnap::Seq(values.to_vec()),
        Object::Uint8Array(bytes) => {
            let rendered = bytes
                .to_vec()
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            DisplaySnap::Leaf(format!("[{rendered}]"))
        }
        Object::Map(map) => DisplaySnap::Entries(
            map.to_index_map()
                .into_iter()
                .map(|(k, v)| (k.as_str().to_string(), v))
                .collect(),
        ),
        Object::Instance(inst) => {
            let field_values: Vec<Value> = inst.fields.iter().map(AtomicValueSlot::load).collect();
            let (class_name, field_names) = match vm.get_object(inst.class) {
                Object::Class(class) => (
                    class.name.name().to_string(),
                    class
                        .fields
                        .iter()
                        .map(|f| f.name.clone())
                        .collect::<Vec<_>>(),
                ),
                _ => (String::new(), Vec::new()),
            };
            let paired = field_values
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let name = field_names.get(i).cloned().unwrap_or_else(|| i.to_string());
                    (name, v)
                })
                .collect();
            DisplaySnap::Instance(class_name, paired)
        }
        Object::Variant(var) => {
            let name = match vm.get_object(var.enm) {
                Object::Enum(enm) => enm
                    .variants
                    .get(var.index)
                    .map(|v| v.name.clone())
                    .unwrap_or_default(),
                _ => String::new(),
            };
            DisplaySnap::Leaf(name)
        }
        other => DisplaySnap::Leaf(other.to_string()),
    };

    match snap {
        DisplaySnap::Leaf(s) => s,
        DisplaySnap::Str(s) => {
            if nested {
                format!("{s:?}")
            } else {
                s
            }
        }
        DisplaySnap::Seq(values) => {
            let mut parts: Vec<String> = Vec::with_capacity(values.len());
            for v in &values {
                parts.push(render_to_string(vm, *v, true, pending, results, counter));
            }
            format!("[{}]", parts.join(", "))
        }
        DisplaySnap::Entries(entries) => {
            let mut parts: Vec<String> = Vec::with_capacity(entries.len());
            for (k, v) in &entries {
                parts.push(format!(
                    "{k:?}: {}",
                    render_to_string(vm, *v, true, pending, results, counter)
                ));
            }
            format!("{{{}}}", parts.join(", "))
        }
        DisplaySnap::Instance(class_name, paired) => {
            if paired.is_empty() {
                class_name
            } else {
                let mut parts: Vec<String> = Vec::with_capacity(paired.len());
                for (name, v) in &paired {
                    parts.push(format!(
                        "{name}: {}",
                        render_to_string(vm, *v, true, pending, results, counter)
                    ));
                }
                format!("{class_name} {{ {} }}", parts.join(", "))
            }
        }
    }
}

fn deep_copy_value_recursive(
    vm: &mut BexVm,
    value: Value,
    copied_objects: &mut HashMap<HeapPtr, HeapPtr>,
) -> Value {
    match value.kind() {
        ValueKind::OmittedArg | ValueKind::Null | ValueKind::Int(_) | ValueKind::Bool(_) => value,

        ValueKind::Object(ptr) => {
            if let Some(&new_ptr) = copied_objects.get(&ptr) {
                return Value::object(new_ptr);
            }

            // Futures are *handles*, not values: a `Future` is the user-
            // visible name for an entry the engine's `FutureManager` writes
            // terminal state into. Even after the future completes, its
            // on-heap representation remains shared mutable state from the
            // runtime's point of view — there is no notion of "the same
            // future, but a copy". Short-circuit before cloning the Object
            // (which would otherwise clone the `Future` struct uselessly).
            if matches!(vm.get_object(ptr), Object::Future(_)) {
                copied_objects.insert(ptr, ptr);
                return Value::object(ptr);
            }

            let object = vm.get_object(ptr).clone();

            let new_ptr = match object {
                Object::Float(f) => vm.tlab.alloc(Object::Float(f)),
                Object::String(s) => vm.tlab.alloc(Object::String(s)),
                Object::Uint8Array(bytes) => vm.tlab.alloc(Object::Uint8Array(bytes)),

                Object::Array(values) => {
                    // A deep copy preserves the source array's element type.
                    let element_ty = values.element_ty.as_ref().clone();
                    let placeholder_ptr = vm
                        .tlab
                        .alloc(Object::Array(Array::new(element_ty.clone(), Vec::new())));
                    copied_objects.insert(ptr, placeholder_ptr);

                    // Snapshot under the source's lock; the recursive call
                    // re-enters the VM and may take other container locks.
                    let snapshot = values.to_vec();
                    let mut new_values = Vec::with_capacity(snapshot.len());
                    for value in snapshot {
                        new_values.push(deep_copy_value_recursive(vm, value, copied_objects));
                    }

                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) =
                        Object::Array(Array::new(element_ty, new_values));
                    placeholder_ptr
                }

                Object::Map(map) => {
                    // A deep copy preserves the source map's key/value types.
                    let key_ty = map.key_ty.as_ref().clone();
                    let value_ty = map.value_ty.as_ref().clone();
                    let placeholder_ptr = vm.tlab.alloc(Object::Map(Map::new(
                        key_ty.clone(),
                        value_ty.clone(),
                        IndexMap::new(),
                    )));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let snapshot = map.to_index_map();
                    let mut new_map = IndexMap::new();
                    for (key, value) in &snapshot {
                        let new_value = deep_copy_value_recursive(vm, *value, copied_objects);
                        new_map.insert(key.clone(), new_value);
                    }

                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) =
                        Object::Map(Map::new(key_ty, value_ty, new_map));
                    placeholder_ptr
                }

                Object::Instance(instance) => {
                    let placeholder_ptr = vm.tlab.alloc(Object::Instance(Instance::new(
                        instance.class,
                        instance.class_type_args.to_vec().into_boxed_slice(),
                        Vec::new(),
                    )));
                    copied_objects.insert(ptr, placeholder_ptr);

                    let mut new_fields = Vec::with_capacity(instance.fields.len());
                    for field in instance.field_values() {
                        new_fields.push(deep_copy_value_recursive(vm, field, copied_objects));
                    }

                    let new_instance = Instance::new(
                        instance.class,
                        instance.class_type_args.to_vec().into_boxed_slice(),
                        new_fields,
                    );
                    // no GC write barrier because it is all in gen0
                    *vm.get_object_mut(placeholder_ptr) = Object::Instance(new_instance);
                    placeholder_ptr
                }

                // Bigint is behind Arc — clone() is cheap (increments refcount).
                Object::Bigint(arc) => vm.tlab.alloc(Object::Bigint(std::sync::Arc::clone(&arc))),
                Object::Function(f) => vm.tlab.alloc(Object::Function(f)),
                Object::Interface(i) => vm.tlab.alloc(Object::Interface(i)),
                Object::Package(p) => vm.tlab.alloc(Object::Package(p)),
                Object::ImplRule(r) => vm.tlab.alloc(Object::ImplRule(r)),
                Object::Class(c) => vm.tlab.alloc(Object::Class(c)),
                Object::Enum(e) => vm.tlab.alloc(Object::Enum(e)),
                Object::Variant(v) => vm.tlab.alloc(Object::Variant(v)),
                Object::RustData(arc) => vm.tlab.alloc(Object::RustData(Arc::clone(&arc))),
                // `Object::Future(_)` is short-circuited above; it can't
                // reach this match arm.
                Object::Future(_) => unreachable!("Future short-circuited above"),
                Object::UnscheduledFuture(f) => vm.tlab.alloc(Object::UnscheduledFuture(f)),
                Object::Collector(c) => vm.tlab.alloc(Object::Collector(c)),
                Object::Type(ty) => vm.tlab.alloc(Object::Type(ty)),
                // Closures, bound methods, and cells are shallow-copied: the captured
                // state is shared by design (mutation semantics).
                Object::Closure(c) => vm.tlab.alloc(Object::Closure(c)),
                Object::BoundMethod(bm) => vm.tlab.alloc(Object::BoundMethod(bm)),
                Object::GenericFunction(gf) => vm.tlab.alloc(Object::GenericFunction(gf)),
                // `HostClosure` is a value-type wrapper around an
                // `Arc<HostValueArc>`; cloning the Arc is cheap and matches
                // the closure semantics (shared handle).
                Object::HostClosure(hc) => vm.tlab.alloc(Object::HostClosure(hc)),
                Object::Cell(cell) => vm.tlab.alloc(Object::Cell(cell)),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(kind) => vm.tlab.alloc(Object::Sentinel(kind)),
            };

            copied_objects.entry(ptr).or_insert(new_ptr);

            Value::object(new_ptr)
        }
    }
}

#[allow(clippy::float_cmp)]
fn deep_equals_recursive(
    vm: &BexVm,
    a: Value,
    b: Value,
    visited: &mut HashMap<(HeapPtr, HeapPtr), bool>,
) -> bool {
    match (a.kind(), b.kind()) {
        (ValueKind::OmittedArg, ValueKind::OmittedArg) => true,
        (ValueKind::Null, ValueKind::Null) => true,
        (ValueKind::Int(a), ValueKind::Int(b)) => a == b,
        (ValueKind::Bool(a), ValueKind::Bool(b)) => a == b,

        (ValueKind::Object(a_ptr), ValueKind::Object(b_ptr)) => {
            if a_ptr == b_ptr {
                return true;
            }

            let key = if a_ptr < b_ptr {
                (a_ptr, b_ptr)
            } else {
                (b_ptr, a_ptr)
            };

            if let Some(&result) = visited.get(&key) {
                return result;
            }

            visited.insert(key, true);

            let result = match (vm.get_object(a_ptr), vm.get_object(b_ptr)) {
                (Object::Float(a), Object::Float(b)) => (a.is_nan() && b.is_nan()) || a == b,
                (Object::String(a), Object::String(b)) => a == b,
                (Object::Uint8Array(a), Object::Uint8Array(b)) => {
                    let a_snap = a.to_vec();
                    let b_snap = b.to_vec();
                    a_snap == b_snap
                }

                // Different `Arc`s with the same numeric value must compare equal.
                (Object::Bigint(a), Object::Bigint(b)) => a == b,

                (Object::Array(a_values), Object::Array(b_values)) => {
                    // Snapshot under each lock before recursing; deep_equals
                    // is mutator code so we cannot hold the lock across
                    // recursive lookups that may also lock containers.
                    let a_snap = a_values.to_vec();
                    let b_snap = b_values.to_vec();
                    a_snap.len() == b_snap.len()
                        && a_snap
                            .iter()
                            .zip(b_snap.iter())
                            .all(|(a, b)| deep_equals_recursive(vm, *a, *b, visited))
                }

                (Object::Map(a_map), Object::Map(b_map)) => {
                    let a_snap = a_map.to_index_map();
                    let b_snap = b_map.to_index_map();
                    a_snap.len() == b_snap.len()
                        && a_snap.iter().all(|(key, a_val)| {
                            b_snap.get(key).is_some_and(|b_val| {
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
                            .all(|(a, b)| deep_equals_recursive(vm, a.load(), b.load(), visited))
                }

                (Object::Variant(a_var), Object::Variant(b_var)) => {
                    a_var.enm == b_var.enm && a_var.index == b_var.index
                }

                (Object::Type(a_ty), Object::Type(b_ty)) => a_ty == b_ty,

                (Object::Enum(a_enum), Object::Enum(b_enum)) => {
                    a_enum.name == b_enum.name
                        && a_enum.variants.len() == b_enum.variants.len()
                        && a_enum
                            .variants
                            .iter()
                            .zip(b_enum.variants.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Class(a_class), Object::Class(b_class)) => {
                    a_class.name == b_class.name
                        && a_class.fields.len() == b_class.fields.len()
                        && a_class
                            .fields
                            .iter()
                            .zip(b_class.fields.iter())
                            .all(|(a, b)| a.name == b.name)
                }

                (Object::Function(_), Object::Function(_)) => a_ptr == b_ptr,

                // GenericFunction values compare structurally (same base
                // function + same type args). The interned/pooled case already
                // short-circuits via the `a_ptr == b_ptr` fast path above; this
                // arm covers non-pooled copies (e.g. from `baml.deep_copy`).
                (Object::GenericFunction(a_gf), Object::GenericFunction(b_gf)) => {
                    a_gf.function == b_gf.function && a_gf.type_args == b_gf.type_args
                }

                (Object::Future(a_fut), Object::Future(b_fut)) => {
                    match (a_fut.read(), b_fut.read()) {
                        (FutureRead::Ready(a_val), FutureRead::Ready(b_val)) => {
                            deep_equals_recursive(vm, a_val, b_val, visited)
                        }
                        (FutureRead::Pending(a_id), FutureRead::Pending(b_id)) => a_id == b_id,
                        _ => false,
                    }
                }

                (Object::UnscheduledFuture(a_fut), Object::UnscheduledFuture(b_fut)) => {
                    a_fut.closure == b_fut.closure
                        && a_fut.name == b_fut.name
                        && a_fut.config == b_fut.config
                }

                _ => false,
            };

            visited.insert(key, result);
            result
        }

        _ => false,
    }
}

// ── Helpers for the numeric-array reductions ──────────────────────────────────

/// Returns a human-readable runtime type name for the `unreachable!` diagnostics
/// in `expect_float` / `expect_int`.
fn value_type_name(vm: &BexVm, value: Value) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if value.as_int().is_some() {
        return "int".to_string();
    }
    if value.as_bool().is_some() {
        return "bool".to_string();
    }
    if value.is_omitted() {
        return "omitted".to_string();
    }
    if let Some(ptr) = value.as_object_ptr() {
        return ObjectType::of(vm.get_object(ptr)).to_string();
    }
    "unknown".to_string()
}

/// Extracts a float from a validated `float[]` element. The `FloatStats` /
/// `Summable` methods are declared on `float[]`, so by the time execution reaches
/// the native path each element is a boxed float; any other tag is an upstream
/// invariant violation.
fn expect_float(vm: &BexVm, value: Value, fn_name: &str, index: usize) -> f64 {
    let Some(ptr) = value.as_object_ptr() else {
        unreachable!(
            "{fn_name}: expected float at index {index}, got {}",
            value_type_name(vm, value)
        );
    };
    match vm.get_object(ptr) {
        Object::Float(float) => *float,
        _ => unreachable!(
            "{fn_name}: expected float at index {index}, got {}",
            value_type_name(vm, value)
        ),
    }
}

/// Extracts an `i64` from a validated `int[]` element. Ints are unboxed tagged
/// values, so no heap read is needed; a missing int tag is an upstream invariant
/// violation.
fn expect_int(value: Value, fn_name: &str, index: usize) -> i64 {
    value
        .as_int()
        .unwrap_or_else(|| unreachable!("{fn_name}: expected int at index {index}"))
}

#[cfg(test)]
mod trunc_to_int_tests {
    use super::{BamlPackageBaml, PackageBamlImpl};

    /// `_trunc_to_int` must preserve the old `baml.math.trunc` semantics exactly:
    /// truncate toward zero, saturate to the `i64` range, map NaN to `0`, and
    /// never throw.
    #[test]
    fn trunc_to_int_saturating_semantics() {
        assert_eq!(PackageBamlImpl::_trunc_to_int(3.7), 3);
        assert_eq!(PackageBamlImpl::_trunc_to_int(-3.7), -3);
        assert_eq!(PackageBamlImpl::_trunc_to_int(3.0), 3);
        assert_eq!(PackageBamlImpl::_trunc_to_int(0.0), 0);
        assert_eq!(PackageBamlImpl::_trunc_to_int(-0.0), 0);
        // NaN maps to 0; ±∞ saturate to the i64 bounds (Rust's `as` cast).
        assert_eq!(PackageBamlImpl::_trunc_to_int(f64::NAN), 0);
        assert_eq!(PackageBamlImpl::_trunc_to_int(f64::INFINITY), i64::MAX);
        assert_eq!(PackageBamlImpl::_trunc_to_int(f64::NEG_INFINITY), i64::MIN);
    }
}
