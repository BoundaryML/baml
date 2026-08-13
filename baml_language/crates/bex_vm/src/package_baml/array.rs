use std::{borrow::Cow, cmp::Ordering, collections::HashMap};

use bex_heap::TlabHolder;
use bex_vm_types::{HeapPtr, Object, ObjectType, types::Value};
use num_bigint::BigInt;

use super::{ArrayView, BamlClassArray, Continuation, NativeCallResult, PackageBamlImpl};
use crate::{
    BexVm,
    array_index::{resolve_index, resolve_insert_index, resolve_slice_bound},
    errors::{VmBamlError, VmInternalError, VmRustFnError},
};

/// Equality matching BAML's `==` operator: by-value for primitives, by-content
/// for strings / uint8arrays / variants, by-`HeapPtr` reference for arrays /
/// maps / class instances. Used by `includes`, `index_of`, `last_index_of`.
fn baml_eq(vm: &BexVm, a: Value, b: Value) -> bool {
    if let (Some(la), Some(rb)) = (a.as_object_ptr(), b.as_object_ptr()) {
        // Try content comparison for the types where `==` is content-based;
        // fall through to HeapPtr equality otherwise.
        let lobj = vm.get_object(la);
        let robj = vm.get_object(rb);
        match (lobj, robj) {
            (Object::String(_), Object::String(_)) => match (vm.as_string(&a), vm.as_string(&b)) {
                (Ok(ls), Ok(rs)) => ls == rs,
                _ => false,
            },
            (Object::Uint8Array(_), Object::Uint8Array(_)) => {
                match (
                    vm.as_uint8array(&a).map(|bytes| bytes.to_vec()),
                    vm.as_uint8array(&b).map(|bytes| bytes.to_vec()),
                ) {
                    (Ok(lb), Ok(rb)) => lb == rb,
                    _ => false,
                }
            }
            (Object::Variant(lv), Object::Variant(rv)) => lv.enm == rv.enm && lv.index == rv.index,
            // Heap-boxed floats compare by content (the post-tagged-pointer
            // encoding allocates a fresh `Object::Float` per float, so two
            // semantically-equal floats land at distinct `HeapPtr`s and
            // would otherwise miss the reference-equality fallback).
            (Object::Float(lf), Object::Float(rf)) => lf == rf,
            _ => la == rb,
        }
    } else {
        a == b
    }
}

// ── GC bookkeeping helpers ────────────────────────────────────────────────────
//
// All callback-driven continuations capture an `f_ptr` (the callable) plus
// some Values they're operating on. These helpers collect the heap roots and
// apply forwarding without each continuation re-implementing the same loops.

fn collect_value_roots(values: &[Value], roots: &mut Vec<HeapPtr>) {
    for v in values {
        if let Some(p) = v.as_object_ptr() {
            roots.push(p);
        }
    }
}

fn forward_values(values: &mut [Value], forwarding: &HashMap<HeapPtr, HeapPtr>) {
    for v in values {
        if let Some(ptr) = v.as_object_ptr() {
            if let Some(&new) = forwarding.get(&ptr) {
                *v = Value::object(new);
            }
        }
    }
}

fn forward_ptr(ptr: &mut HeapPtr, forwarding: &HashMap<HeapPtr, HeapPtr>) {
    if let Some(&new) = forwarding.get(ptr) {
        *ptr = new;
    }
}

/// The result element type `U` for the `<U, E>` array transforms (`map`,
/// `flat_map`), read at dispatch time before any nested callback overwrites the
/// pending type-args.
///
/// `current_call_type_args()` is laid out `[<receiver class args>.., U, E]`: MIR
/// prepends the receiver array's element type ahead of the method's own `<U, E>`
/// (the receiver-class-type-arg prepend in `lower_call`). So `U` is *not*
/// `.first()` — that is the receiver's `T` — it is the first of the method's two
/// own generics, i.e. the second-to-last arg. Counting from the back makes it
/// correct whether or not the prepend fired (e.g. an `unknown`-typed receiver).
fn map_result_element_ty(vm: &BexVm) -> baml_type::RealizedTy {
    let type_args = vm.current_call_type_args();
    type_args
        .len()
        .checked_sub(2)
        .and_then(|u_index| type_args.get(u_index))
        .cloned()
        .unwrap_or_else(baml_type::RealizedTy::unknown)
}

/// Extract the callback `HeapPtr` from a `Value` carrying a heap
/// object, or return a type-error `NativeCallResult` for any other
/// variant.
fn extract_callable(vm: &BexVm, f: Value) -> Result<HeapPtr, NativeCallResult> {
    if let Some(p) = f.as_object_ptr() {
        Ok(p)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Object(bex_vm_types::ObjectType::Any),
            got: vm.type_of(&f),
        }))
    }
}

fn expect_bool(vm: &BexVm, value: Value) -> Result<bool, NativeCallResult> {
    if let Some(b) = value.as_bool() {
        Ok(b)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Bool,
            got: vm.type_of(&value),
        }))
    }
}

fn expect_int(vm: &BexVm, value: Value) -> Result<i64, NativeCallResult> {
    if let Some(i) = value.as_int() {
        Ok(i)
    } else {
        Err(NativeCallResult::from(VmInternalError::TypeError {
            expected: bex_vm_types::types::Type::Int,
            got: vm.type_of(&value),
        }))
    }
}

// ── Natural-order sort machinery (`baml._rust_sort` fast path) ───────────────
//
// `Sortable.sort` routes homogeneous primitive arrays (int/bigint/string/
// float) here via the `_is_primitive_array` guard; everything else goes
// through `sort_by` + `_compare_shim`. The float domain orders by
// `f64::total_cmp` — a total order over all doubles including NaN — kept
// bit-exact with `_float_total_cmp` (which backs `Comparable for float`), so
// the fast path and the comparator path can never disagree on an ordering.
// The mixed-domain / null / non-primitive rejections below are defensive:
// `_rust_sort` is only reached for arrays the type system already proved
// homogeneous primitive.

#[derive(Clone, Copy)]
pub(super) enum NaturalDomain {
    Int,
    Float,
    Bigint,
    String,
}

#[derive(Clone, Copy)]
enum NaturalKind {
    Int,
    Float,
    Bigint,
    String,
}

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

fn invalid_sort(context: &str, message: impl Into<String>) -> VmRustFnError {
    VmBamlError::InvalidArgument {
        message: format!("{context}: {}", message.into()),
    }
    .into()
}

fn natural_kind(vm: &BexVm, context: &str, value: Value) -> Result<NaturalKind, VmRustFnError> {
    if value.is_null() {
        return Err(invalid_sort(
            context,
            "natural ordering does not support null",
        ));
    }
    if value.as_int().is_some() {
        return Ok(NaturalKind::Int);
    }
    let Some(ptr) = value.as_object_ptr() else {
        return Err(invalid_sort(
            context,
            format!(
                "natural ordering does not support {}",
                value_type_name(vm, value)
            ),
        ));
    };
    match vm.get_object(ptr) {
        // NaN is *not* rejected: the float domain orders by `total_cmp`,
        // which gives NaN a defined position.
        Object::Float(_) => Ok(NaturalKind::Float),
        Object::Bigint(_) => Ok(NaturalKind::Bigint),
        Object::String(_) => Ok(NaturalKind::String),
        _ => Err(invalid_sort(
            context,
            format!(
                "natural ordering does not support {}",
                value_type_name(vm, value)
            ),
        )),
    }
}

pub(super) fn validate_natural_order_with_vm(
    vm: &BexVm,
    context: &str,
    values: &[Value],
) -> Result<NaturalDomain, VmRustFnError> {
    let mut has_int = false;
    let mut has_float = false;
    let mut has_bigint = false;
    let mut has_string = false;

    for value in values {
        match natural_kind(vm, context, *value)? {
            NaturalKind::Int => has_int = true,
            NaturalKind::Float => has_float = true,
            NaturalKind::Bigint => has_bigint = true,
            NaturalKind::String => has_string = true,
        }
    }

    let numeric_domains = usize::from(has_float) + usize::from(has_bigint);
    if has_string && (has_int || has_float || has_bigint) {
        return Err(invalid_sort(
            context,
            "natural ordering does not support mixing string with numeric values",
        ));
    }
    if has_float && has_bigint {
        return Err(invalid_sort(
            context,
            "natural ordering does not support mixing float and bigint values",
        ));
    }
    if has_string {
        Ok(NaturalDomain::String)
    } else if numeric_domains == 0 {
        Ok(NaturalDomain::Int)
    } else if has_float {
        Ok(NaturalDomain::Float)
    } else {
        Ok(NaturalDomain::Bigint)
    }
}

fn value_as_float_for_sort(vm: &BexVm, value: Value) -> f64 {
    if let Some(i) = value.as_int() {
        #[allow(clippy::cast_precision_loss)]
        return i as f64;
    }
    let ptr = value
        .as_object_ptr()
        .expect("validated float sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::Float(f) => *f,
        _ => unreachable!("validated float sort value should be float or int"),
    }
}

fn value_as_bigint_cow_for_sort(vm: &BexVm, value: Value) -> Cow<'_, BigInt> {
    if let Some(i) = value.as_int() {
        return Cow::Owned(BigInt::from(i));
    }
    let ptr = value
        .as_object_ptr()
        .expect("validated bigint sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::Bigint(arc) => Cow::Borrowed(arc.as_ref()),
        _ => unreachable!("validated bigint sort value should be bigint or int"),
    }
}

fn value_as_string_for_sort(vm: &BexVm, value: Value) -> &bex_str::BexStr {
    let ptr = value
        .as_object_ptr()
        .expect("validated string sort value should be object-backed");
    match vm.get_object(ptr) {
        Object::String(s) => s,
        _ => unreachable!("validated string sort value should be string"),
    }
}

pub(super) fn compare_natural_values(
    vm: &BexVm,
    domain: NaturalDomain,
    left: Value,
    right: Value,
) -> Ordering {
    match domain {
        NaturalDomain::Int => left
            .as_int()
            .expect("validated int sort value should be int")
            .cmp(
                &right
                    .as_int()
                    .expect("validated int sort value should be int"),
            ),
        NaturalDomain::Float => {
            value_as_float_for_sort(vm, left).total_cmp(&value_as_float_for_sort(vm, right))
        }
        NaturalDomain::Bigint => {
            value_as_bigint_cow_for_sort(vm, left).cmp(&value_as_bigint_cow_for_sort(vm, right))
        }
        NaturalDomain::String => {
            value_as_string_for_sort(vm, left).cmp(value_as_string_for_sort(vm, right))
        }
    }
}

/// Whether the first element's runtime type tag puts `values` in one of the
/// natural-sort primitive domains. Decides `Sortable.sort`'s dispatch (see
/// `baml._is_primitive_array` in `comparable.baml`). Empty → `true`.
pub(super) fn is_primitive_array_values(vm: &BexVm, values: &[Value]) -> bool {
    match values.first() {
        None => true,
        Some(v) => {
            if v.as_int().is_some() {
                true
            } else if let Some(ptr) = v.as_object_ptr() {
                matches!(
                    vm.get_object(ptr),
                    Object::Float(_) | Object::Bigint(_) | Object::String(_)
                )
            } else {
                false
            }
        }
    }
}

fn write_back_array(
    vm: &mut BexVm,
    receiver: Value,
    sorted: Vec<Value>,
) -> Result<Value, VmRustFnError> {
    {
        let mut array = vm.as_array_mut(&receiver)?;
        *array = sorted;
    }
    Ok(receiver)
}

fn write_back_array_result(
    vm: &mut BexVm,
    receiver: Value,
    sorted: Vec<Value>,
) -> NativeCallResult {
    match write_back_array(vm, receiver, sorted) {
        Ok(value) => NativeCallResult::Done(value),
        Err(e) => NativeCallResult::Error(e),
    }
}

// Boilerplate macro for the standard `Continuation` GC impls. Captures named
// fields by kind:
//   `f_ptr: <name>`         — a single `HeapPtr` field (optional).
//   `values: [<name>, ...]` — one or more `Vec<Value>` / `[Value]` fields.
macro_rules! gc_impl_array {
    (f_ptr: $f_ptr:ident, values: [$($values:ident),+ $(,)?] $(,)?) => {
        fn gc_roots(&self) -> Vec<HeapPtr> {
            let mut roots = vec![self.$f_ptr];
            $(collect_value_roots(&self.$values, &mut roots);)+
            roots
        }
        fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
            forward_ptr(&mut self.$f_ptr, forwarding);
            $(forward_values(&mut self.$values, forwarding);)+
        }
    };
    (values: [$($values:ident),+ $(,)?] $(,)?) => {
        fn gc_roots(&self) -> Vec<HeapPtr> {
            let mut roots = Vec::new();
            $(collect_value_roots(&self.$values, &mut roots);)+
            roots
        }
        fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
            $(forward_values(&mut self.$values, forwarding);)+
        }
    };
}

// ── map ───────────────────────────────────────────────────────────────────────

/// Continuation for `Array.map`. Accumulates the mapped results one element at
/// a time, yielding to the callback for each element and completing when the
/// last result has been collected.
struct MapContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    results: Vec<Value>,
    /// Result element type — the closure's return type `U` from
    /// `map<U, E>(self, f: (T) -> U) -> U[]`, captured at dispatch time.
    element_ty: baml_type::RealizedTy,
}

impl Continuation for MapContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        self.results.push(value);
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::object(
                vm.alloc_array(self.element_ty, self.results),
            ));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array, results]);
}

// ── filter ────────────────────────────────────────────────────────────────────

/// Continuation for `Array.filter`. Accumulates the original elements whose
/// predicate result is true, preserving order.
struct FilterContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    results: Vec<Value>,
    /// The receiver array's element type `T`, captured at dispatch — `filter`
    /// preserves it (`T[] -> T[]`).
    element_ty: baml_type::RealizedTy,
}

impl Continuation for FilterContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let keep = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if keep {
            self.results.push(self.array[self.idx]);
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            // Filter preserves the receiver's element type.
            return NativeCallResult::Done(Value::object(
                vm.alloc_array(self.element_ty, self.results),
            ));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array, results]);
}

// ── some / every ──────────────────────────────────────────────────────────────
//
// Both walk the array left-to-right, short-circuiting. `some` stops at the
// first true (returning true); `every` stops at the first false (returning
// false). Empty array → `some = false`, `every = true`.

struct SomeContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
}

impl Continuation for SomeContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            return NativeCallResult::Done(Value::bool(true));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::bool(false));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

struct EveryContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
}

impl Continuation for EveryContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if !b {
            return NativeCallResult::Done(Value::bool(false));
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::bool(true));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── find / find_index ─────────────────────────────────────────────────────────

struct FindContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    /// `true` to return the matching element, `false` to return its index.
    return_value: bool,
}

impl Continuation for FindContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::NULL);
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── find_last / find_last_index ───────────────────────────────────────────────

struct FindLastContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    /// Index of the element whose predicate result we await. We descend from
    /// `len - 1` toward 0; on reaching 0 with a `false` result, return null.
    idx: usize,
    /// `true` to return the matching element, `false` to return its index.
    return_value: bool,
}

impl Continuation for FindLastContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let b = match expect_bool(vm, value) {
            Ok(b) => b,
            Err(e) => return e,
        };
        if b {
            let result = if self.return_value {
                self.array[self.idx]
            } else {
                #[allow(clippy::cast_possible_wrap)]
                Value::int(self.idx as i64)
            };
            return NativeCallResult::Done(result);
        }
        if self.idx == 0 {
            return NativeCallResult::Done(Value::NULL);
        }
        self.idx -= 1;
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── reduce ────────────────────────────────────────────────────────────────────

struct ReduceContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    /// Index of the element we just paired with the accumulator and yielded
    /// to the reducer. The reducer's return value will be the new accumulator.
    idx: usize,
}

impl Continuation for ReduceContinuation {
    fn call(mut self: Box<Self>, _vm: &mut BexVm, value: Value) -> NativeCallResult {
        // `value` is the new accumulator returned by the reducer.
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(value);
        }
        let next_elem = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![value, next_elem],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array]);
}

// ── flat_map ──────────────────────────────────────────────────────────────────

struct FlatMapContinuation {
    f_ptr: HeapPtr,
    array: Vec<Value>,
    idx: usize,
    /// Flattened results accumulated so far.
    results: Vec<Value>,
    /// Result element type — the closure's element return type `U` from
    /// `flat_map<U, E>(self, f: (T) -> U[]) -> U[]`, captured at dispatch time.
    element_ty: baml_type::RealizedTy,
}

impl Continuation for FlatMapContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        // `value` is the inner array returned for `array[idx]`. Append its
        // contents to the accumulated results.
        let inner = match vm.as_array(&value) {
            Ok(slice) => slice.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        self.results.extend(inner);
        self.idx += 1;
        if self.idx >= self.array.len() {
            return NativeCallResult::Done(Value::object(
                vm.alloc_array(self.element_ty, self.results),
            ));
        }
        let next_arg = self.array[self.idx];
        NativeCallResult::YieldToCall {
            callee: self.f_ptr,
            args: vec![next_arg],
            type_args: vec![],
            continuation: self,
        }
    }
    gc_impl_array!(f_ptr: f_ptr, values: [array, results]);
}

// ─── Array.sort_by continuation ─────────────────────────────────────────────

/// CPS bottom-up merge sort. A native Rust `slice::sort_by` cannot be used here
/// because a BAML comparator may yield back into the VM. Each merge comparison
/// therefore crosses the VM trampoline, while the merge passes retain stable
/// O(n log n) behavior.
///
/// `source` is the immutable input for the current pass and `merged` receives
/// complete runs. The two vectors are swapped only after a pass completes. The
/// buffers are reused for every pass, keeping auxiliary storage at O(n). The
/// receiver is written back only once the entire sort succeeds, so a throwing
/// comparator leaves it in its pre-sort state.
struct SortByContinuation {
    receiver: Value,
    f_ptr: HeapPtr,
    source: Vec<Value>,
    merged: Vec<Value>,
    width: usize,
    run_start: usize,
    left: usize,
    middle: usize,
    right: usize,
    run_end: usize,
}

impl SortByContinuation {
    /// Advance through already-exhausted runs and yield the next comparison.
    /// Equal elements are taken from the left run by `call`, preserving the
    /// original relative order.
    fn advance(mut self: Box<Self>, vm: &mut BexVm) -> NativeCallResult {
        loop {
            if self.left < self.middle && self.right < self.run_end {
                return NativeCallResult::YieldToCall {
                    callee: self.f_ptr,
                    args: vec![self.source[self.left], self.source[self.right]],
                    type_args: vec![],
                    continuation: self,
                };
            }

            self.merged
                .extend_from_slice(&self.source[self.left..self.middle]);
            self.merged
                .extend_from_slice(&self.source[self.right..self.run_end]);

            self.run_start = self.run_end;
            if self.run_start < self.source.len() {
                self.set_current_run();
                continue;
            }

            std::mem::swap(&mut self.source, &mut self.merged);
            self.merged.clear();
            self.width = self.width.saturating_mul(2);
            if self.width >= self.source.len() {
                return write_back_array_result(vm, self.receiver, self.source);
            }

            self.run_start = 0;
            self.set_current_run();
        }
    }

    fn set_current_run(&mut self) {
        self.left = self.run_start;
        self.middle = self
            .run_start
            .saturating_add(self.width)
            .min(self.source.len());
        self.right = self.middle;
        self.run_end = self
            .middle
            .saturating_add(self.width)
            .min(self.source.len());
    }
}

impl Continuation for SortByContinuation {
    fn call(mut self: Box<Self>, vm: &mut BexVm, value: Value) -> NativeCallResult {
        let cmp = match expect_int(vm, value) {
            Ok(i) => i,
            Err(e) => return e,
        };
        if cmp <= 0 {
            self.merged.push(self.source[self.left]);
            self.left += 1;
        } else {
            self.merged.push(self.source[self.right]);
            self.right += 1;
        }
        self.advance(vm)
    }

    fn gc_roots(&self) -> Vec<HeapPtr> {
        let mut roots = vec![self.f_ptr];
        collect_value_roots(std::slice::from_ref(&self.receiver), &mut roots);
        collect_value_roots(&self.source, &mut roots);
        collect_value_roots(&self.merged, &mut roots);
        roots
    }

    fn apply_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        forward_ptr(&mut self.f_ptr, forwarding);
        forward_values(std::slice::from_mut(&mut self.receiver), forwarding);
        forward_values(&mut self.source, forwarding);
        forward_values(&mut self.merged, forwarding);
    }
}

impl BamlClassArray for PackageBamlImpl {
    #[allow(clippy::cast_possible_wrap)]
    fn length(array: ArrayView<'_>) -> i64 {
        array.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
    fn push(array: &mut Vec<Value>, item: &Value) -> i64 {
        array.push(*item);
        array.len() as i64
    }

    fn at(array: ArrayView<'_>, index: i64) -> Option<Value> {
        resolve_index(index, array.len()).map(|i| array[i])
    }

    /// Builds a new array of `length` elements, each equal to `value`.
    ///
    /// Static constructor for `baml.Array.filled`. A negative `length` clamps to
    /// an empty array. `Value` is `Copy`, so every slot shares the same `value`
    /// (for reference types, the same underlying object).
    ///
    /// The two non-positive cases are handled explicitly so they are not
    /// conflated:
    /// - A negative `length` clamps to an empty array, as documented.
    /// - A `length` that exceeds `usize::MAX` (only possible on 32-bit targets
    ///   such as `wasm32-unknown-unknown`) is requesting more elements than the
    ///   platform can address, so it saturates to `usize::MAX`. The subsequent
    ///   allocation then fails loudly rather than silently producing an empty
    ///   array.
    fn filled(length: i64, value: &Value) -> Vec<Value> {
        if length <= 0 {
            return Vec::new();
        }
        let count = usize::try_from(length).unwrap_or(usize::MAX);
        vec![*value; count]
    }

    fn concat(array: ArrayView<'_>, other: &[Value]) -> Vec<Value> {
        array.iter().chain(other.iter()).copied().collect()
    }

    fn pop(array: &mut Vec<Value>) -> Option<Value> {
        array.pop()
    }

    fn remove_at(array: &mut Vec<Value>, index: i64) -> Option<Value> {
        let index = resolve_index(index, array.len())?;
        Some(array.remove(index))
    }

    fn reverse(array: ArrayView<'_>) -> Vec<Value> {
        let mut result = array.to_vec();
        result.reverse();
        result
    }

    fn slice(array: ArrayView<'_>, start: i64, end: i64) -> Vec<Value> {
        let start = resolve_slice_bound(start, array.len());
        // An `end` resolving before `start` yields an empty slice.
        let end = resolve_slice_bound(end, array.len()).max(start);
        array[start..end].to_vec()
    }

    fn join(vm: &BexVm, array: ArrayView<'_>, separator: &bex_str::BexStr) -> bex_str::BexStr {
        let sep = separator.as_str();
        let joined = array
            .iter()
            .map(|v| {
                vm.as_string(v)
                    .map(|s| s.as_str().to_owned())
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>()
            .join(sep);
        bex_str::BexStr::from(joined)
    }

    #[allow(clippy::unused_unit)]
    fn clear(array: &mut Vec<Value>) -> () {
        array.clear();
    }

    fn shift(array: &mut Vec<Value>) -> Option<Value> {
        if array.is_empty() {
            None
        } else {
            Some(array.remove(0))
        }
    }

    #[allow(clippy::cast_possible_wrap)]
    fn unshift(array: &mut Vec<Value>, item: &Value) -> i64 {
        array.insert(0, *item);
        array.len() as i64
    }

    #[allow(clippy::cast_possible_wrap)]
    fn insert(array: &mut Vec<Value>, item: &Value, idx: i64) -> Result<i64, VmRustFnError> {
        let len = array.len();
        // A negative `idx` counts from the end; `[0, len]` is the valid range
        // (inserting at `len` appends).
        let Some(idx) = resolve_insert_index(idx, len) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.insert: idx ({idx}) is out of range for length {len}"),
            }
            .into());
        };
        array.insert(idx, *item);
        Ok(array.len() as i64)
    }

    fn splice(
        array: &mut Vec<Value>,
        start: i64,
        count: i64,
        replace: &[Value],
    ) -> Result<(), VmRustFnError> {
        let len = array.len();
        // A negative `start` counts from the end; `[0, len]` is the valid range.
        let Some(start) = resolve_insert_index(start, len) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.splice: start ({start}) is out of range for length {len}"),
            }
            .into());
        };
        let Ok(count) = usize::try_from(count) else {
            return Err(VmBamlError::InvalidArgument {
                message: format!("array.splice: count ({count}) is negative"),
            }
            .into());
        };
        let end = start.saturating_add(count).min(len);
        array.splice(start..end, replace.iter().copied());
        Ok(())
    }

    fn sort_by(vm: &mut BexVm, array: &Value, compare: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *compare) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let items = match vm.as_array(array) {
            Ok(array) => array.to_vec(),
            Err(e) => return NativeCallResult::Error(e.into()),
        };
        if items.len() <= 1 {
            return write_back_array_result(vm, *array, items);
        }
        let capacity = items.len();
        Box::new(SortByContinuation {
            receiver: *array,
            f_ptr,
            source: items,
            merged: Vec::with_capacity(capacity),
            width: 1,
            run_start: 0,
            left: 0,
            middle: 1,
            right: 1,
            run_end: 2,
        })
        .advance(vm)
    }

    fn map(vm: &mut BexVm, array: ArrayView<'_>, f: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        // `map<U, E>(self, f: (T) -> U) -> U[]`: the result element type is the
        // closure return type `U` (not the receiver's `T`).
        let element_ty = map_result_element_ty(vm);
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(element_ty, vec![])));
        }
        let first_arg = array[0];
        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(MapContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
                element_ty,
            }),
        }
    }

    fn filter(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        // `filter` preserves the receiver's element type (`T[] -> T[]`); capture
        // it before the `to_vec` below shadows the `ArrayView`.
        let element_ty = (*array.ty).clone();
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(element_ty, vec![])));
        }
        let first_arg = array[0];
        let capacity = array.len();
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FilterContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::with_capacity(capacity),
                element_ty,
            }),
        }
    }

    // ── Predicate scans ───────────────────────────────────────────────────────

    fn some(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::bool(false));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(SomeContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn every(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::bool(true));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(EveryContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn find(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindContinuation {
                f_ptr,
                array,
                idx: 0,
                return_value: true,
            }),
        }
    }

    fn find_index(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindContinuation {
                f_ptr,
                array,
                idx: 0,
                return_value: false,
            }),
        }
    }

    fn find_last(vm: &mut BexVm, array: ArrayView<'_>, predicate: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
        }
        let last_idx = array.len() - 1;
        let first_arg = array[last_idx];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindLastContinuation {
                f_ptr,
                array,
                idx: last_idx,
                return_value: true,
            }),
        }
    }

    fn find_last_index(
        vm: &mut BexVm,
        array: ArrayView<'_>,
        predicate: &Value,
    ) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *predicate) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::NULL);
        }
        let last_idx = array.len() - 1;
        let first_arg = array[last_idx];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FindLastContinuation {
                f_ptr,
                array,
                idx: last_idx,
                return_value: false,
            }),
        }
    }

    fn includes(vm: &BexVm, array: ArrayView<'_>, item: &Value) -> bool {
        array.iter().any(|v| baml_eq(vm, *v, *item))
    }

    #[allow(clippy::cast_possible_wrap)]
    fn index_of(vm: &BexVm, array: ArrayView<'_>, item: &Value) -> Option<i64> {
        array
            .iter()
            .position(|v| baml_eq(vm, *v, *item))
            .map(|i| i as i64)
    }

    #[allow(clippy::cast_possible_wrap)]
    fn last_index_of(vm: &BexVm, array: ArrayView<'_>, item: &Value) -> Option<i64> {
        array
            .iter()
            .rposition(|v| baml_eq(vm, *v, *item))
            .map(|i| i as i64)
    }

    fn reduce(
        vm: &mut BexVm,
        array: ArrayView<'_>,
        reducer: &Value,
        initial: &Value,
    ) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *reducer) {
            Ok(p) => p,
            Err(e) => return e,
        };
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(*initial);
        }
        let first_elem = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![*initial, first_elem],
            type_args: vec![],
            continuation: Box::new(ReduceContinuation {
                f_ptr,
                array,
                idx: 0,
            }),
        }
    }

    fn flat_map(vm: &mut BexVm, array: ArrayView<'_>, f: &Value) -> NativeCallResult {
        let f_ptr = match extract_callable(vm, *f) {
            Ok(p) => p,
            Err(e) => return e,
        };
        // `flat_map<U, E>(self, f: (T) -> U[]) -> U[]`: the result element type
        // is the closure's element return type `U` (not the receiver's `T`).
        let element_ty = map_result_element_ty(vm);
        let array = array.to_vec();
        if array.is_empty() {
            return NativeCallResult::Done(Value::object(vm.alloc_array(element_ty, vec![])));
        }
        let first_arg = array[0];
        NativeCallResult::YieldToCall {
            callee: f_ptr,
            args: vec![first_arg],
            type_args: vec![],
            continuation: Box::new(FlatMapContinuation {
                f_ptr,
                array,
                idx: 0,
                results: Vec::new(),
                element_ty,
            }),
        }
    }
}
